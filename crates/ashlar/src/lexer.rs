//! Lexer: turns Ashlar source text into a token stream plus diagnostics.
//!
//! Implements reference §1 (files and lexical rules) and §5 (text/number
//! literals), plus the lexer-owned rows of docs/diagnostics.md: E007 (stray
//! character), E009 (`${` in a text literal), E010 (`;`), E011 (`#`), E012
//! (raw newline in a text literal).
//!
//! Never panics: anything the reference does not define as valid input is
//! reported as a diagnostic, the offending bit is skipped or recovered from,
//! and lexing continues so the rest of the file is still available to the
//! parser.

use crate::diag::{
    Diag, Edit, Level, E007_PARSE, E009_INTERPOLATION, E010_SEMICOLON, E011_HASH_COMMENT,
    E012_NEWLINE_IN_TEXT,
};
use crate::tokens::{Pos, Span, Tok, Token};

/// Lex one file's source into tokens and diagnostics. Always returns —
/// unrecognized input never aborts the lex, it is reported and skipped.
pub fn lex(file: &str, src: &str) -> (Vec<Token>, Vec<Diag>) {
    let mut lx = Lexer {
        chars: src.chars().collect(),
        pos: 0,
        line: 1,
        col: 1,
        file: file.to_string(),
        tokens: Vec::new(),
        diags: Vec::new(),
    };
    lx.run();
    (lx.tokens, lx.diags)
}

struct Lexer {
    chars: Vec<char>,
    pos: usize,
    line: u32,
    col: u32,
    file: String,
    tokens: Vec<Token>,
    diags: Vec<Diag>,
}

impl Lexer {
    fn peek(&self, offset: usize) -> Option<char> {
        self.chars.get(self.pos + offset).copied()
    }

    /// 1-based position of the next unconsumed character.
    fn here(&self) -> Pos {
        Pos { line: self.line, col: self.col }
    }

    /// Consume and return the current character, updating line/col.
    /// Columns count Unicode scalar values (`char`s), matching the contract.
    fn advance(&mut self) -> Option<char> {
        let c = self.chars.get(self.pos).copied()?;
        self.pos += 1;
        if c == '\n' {
            self.line += 1;
            self.col = 1;
        } else {
            self.col += 1;
        }
        Some(c)
    }

    fn push(&mut self, tok: Tok, span: Span) {
        self.tokens.push(Token { tok, span });
    }

    /// Emit a `Newline` unless suppressed: no token emitted yet (file-leading
    /// newlines), the previously emitted token was already a `Newline`
    /// (collapsing runs of blank lines), or the previously emitted token
    /// cannot end an expression or declaration (Tok::Newline's contract).
    fn try_newline(&mut self, span: Span) {
        match self.tokens.last() {
            None => {}
            Some(t) if matches!(t.tok, Tok::Newline) => {}
            Some(t) if suppresses_newline(&t.tok) => {}
            _ => self.tokens.push(Token { tok: Tok::Newline, span }),
        }
    }

    /// Consume characters up to (not including) the next newline or EOF.
    /// Shared by `//` comments and the `#` recovery path.
    fn skip_to_eol(&mut self) {
        while let Some(c) = self.peek(0) {
            if c == '\n' {
                break;
            }
            self.advance();
        }
    }

