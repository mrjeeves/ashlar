//! CONTRACT FILE — owned by the integrator. Module implementors: do not edit.
//! Token model shared by the lexer (producer) and parser (consumer).

/// 1-based line/column position. Columns count Unicode scalar values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Pos {
    pub line: u32,
    pub col: u32,
}

/// Half-open span: `start` inclusive, `end` exclusive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Span {
    pub start: Pos,
    pub end: Pos,
}

impl Span {
    pub fn point(line: u32, col: u32) -> Span {
        Span {
            start: Pos { line, col },
            end: Pos { line, col: col + 1 },
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Tok {
    /// Identifier or shape-context name (`text`, `number`, `bool`, `data`
    /// lex as plain `Ident`; the parser recognizes them in shape positions).
    Ident(String),
    Number(f64),
    /// Decoded text literal (escapes processed, quotes stripped).
    Text(String),

    // Reserved words (reference §1).
    KwSpace,
    KwUse,
    KwPart,
    KwForeign,
    KwState,
    KwStored,
    KwSynced,
    KwAppend,
    KwDeep,
    KwStack,
    KwPipe,
    KwReverse,
    KwLet,
    KwIf,
    KwElse,
    KwFor,
    KwIn,
    KwReturn,
    KwTrue,
    KwFalse,
    KwNone,
    KwAnd,
    KwOr,
    KwNot,

    // Punctuation and operators.
    LBrace,
    RBrace,
    LParen,
    RParen,
    LBracket,
    RBracket,
    Comma,
    Colon,
    Dot,
    /// `...` spread.
    Ellipsis,
    /// `?` optional-shape marker.
    Question,
    /// `!` postfix non-none assertion.
    Bang,
    Eq,
    EqEq,
    NotEq,
    Lt,
    LtEq,
    Gt,
    GtEq,
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    /// `??` none-coalescing.
    Coalesce,
    /// `=>` function literal arrow.
    Arrow,
    /// `->` function shape arrow.
    ThinArrow,

    /// Statement terminator. The lexer collapses consecutive newlines into
    /// one `Newline` token, and suppresses the token entirely when the
    /// previously emitted token cannot end an expression or declaration:
    /// after any operator, `=`, `=>`, `->`, `,`, `.`, `:`, `??`, `(`, `[`,
    /// or `{`. The parser additionally skips `Newline`s freely right after
    /// `{`/`(`/`[` and right before `}`/`)`/`]`.
    Newline,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub tok: Tok,
    pub span: Span,
}