    fn run(&mut self) {
        loop {
            let Some(c) = self.peek(0) else { break };
            match c {
                ' ' | '\t' | '\r' => {
                    self.advance();
                }
                '\n' => {
                    let start = self.here();
                    self.advance();
                    self.try_newline(Span::point(start.line, start.col));
                }
                '/' if self.peek(1) == Some('/') => {
                    self.advance();
                    self.advance();
                    self.skip_to_eol();
                }
                '#' => {
                    let start = self.here();
                    self.advance();
                    let span = Span::point(start.line, start.col);
                    self.diags.push(
                        Diag::new(
                            E011_HASH_COMMENT,
                            Level::Error,
                            &self.file,
                            span,
                            "`#` is not a valid comment marker.".to_string(),
                        )
                        .with_fix(
                            "Replace `#` with `//`.".to_string(),
                            vec![Edit {
                                file: self.file.clone(),
                                start: span.start,
                                end: span.end,
                                text: "//".to_string(),
                            }],
                        ),
                    );
                    // Recover as if it had been `//`: rest of the line is a comment.
                    self.skip_to_eol();
                }
                ';' => {
                    let start = self.here();
                    self.advance();
                    let span = Span::point(start.line, start.col);
                    self.diags.push(
                        Diag::new(
                            E010_SEMICOLON,
                            Level::Error,
                            &self.file,
                            span,
                            "`;` is not valid Ashlar syntax.".to_string(),
                        )
                        .with_fix(
                            "Replace the semicolon with a newline.".to_string(),
                            vec![Edit {
                                file: self.file.clone(),
                                start: span.start,
                                end: span.end,
                                text: "\n".to_string(),
                            }],
                        ),
                    );
                    // Recover as if the semicolon had already been replaced by
                    // a newline: emit one here (still subject to suppression).
                    self.try_newline(span);
                }
                '"' | '\'' => self.lex_text(c),
                c if c.is_ascii_digit() => self.lex_number(),
                c if c.is_alphabetic() || c == '_' => self.lex_ident(),
                _ => self.lex_operator_or_error(),
            }
        }
    }

    fn lex_ident(&mut self) {
        let start = self.here();
        let mut s = String::new();
        while let Some(c) = self.peek(0) {
            if c.is_alphanumeric() || c == '_' {
                s.push(c);
                self.advance();
            } else {
                break;
            }
        }
        let end = self.here();
        // `text`, `number`, `bool`, `data` are ordinary names at the lexer
        // level; the parser recognizes them only in shape positions.
        let tok = keyword(&s).unwrap_or(Tok::Ident(s));
        self.push(tok, Span { start, end });
    }

    fn lex_number(&mut self) {
        let start = self.here();
        let mut s = String::new();
        while let Some(c) = self.peek(0) {
            if c.is_ascii_digit() {
                s.push(c);
                self.advance();
            } else {
                break;
            }
        }
        // A single optional `.` fraction — only when a digit follows, so a
        // trailing `.` (e.g. member access on `42.foo`) is not swallowed and
        // a leading `.` (`.5`) is never treated as a number.
        if self.peek(0) == Some('.') && matches!(self.peek(1), Some(d) if d.is_ascii_digit()) {
            s.push('.');
            self.advance();
            while let Some(c) = self.peek(0) {
                if c.is_ascii_digit() {
                    s.push(c);
                    self.advance();
                } else {
                    break;
                }
            }
        }
        let end = self.here();
        // `s` is built from digits and at most one `.`, so this always parses.
        let val: f64 = s.parse().unwrap_or(0.0);
        self.push(Tok::Number(val), Span { start, end });
    }

    fn lex_text(&mut self, quote: char) {
        let start = self.here();
        self.advance(); // opening quote
        let mut buf = String::new();
        loop {
            match self.peek(0) {
                // Unterminated at EOF: the reference does not define this
                // case explicitly. Recover by closing the literal silently
                // with whatever content was seen, rather than inventing an
                // undocumented diagnostic id.
                None => break,
                Some(c) if c == quote => {
                    self.advance();
                    break;
                }
                Some('\n') => {
                    let at = self.here();
                    let span = Span::point(at.line, at.col);
                    self.diags.push(
                        Diag::new(
                            E012_NEWLINE_IN_TEXT,
                            Level::Error,
                            &self.file,
                            span,
                            format!(
                                "Text literal starting with `{}` contains a raw newline before it closes.",
                                quote
                            ),
                        )
                        .with_fix(
                            "Close the literal and join lines with `+`.".to_string(),
                            vec![],
                        ),
                    );
                    // Recover by closing the literal at the newline: leave
                    // the newline itself for the main loop to process.
                    break;
                }
                Some('\\') => {
                    self.advance();
                    match self.peek(0) {
                        None => {
                            // Dangling backslash at EOF: keep it literally.
                            buf.push('\\');
                            break;
                        }
                        Some(next) => {
                            match next {
                                '"' => buf.push('"'),
                                '\'' => buf.push('\''),
                                '\\' => buf.push('\\'),
                                'n' => buf.push('\n'),
                                't' => buf.push('\t'),
                                // Not one of \" \' \\ \n \t: the reference
                                // defines no other escape. Keep both
                                // characters literally rather than silently
                                // dropping the backslash.
                                other => {
                                    buf.push('\\');
                                    buf.push(other);
                                }
                            }
                            self.advance();
                        }
                    }
                }
                Some('$') if self.peek(1) == Some('{') => {
                    let at = self.here();
                    let span = Span { start: at, end: Pos { line: at.line, col: at.col + 2 } };
                    self.diags.push(
                        Diag::new(
                            E009_INTERPOLATION,
                            Level::Error,
                            &self.file,
                            span,
                            "Text literal contains `${`.".to_string(),
                        )
                        .with_fix(
                            "Ashlar has no interpolation; close the literal and join pieces with +"
                                .to_string(),
                            vec![],
                        ),
                    );
                    // No edits, no special decoding: keep lexing the literal
                    // (the `$` and, next iteration, the `{` are ordinary text).
                    buf.push('$');
                    self.advance();
                }
                Some(c) => {
                    buf.push(c);
                    self.advance();
                }
            }
        }
        let end = self.here();
        self.push(Tok::Text(buf), Span { start, end });
    }

    /// Maximal munch over the punctuation/operator set (reference §6) plus
    /// the catch-all E007 for anything else.
    fn lex_operator_or_error(&mut self) {
        let start = self.here();
        // Safe: only called right after the run() loop confirmed a char is
        // present at the current position, with nothing consumed since.
        let c = self.advance().unwrap();
        let tok = match c {
            '?' => {
                if self.peek(0) == Some('?') {
                    self.advance();
                    Tok::Coalesce
                } else {
                    Tok::Question
                }
            }
            '.' => {
                if self.peek(0) == Some('.') && self.peek(1) == Some('.') {
                    self.advance();
                    self.advance();
                    Tok::Ellipsis
                } else {
                    Tok::Dot
                }
            }
            '=' => {
                if self.peek(0) == Some('>') {
                    self.advance();
                    Tok::Arrow
                } else if self.peek(0) == Some('=') {
                    self.advance();
                    Tok::EqEq
                } else {
                    Tok::Eq
                }
            }
            '-' => {
                if self.peek(0) == Some('>') {
                    self.advance();
                    Tok::ThinArrow
                } else {
                    Tok::Minus
                }
            }
            '!' => {
                if self.peek(0) == Some('=') {
                    self.advance();
                    Tok::NotEq
                } else {
                    Tok::Bang
                }
            }
            '<' => {
                if self.peek(0) == Some('=') {
                    self.advance();
                    Tok::LtEq
                } else {
                    Tok::Lt
                }
            }
            '>' => {
                if self.peek(0) == Some('=') {
                    self.advance();
                    Tok::GtEq
                } else {
                    Tok::Gt
                }
            }
            '+' => Tok::Plus,
            '*' => Tok::Star,
            '/' => Tok::Slash,
            '%' => Tok::Percent,
            '(' => Tok::LParen,
            ')' => Tok::RParen,
            '[' => Tok::LBracket,
            ']' => Tok::RBracket,
            '{' => Tok::LBrace,
            '}' => Tok::RBrace,
            ',' => Tok::Comma,
            ':' => Tok::Colon,
            other => {
                let span = Span { start, end: self.here() };
                self.diags.push(Diag::new(
                    E007_PARSE,
                    Level::Error,
                    &self.file,
                    span,
                    format!("unexpected character `{}`.", other),
                ));
                return;
            }
        };
        let end = self.here();
        self.push(tok, Span { start, end });
    }
}

fn keyword(s: &str) -> Option<Tok> {
    Some(match s {
        "space" => Tok::KwSpace,
        "use" => Tok::KwUse,
        "part" => Tok::KwPart,
        "foreign" => Tok::KwForeign,
        "state" => Tok::KwState,
        "stored" => Tok::KwStored,
        "owned" => Tok::KwOwned,
        "append" => Tok::KwAppend,
        "deep" => Tok::KwDeep,
        "stack" => Tok::KwStack,
        "pipe" => Tok::KwPipe,
        "reverse" => Tok::KwReverse,
        "let" => Tok::KwLet,
        "if" => Tok::KwIf,
        "else" => Tok::KwElse,
        "for" => Tok::KwFor,
        "in" => Tok::KwIn,
        "return" => Tok::KwReturn,
        "true" => Tok::KwTrue,
        "false" => Tok::KwFalse,
        "none" => Tok::KwNone,
        "and" => Tok::KwAnd,
        "or" => Tok::KwOr,
        "not" => Tok::KwNot,
        _ => return None,
    })
}

/// Tokens after which a Newline is suppressed because they cannot end an
/// expression or declaration: binary operators, `=`, `=>`, `->`, `,`, `.`,
/// `:`, `??`, the three opening brackets, and the keyword operators
/// `not`/`and`/`or`. (See the Tok::Newline doc comment in tokens.rs.)
fn suppresses_newline(tok: &Tok) -> bool {
    matches!(
        tok,
        Tok::Eq
            | Tok::EqEq
            | Tok::NotEq
            | Tok::Lt
            | Tok::LtEq
            | Tok::Gt
            | Tok::GtEq
            | Tok::Plus
            | Tok::Minus
            | Tok::Star
            | Tok::Slash
            | Tok::Percent
            | Tok::Coalesce
            | Tok::Arrow
            | Tok::ThinArrow
            | Tok::Comma
            | Tok::Dot
            | Tok::Colon
            | Tok::LParen
            | Tok::LBracket
            | Tok::LBrace
            | Tok::KwNot
            | Tok::KwAnd
            | Tok::KwOr
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(toks: &[Token]) -> Vec<Tok> {
        toks.iter().map(|t| t.tok.clone()).collect()
    }

    // ---- keywords vs. plain identifiers -----------------------------------

    #[test]
    fn all_reserved_words_are_keywords() {
        let src = "space use part foreign state stored owned append deep stack pipe reverse \
                    let if else for in return true false none and or not";
        let (toks, diags) = lex("t.ash", src);
        assert!(diags.is_empty());
        assert_eq!(
            kinds(&toks),
            vec![
                Tok::KwSpace,
                Tok::KwUse,
                Tok::KwPart,
                Tok::KwForeign,
                Tok::KwState,
                Tok::KwStored,
                Tok::KwOwned,
                Tok::KwAppend,
                Tok::KwDeep,
                Tok::KwStack,
                Tok::KwPipe,
                Tok::KwReverse,
                Tok::KwLet,
                Tok::KwIf,
                Tok::KwElse,
                Tok::KwFor,
                Tok::KwIn,
                Tok::KwReturn,
                Tok::KwTrue,
                Tok::KwFalse,
                Tok::KwNone,
                Tok::KwAnd,
                Tok::KwOr,
                Tok::KwNot,
            ]
        );
    }

    #[test]
    fn shape_names_are_plain_idents_not_keywords() {
        let (toks, diags) = lex("t.ash", "text number bool data");
        assert!(diags.is_empty());
        assert_eq!(
            kinds(&toks),
            vec![
                Tok::Ident("text".into()),
                Tok::Ident("number".into()),
                Tok::Ident("bool".into()),
                Tok::Ident("data".into()),
            ]
        );
    }

    // ---- identifiers and dotted names -------------------------------------

    #[test]
    fn dotted_name_lexes_as_ident_dot_ident() {
        let (toks, diags) = lex("t.ash", "chat.ui.Message");
        assert!(diags.is_empty());
        assert_eq!(
            kinds(&toks),
            vec![
                Tok::Ident("chat".into()),
                Tok::Dot,
                Tok::Ident("ui".into()),
                Tok::Dot,
                Tok::Ident("Message".into()),
            ]
        );
    }

    #[test]
    fn identifiers_allow_underscore_and_digits_not_leading() {
        let (toks, diags) = lex("t.ash", "_x foo_bar42");
        assert!(diags.is_empty());
        assert_eq!(
            kinds(&toks),
            vec![Tok::Ident("_x".into()), Tok::Ident("foo_bar42".into())]
        );
    }

    // ---- numbers -----------------------------------------------------------

    #[test]
    fn numbers_integer_and_fraction() {
        let (toks, diags) = lex("t.ash", "42 3.5");
        assert!(diags.is_empty());
        assert_eq!(kinds(&toks), vec![Tok::Number(42.0), Tok::Number(3.5)]);
    }

    #[test]
    fn number_has_no_leading_dot() {
        let (toks, diags) = lex("t.ash", ".5");
        assert!(diags.is_empty());
        assert_eq!(kinds(&toks), vec![Tok::Dot, Tok::Number(5.0)]);
    }

    #[test]
    fn number_has_no_exponent_and_no_sign() {
        let (toks, diags) = lex("t.ash", "1e10");
        assert!(diags.is_empty());
        assert_eq!(kinds(&toks), vec![Tok::Number(1.0), Tok::Ident("e10".into())]);

        let (toks, diags) = lex("t.ash", "-1");
        assert!(diags.is_empty());
        assert_eq!(kinds(&toks), vec![Tok::Minus, Tok::Number(1.0)]);
    }

    #[test]
    fn number_stops_fraction_at_second_dot() {
        let (toks, diags) = lex("t.ash", "3.5.6");
        assert!(diags.is_empty());
        assert_eq!(
            kinds(&toks),
            vec![Tok::Number(3.5), Tok::Dot, Tok::Number(6.0)]
        );
    }

    // ---- text literals -------------------------------------------------------

    #[test]
    fn text_both_quote_styles_same_meaning() {
        let (toks, diags) = lex("t.ash", r#""hi" 'hi'"#);
        assert!(diags.is_empty());
        assert_eq!(
            kinds(&toks),
            vec![Tok::Text("hi".to_string()), Tok::Text("hi".to_string())]
        );
    }

    #[test]
    fn text_escapes_are_decoded() {
        let (toks, diags) = lex("t.ash", r#""a\"b""#);
        assert!(diags.is_empty());
        assert_eq!(kinds(&toks), vec![Tok::Text("a\"b".to_string())]);

        let (toks, diags) = lex("t.ash", r#"'a\'b'"#);
        assert!(diags.is_empty());
        assert_eq!(kinds(&toks), vec![Tok::Text("a'b".to_string())]);

        let (toks, diags) = lex("t.ash", r#""a\\b""#);
        assert!(diags.is_empty());
        assert_eq!(kinds(&toks), vec![Tok::Text(r"a\b".to_string())]);

        let (toks, diags) = lex("t.ash", r#""a\nb""#);
        assert!(diags.is_empty());
        assert_eq!(kinds(&toks), vec![Tok::Text("a\nb".to_string())]);

        let (toks, diags) = lex("t.ash", r#""a\tb""#);
        assert!(diags.is_empty());
        assert_eq!(kinds(&toks), vec![Tok::Text("a\tb".to_string())]);
    }

    #[test]
    fn text_unterminated_at_eof_closes_silently() {
        // Not defined by the reference; recovered as a best-effort close
        // with no diagnostic (see the comment in lex_text).
        let (toks, diags) = lex("t.ash", "\"abc");
        assert!(diags.is_empty());
        assert_eq!(kinds(&toks), vec![Tok::Text("abc".to_string())]);
    }

    // ---- representative composite lines -----------------------------------

    #[test]
    fn property_with_merge_kind() {
        let (toks, diags) = lex("t.ash", r#"tags append: [text] = ["core"]"#);
        assert!(diags.is_empty());
        assert_eq!(
            kinds(&toks),
            vec![
                Tok::Ident("tags".into()),
                Tok::KwAppend,
                Tok::Colon,
                Tok::LBracket,
                Tok::Ident("text".into()),
                Tok::RBracket,
                Tok::Eq,
                Tok::LBracket,
                Tok::Text("core".to_string()),
                Tok::RBracket,
            ]
        );
    }

    #[test]
    fn function_literal_with_arrow() {
        let (toks, diags) = lex("t.ash", "(at: number) => text(at)");
        assert!(diags.is_empty());
        assert_eq!(
            kinds(&toks),
            vec![
                Tok::LParen,
                Tok::Ident("at".into()),
                Tok::Colon,
                Tok::Ident("number".into()),
                Tok::RParen,
                Tok::Arrow,
                Tok::Ident("text".into()),
                Tok::LParen,
                Tok::Ident("at".into()),
                Tok::RParen,
            ]
        );
    }

    #[test]
    fn spread_in_list_literal() {
        let (toks, diags) = lex("t.ash", "[...xs, x]");
        assert!(diags.is_empty());
        assert_eq!(
            kinds(&toks),
            vec![
                Tok::LBracket,
                Tok::Ellipsis,
                Tok::Ident("xs".into()),
                Tok::Comma,
                Tok::Ident("x".into()),
                Tok::RBracket,
            ]
        );
    }

    #[test]
    fn every_operator_maximal_munch() {
        let src = "== != < <= > >= + - * / % ?? => -> ... ? ! = , . : ( ) [ ] { }";
        let (toks, diags) = lex("t.ash", src);
        assert!(diags.is_empty());
        assert_eq!(
            kinds(&toks),
            vec![
                Tok::EqEq,
                Tok::NotEq,
                Tok::Lt,
                Tok::LtEq,
                Tok::Gt,
                Tok::GtEq,
                Tok::Plus,
                Tok::Minus,
                Tok::Star,
                Tok::Slash,
                Tok::Percent,
                Tok::Coalesce,
                Tok::Arrow,
                Tok::ThinArrow,
                Tok::Ellipsis,
                Tok::Question,
                Tok::Bang,
                Tok::Eq,
                Tok::Comma,
                Tok::Dot,
                Tok::Colon,
                Tok::LParen,
                Tok::RParen,
                Tok::LBracket,
                Tok::RBracket,
                Tok::LBrace,
                Tok::RBrace,
            ]
        );
    }

    // ---- comments ------------------------------------------------------------

    #[test]
    fn line_comment_stripped_but_newline_still_terminates() {
        let (toks, diags) = lex("t.ash", "a // hi\nb");
        assert!(diags.is_empty());
        assert_eq!(
            kinds(&toks),
            vec![Tok::Ident("a".into()), Tok::Newline, Tok::Ident("b".into())]
        );
    }

    // ---- newline suppression / collapsing ------------------------------------

    #[test]
    fn newline_suppressed_after_plus_and_comma() {
        let (toks, diags) = lex("t.ash", "1 +\n2");
        assert!(diags.is_empty());
        assert_eq!(kinds(&toks), vec![Tok::Number(1.0), Tok::Plus, Tok::Number(2.0)]);

        let (toks, diags) = lex("t.ash", "f(a,\n  b)");
        assert!(diags.is_empty());
        assert_eq!(
            kinds(&toks),
            vec![
                Tok::Ident("f".into()),
                Tok::LParen,
                Tok::Ident("a".into()),
                Tok::Comma,
                Tok::Ident("b".into()),
                Tok::RParen,
            ]
        );
    }

    #[test]
    fn newline_suppressed_after_dot_colon_and_opening_brackets() {
        let (toks, diags) = lex("t.ash", "chat.\nui");
        assert!(diags.is_empty());
        assert_eq!(
            kinds(&toks),
            vec![Tok::Ident("chat".into()), Tok::Dot, Tok::Ident("ui".into())]
        );

        let (toks, diags) = lex("t.ash", "a:\nb");
        assert!(diags.is_empty());
        assert_eq!(kinds(&toks), vec![Tok::Ident("a".into()), Tok::Colon, Tok::Ident("b".into())]);

        // The lexer still emits the Newline after the Number before `]`;
        // skipping it there is the parser's job (see Tok::Newline's doc).
        let (toks, diags) = lex("t.ash", "[\n1\n]");
        assert!(diags.is_empty());
        assert_eq!(
            kinds(&toks),
            vec![Tok::LBracket, Tok::Number(1.0), Tok::Newline, Tok::RBracket]
        );
    }

    #[test]
    fn newline_suppressed_after_and_or_not() {
        let (toks, diags) = lex("t.ash", "a and\nb\n");
        assert!(diags.is_empty());
        assert_eq!(
            kinds(&toks),
            vec![Tok::Ident("a".into()), Tok::KwAnd, Tok::Ident("b".into()), Tok::Newline]
        );

        let (toks, diags) = lex("t.ash", "a or\nb");
        assert!(diags.is_empty());
        assert_eq!(kinds(&toks), vec![Tok::Ident("a".into()), Tok::KwOr, Tok::Ident("b".into())]);

        let (toks, diags) = lex("t.ash", "not\ntrue");
        assert!(diags.is_empty());
        assert_eq!(kinds(&toks), vec![Tok::KwNot, Tok::KwTrue]);
    }

    #[test]
    fn newline_collapses_blank_lines() {
        let (toks, diags) = lex("t.ash", "a\n\n\nb\n");
        assert!(diags.is_empty());
        assert_eq!(
            kinds(&toks),
            vec![Tok::Ident("a".into()), Tok::Newline, Tok::Ident("b".into()), Tok::Newline]
        );
    }

    #[test]
    fn leading_newlines_are_suppressed() {
        let (toks, diags) = lex("t.ash", "\n\na\n");
        assert!(diags.is_empty());
        assert_eq!(kinds(&toks), vec![Tok::Ident("a".into()), Tok::Newline]);
    }

    // ---- spans ----------------------------------------------------------------

    #[test]
    fn spans_are_accurate_across_lines_and_suppress_after_lbrace() {
        let src = "space chat.ui\n\npart Foo {\n}\n";
        let (toks, diags) = lex("t.ash", src);
        assert!(diags.is_empty());
        let expect = vec![
            Token { tok: Tok::KwSpace, span: Span { start: Pos { line: 1, col: 1 }, end: Pos { line: 1, col: 6 } } },
            Token { tok: Tok::Ident("chat".into()), span: Span { start: Pos { line: 1, col: 7 }, end: Pos { line: 1, col: 11 } } },
            Token { tok: Tok::Dot, span: Span { start: Pos { line: 1, col: 11 }, end: Pos { line: 1, col: 12 } } },
            Token { tok: Tok::Ident("ui".into()), span: Span { start: Pos { line: 1, col: 12 }, end: Pos { line: 1, col: 14 } } },
            Token { tok: Tok::Newline, span: Span { start: Pos { line: 1, col: 14 }, end: Pos { line: 1, col: 15 } } },
            Token { tok: Tok::KwPart, span: Span { start: Pos { line: 3, col: 1 }, end: Pos { line: 3, col: 5 } } },
            Token { tok: Tok::Ident("Foo".into()), span: Span { start: Pos { line: 3, col: 6 }, end: Pos { line: 3, col: 9 } } },
            Token { tok: Tok::LBrace, span: Span { start: Pos { line: 3, col: 10 }, end: Pos { line: 3, col: 11 } } },
            // No Newline here: suppressed right after `{`, and the blank
            // line 2 collapsed into the single Newline emitted above.
            Token { tok: Tok::RBrace, span: Span { start: Pos { line: 4, col: 1 }, end: Pos { line: 4, col: 2 } } },
            Token { tok: Tok::Newline, span: Span { start: Pos { line: 4, col: 2 }, end: Pos { line: 4, col: 3 } } },
        ];
        assert_eq!(toks, expect);
    }

    // ---- diagnostics: E007, E009, E010, E011, E012 -----------------------------

    #[test]
    fn e007_unexpected_character_is_skipped() {
        let (toks, diags) = lex("t.ash", "a $ b");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].id, "E007");
        assert_eq!(diags[0].cause, "unexpected character `$`.");
        assert_eq!(
            kinds(&toks),
            vec![Tok::Ident("a".into()), Tok::Ident("b".into())]
        );
    }

    #[test]
    fn e007_never_panics_on_a_run_of_garbage() {
        let (toks, diags) = lex("t.ash", "@ ` ~ \\");
        assert!(toks.is_empty());
        assert_eq!(diags.len(), 4);
        assert!(diags.iter().all(|d| d.id == "E007"));
    }

    #[test]
    fn e009_interpolation_flagged_and_lexing_continues() {
        let (toks, diags) = lex("t.ash", "\"a${b}\"");
        assert_eq!(diags.len(), 1);
        let d = &diags[0];
        assert_eq!(d.id, "E009");
        assert_eq!(d.cause, "Text literal contains `${`.");
        let fix = d.fix.as_ref().expect("E009 carries a fix note");
        assert!(fix.edits.is_empty());
        assert_eq!(kinds(&toks), vec![Tok::Text("a${b}".to_string())]);
    }

    #[test]
    fn e010_semicolon_id_and_newline_replacement_edit() {
        let (toks, diags) = lex("t.ash", "a; b");
        assert_eq!(diags.len(), 1);
        let d = &diags[0];
        assert_eq!(d.id, "E010");
        let fix = d.fix.as_ref().expect("E010 carries a fix");
        assert_eq!(
            fix.edits,
            vec![Edit {
                file: "t.ash".to_string(),
                start: Pos { line: 1, col: 2 },
                end: Pos { line: 1, col: 3 },
                text: "\n".to_string(),
            }]
        );
        // Recovery: a Newline stands in for the semicolon so parsing
        // continues as if it had already been fixed.
        assert_eq!(
            kinds(&toks),
            vec![Tok::Ident("a".into()), Tok::Newline, Tok::Ident("b".into())]
        );
    }

    #[test]
    fn e011_hash_id_and_comment_replacement_edit() {
        let (toks, diags) = lex("t.ash", "a #hi\nb");
        assert_eq!(diags.len(), 1);
        let d = &diags[0];
        assert_eq!(d.id, "E011");
        let fix = d.fix.as_ref().expect("E011 carries a fix");
        assert_eq!(
            fix.edits,
            vec![Edit {
                file: "t.ash".to_string(),
                start: Pos { line: 1, col: 3 },
                end: Pos { line: 1, col: 4 },
                text: "//".to_string(),
            }]
        );
        // Recovery: `hi` was swallowed as a comment, and the newline that
        // follows terminates the statement normally.
        assert_eq!(
            kinds(&toks),
            vec![Tok::Ident("a".into()), Tok::Newline, Tok::Ident("b".into())]
        );
    }

    #[test]
    fn e012_raw_newline_closes_literal_and_recovers() {
        let (toks, diags) = lex("t.ash", "\"a\nb\"");
        assert_eq!(diags.len(), 1);
        let d = &diags[0];
        assert_eq!(d.id, "E012");
        let fix = d.fix.as_ref().expect("E012 carries a fix note");
        assert!(fix.edits.is_empty());
        // Recovery: the literal closes at the newline (`Text("a")`), the
        // newline itself terminates the statement, then lexing resumes
        // normally on the next line (`b` then a fresh, EOF-unterminated
        // literal that closes empty).
        assert_eq!(
            kinds(&toks),
            vec![
                Tok::Text("a".to_string()),
                Tok::Newline,
                Tok::Ident("b".into()),
                Tok::Text(String::new()),
            ]
        );
    }

    #[test]
    fn e010_and_e011_do_not_duplicate_newlines_on_repeats() {
        // Two semicolons in a row: only one recovery Newline (collapsing),
        // but each semicolon still gets its own diagnostic.
        let (toks, diags) = lex("t.ash", "a;; b");
        assert_eq!(diags.len(), 2);
        assert!(diags.iter().all(|d| d.id == "E010"));
        assert_eq!(
            kinds(&toks),
            vec![Tok::Ident("a".into()), Tok::Newline, Tok::Ident("b".into())]
        );
    }
}
