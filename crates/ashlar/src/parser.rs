//! Parser: recursive descent over the lexer's token stream, producing
//! `ast::SrcFile` per reference §1–§7.
//!
//! Newline discipline: the lexer already collapses newline runs and
//! suppresses `Newline` after tokens that cannot end a declaration or
//! expression. On top of that, this parser skips newlines freely right
//! after `{`, `(`, `[` and right before `}`, `)`, `]`, so multi-line
//! literals and parameter lists need no continuation syntax.
//!
//! Never panics: every unexpected token becomes a diagnostic and a
//! bounded recovery skip (to the next newline at the current bracket
//! depth inside a part or block; to the next top-level declaration at
//! file scope). At most one diagnostic is emitted per skip.

use crate::ast::{
    Expr, FnBody, ForeignDecl, KindDecl, ListItem, MapItem, MergeKind, Name, Param, PartDecl,
    Prop, SExpr, SShape, Shape, SrcFile, Stmt, Storage, UnOp,
};
use crate::diag::{
    Diag, Edit, Level, E007_PARSE, E016_RESERVED_WORD, E018_FOREIGN_TOPLEVEL, E020_BAD_REVERSE,
    E022_SPACE_HEADER, E023_FOREIGN_STMT,
};
use crate::tokens::{Pos, Span, Tok, Token};

/// Parse one file's tokens. `None` means the file had no usable `space`
/// header (E022) and contributes nothing downstream; diagnostics are
/// returned either way.
pub fn parse(file: &str, tokens: &[Token]) -> (Option<SrcFile>, Vec<Diag>) {
    let mut p = Parser {
        file: file.to_string(),
        toks: tokens,
        pos: 0,
        diags: Vec::new(),
    };
    let out = p.parse_file();
    (out, p.diags)
}

/// Top-level identifiers from neighboring languages (E018).
const FOREIGN_TOPLEVEL: &[&str] = &[
    "import", "from", "export", "class", "function", "def", "struct", "interface", "enum",
    "mod", "package",
];

/// Statement-leading identifiers from neighboring languages (E023).
const FOREIGN_STMT: &[&str] = &[
    "while", "switch", "match", "try", "catch", "throw", "finally", "do", "var", "const", "elif",
];

struct Parser<'a> {
    file: String,
    toks: &'a [Token],
    pos: usize,
    diags: Vec<Diag>,
}

impl<'a> Parser<'a> {
    // -- cursor helpers -----------------------------------------------------

    fn cur(&self) -> Option<&Tok> {
        self.toks.get(self.pos).map(|t| &t.tok)
    }

    fn nth(&self, n: usize) -> Option<&Tok> {
        self.toks.get(self.pos + n).map(|t| &t.tok)
    }

    fn at(&self, t: &Tok) -> bool {
        self.cur() == Some(t)
    }

    fn bump(&mut self) {
        if self.pos < self.toks.len() {
            self.pos += 1;
        }
    }

    /// Span of the current token, or a point just past the last token at EOF.
    fn here_span(&self) -> Span {
        match self.toks.get(self.pos) {
            Some(t) => t.span,
            None => match self.toks.last() {
                Some(t) => Span {
                    start: t.span.end,
                    end: Pos {
                        line: t.span.end.line,
                        col: t.span.end.col + 1,
                    },
                },
                None => Span::point(1, 1),
            },
        }
    }

    /// End position of the most recently consumed token.
    fn prev_end(&self) -> Pos {
        if self.pos > 0 {
            self.toks[self.pos - 1].span.end
        } else {
            Pos { line: 1, col: 1 }
        }
    }

    fn span_from(&self, start: Pos) -> Span {
        Span {
            start,
            end: self.prev_end(),
        }
    }

    /// Consume the current token if it equals `t`; return its span.
    fn eat(&mut self, t: &Tok) -> Option<Span> {
        if self.at(t) {
            let sp = self.here_span();
            self.bump();
            Some(sp)
        } else {
            None
        }
    }

    fn skip_newlines(&mut self) {
        while self.at(&Tok::Newline) {
            self.bump();
        }
    }

    // -- diagnostics --------------------------------------------------------

    fn err(&mut self, span: Span, cause: String) {
        self.diags
            .push(Diag::new(E007_PARSE, Level::Error, &self.file, span, cause));
    }

    fn err_expected(&mut self, what: &str) {
        let span = self.here_span();
        let found = describe(self.cur());
        self.err(span, format!("expected {}, found {}.", what, found));
    }

    // -- recovery -----------------------------------------------------------

    /// Consume tokens up to and including the next `Newline` at the current
    /// bracket depth. Stops (without consuming) before a `}` that would
    /// close the enclosing block, and at EOF. Always makes progress when
    /// not already at such a stopping point.
    fn skip_to_newline_at_depth(&mut self) {
        let mut depth: i32 = 0;
        while let Some(t) = self.cur() {
            match t {
                Tok::LBrace | Tok::LParen | Tok::LBracket => depth += 1,
                Tok::RBrace if depth == 0 => {
                    return; // enclosing block's closer: leave it for the caller
                }
                Tok::RBrace | Tok::RParen | Tok::RBracket => {
                    // A stray `)`/`]` at depth 0 is junk being skipped, not
                    // an enclosing closer — consume it and keep scanning.
                    if depth > 0 {
                        depth -= 1;
                    }
                }
                Tok::Newline if depth == 0 => {
                    self.bump();
                    return;
                }
                _ => {}
            }
            self.bump();
        }
    }

    /// Consume tokens until the next top-level declaration keyword or EOF.
    fn skip_to_top_decl(&mut self) {
        let mut depth: i32 = 0;
        // Always consume at least the offending token.
        if let Some(t) = self.cur() {
            if matches!(t, Tok::LBrace | Tok::LParen | Tok::LBracket) {
                depth += 1;
            }
            self.bump();
        }
        while let Some(t) = self.cur() {
            match t {
                Tok::LBrace | Tok::LParen | Tok::LBracket => depth += 1,
                Tok::RBrace | Tok::RParen | Tok::RBracket => depth -= 1,
                Tok::KwPart | Tok::KwForeign | Tok::KwUse | Tok::KwSpace if depth <= 0 => return,
                _ => {}
            }
            self.bump();
        }
    }

    /// Statement/declaration terminator: consume a `Newline`, accept an
    /// upcoming `}` or EOF as-is, otherwise E007 and recover.
    fn line_end(&mut self, ctx: &str) -> bool {
        match self.cur() {
            Some(Tok::Newline) => {
                self.bump();
                true
            }
            Some(Tok::RBrace) | None => true,
            _ => {
                self.err_expected(&format!("a line break after {}", ctx));
                self.skip_to_newline_at_depth();
                false
            }
        }
    }

    // -- names --------------------------------------------------------------

    /// One identifier in a name position. Reserved words are E016; anything
    /// else is E007. Consumes the token only when it is an identifier or a
    /// reserved word (to guarantee progress).
    fn name_ident(&mut self) -> Option<(String, Span)> {
        let span = self.here_span();
        match self.cur().cloned() {
            Some(Tok::Ident(s)) => {
                self.bump();
                Some((s, span))
            }
            Some(t) => {
                if let Some(kw) = kw_text(&t) {
                    self.diags.push(
                        Diag::new(
                            E016_RESERVED_WORD,
                            Level::Error,
                            &self.file,
                            span,
                            format!("`{}` is a reserved word and cannot be a name.", kw),
                        )
                        .with_fix("Pick a name that is not a reserved word.".to_string(), vec![]),
                    );
                    self.bump();
                } else {
                    self.err_expected("a name");
                }
                None
            }
            None => {
                self.err_expected("a name");
                None
            }
        }
    }

    /// Dotted name: `Ident ("." Ident)*`.
    fn dotted_name(&mut self) -> Option<(Name, Span)> {
        let (first, first_span) = self.name_ident()?;
        let start = first_span.start;
        let mut segs = vec![first];
        while self.at(&Tok::Dot) && matches!(self.nth(1), Some(Tok::Ident(_))) {
            self.bump(); // dot
            if let Some(Tok::Ident(s)) = self.cur().cloned() {
                segs.push(s);
                self.bump();
            }
        }
        Some((segs, self.span_from(start)))
    }

    // -- file level ---------------------------------------------------------

    fn parse_file(&mut self) -> Option<SrcFile> {
        self.skip_newlines();
        if self.eat(&Tok::KwSpace).is_none() {
            let span = self.here_span();
            self.diags.push(
                Diag::new(
                    E022_SPACE_HEADER,
                    Level::Error,
                    &self.file,
                    span,
                    "a file must start with a `space` declaration.".to_string(),
                )
                .with_fix("Add `space <name>` as the first line.".to_string(), vec![]),
            );
            return None;
        }
        let (space, space_span) = self.dotted_name()?;
        self.line_end("the `space` name");

        let mut uses: Vec<(Name, Span)> = Vec::new();
        let mut parts: Vec<PartDecl> = Vec::new();
        let mut foreigns: Vec<ForeignDecl> = Vec::new();

        loop {
            self.skip_newlines();
            let Some(tok) = self.cur().cloned() else { break };
            match tok {
                Tok::KwUse => {
                    let kw_span = self.here_span();
                    self.bump();
                    if !(parts.is_empty() && foreigns.is_empty()) {
                        self.diags.push(
                            Diag::new(
                                E022_SPACE_HEADER,
                                Level::Error,
                                &self.file,
                                kw_span,
                                "`use` must come before any `part` or `foreign` declaration."
                                    .to_string(),
                            )
                            .with_fix(
                                "Move this `use` line up, directly under the `space` line."
                                    .to_string(),
                                vec![],
                            ),
                        );
                    }
                    if let Some((name, span)) = self.dotted_name() {
                        uses.push((name, span));
                        self.line_end("the `use` target");
                    } else {
                        self.skip_to_newline_at_depth();
                    }
                }
                Tok::KwSpace => {
                    let span = self.here_span();
                    self.bump();
                    self.diags.push(
                        Diag::new(
                            E022_SPACE_HEADER,
                            Level::Error,
                            &self.file,
                            span,
                            "only one `space` declaration is allowed per file.".to_string(),
                        )
                        .with_fix(
                            "Move these declarations to their own file with this header."
                                .to_string(),
                            vec![],
                        ),
                    );
                    let _ = self.dotted_name();
                    self.line_end("the `space` name");
                }
                Tok::KwPart => {
                    if let Some(pd) = self.parse_part() {
                        parts.push(pd);
                    }
                }
                Tok::KwForeign => {
                    if let Some(fd) = self.parse_foreign() {
                        foreigns.push(fd);
                    }
                }
                Tok::Ident(ref name) if FOREIGN_TOPLEVEL.contains(&name.as_str()) => {
                    let span = self.here_span();
                    let note = match name.as_str() {
                        "import" | "from" | "export" => {
                            "Dependencies are declared with `use <space>`; there is no import list."
                        }
                        _ => "Declare a `part` instead; parts are the only unit of composition.",
                    };
                    self.diags.push(
                        Diag::new(
                            E018_FOREIGN_TOPLEVEL,
                            Level::Error,
                            &self.file,
                            span,
                            format!("`{}` is not an Ashlar declaration.", name),
                        )
                        .with_fix(note.to_string(), vec![]),
                    );
                    self.skip_to_top_decl();
                }
                _ => {
                    self.err_expected("`part`, `foreign`, or `use`");
                    self.skip_to_top_decl();
                }
            }
        }

        Some(SrcFile {
            space,
            space_span,
            uses,
            parts,
            foreigns,
        })
    }

    // -- parts and properties ------------------------------------------------

    fn parse_part(&mut self) -> Option<PartDecl> {
        self.bump(); // `part`
        let Some((name, name_span)) = self.dotted_name() else {
            self.skip_to_top_decl();
            return None;
        };
        if self.eat(&Tok::LBrace).is_none() {
            self.err_expected("`{` after the part name");
            self.skip_to_top_decl();
            return None;
        }
        let mut props = Vec::new();
        loop {
            self.skip_newlines();
            if self.eat(&Tok::RBrace).is_some() {
                break;
            }
            if self.cur().is_none() {
                self.err_expected("`}` to close the part");
                break;
            }
            match self.parse_prop() {
                Some(p) => props.push(p),
                None => self.skip_to_newline_at_depth(),
            }
        }
        self.line_end("the part's closing `}`");
        Some(PartDecl {
            name,
            name_span,
            props,
        })
    }

    /// `[storage] name [kind [reverse]] [":" shape] ["=" expr]`
    fn parse_prop(&mut self) -> Option<Prop> {
        let storage: Option<(Storage, Span)> = match self.cur() {
            Some(Tok::KwState) => {
                let sp = self.here_span();
                self.bump();
                Some((Storage::State, sp))
            }
            Some(Tok::KwStored) => {
                let sp = self.here_span();
                self.bump();
                Some((Storage::Stored, sp))
            }
            Some(Tok::KwSynced) => {
                let sp = self.here_span();
                self.bump();
                Some((Storage::Synced, sp))
            }
            _ => None,
        };

        let (name, name_span) = self.name_ident()?;

        let mut kind: Option<KindDecl> = None;
        let mk = match self.cur() {
            Some(Tok::KwAppend) => Some(MergeKind::Append),
            Some(Tok::KwDeep) => Some(MergeKind::Deep),
            Some(Tok::KwStack) => Some(MergeKind::Stack),
            Some(Tok::KwPipe) => Some(MergeKind::Pipe),
            _ => None,
        };
        if let Some(k) = mk {
            let kspan = self.here_span();
            self.bump();
            let mut reverse = false;
            let mut span = kspan;
            if self.at(&Tok::KwReverse) {
                let rspan = self.here_span();
                if matches!(k, MergeKind::Stack | MergeKind::Pipe) {
                    reverse = true;
                    span = Span {
                        start: kspan.start,
                        end: rspan.end,
                    };
                    self.bump();
                } else {
                    self.diags.push(
                        Diag::new(
                            E020_BAD_REVERSE,
                            Level::Error,
                            &self.file,
                            rspan,
                            "`reverse` applies only to `stack` and `pipe` properties.".to_string(),
                        )
                        .with_fix(
                            "Delete `reverse`.".to_string(),
                            vec![Edit {
                                file: self.file.clone(),
                                start: rspan.start,
                                end: rspan.end,
                                text: String::new(),
                            }],
                        ),
                    );
                    self.bump();
                }
            }
            kind = Some(KindDecl {
                kind: k,
                reverse,
                span,
            });
        } else if self.at(&Tok::KwReverse) {
            let rspan = self.here_span();
            self.diags.push(
                Diag::new(
                    E020_BAD_REVERSE,
                    Level::Error,
                    &self.file,
                    rspan,
                    "`reverse` requires a merge kind of `stack` or `pipe`.".to_string(),
                )
                .with_fix(
                    "Delete `reverse`.".to_string(),
                    vec![Edit {
                        file: self.file.clone(),
                        start: rspan.start,
                        end: rspan.end,
                        text: String::new(),
                    }],
                ),
            );
            self.bump();
        }

        let shape = if self.eat(&Tok::Colon).is_some() {
            Some(self.parse_shape()?)
        } else {
            None
        };

        let value = if self.eat(&Tok::Eq).is_some() {
            Some(self.parse_expr()?)
        } else {
            None
        };

        if shape.is_none() && value.is_none() {
            self.diags.push(
                Diag::new(
                    E007_PARSE,
                    Level::Error,
                    &self.file,
                    name_span,
                    "a property needs a shape, a value, or both.".to_string(),
                )
                .with_fix(
                    format!("Add `: <shape>` or `= <value>` after `{}`.", name),
                    vec![],
                ),
            );
        }

        if !self.line_end("the property") {
            // Diagnostic already emitted; the property itself is still usable.
        }

        Some(Prop {
            name,
            name_span,
            storage,
            kind,
            shape,
            value,
        })
    }

    fn parse_foreign(&mut self) -> Option<ForeignDecl> {
        self.bump(); // `foreign`
        let (name, name_span) = match self.name_ident() {
            Some(n) => n,
            None => {
                self.skip_to_top_decl();
                return None;
            }
        };
        if self.eat(&Tok::Colon).is_none() {
            self.err_expected("`:` after the foreign name");
            self.skip_to_top_decl();
            return None;
        }
        let shape = match self.parse_shape() {
            Some(s) => s,
            None => {
                self.skip_to_newline_at_depth();
                return None;
            }
        };
        let decl = match shape.shape {
            Shape::Fn(params, ret) => ForeignDecl {
                name,
                name_span,
                params,
                ret: *ret,
            },
            _ => {
                self.err(
                    shape.span,
                    "a foreign declaration needs a function shape like `(text) -> data`."
                        .to_string(),
                );
                return None;
            }
        };
        self.line_end("the foreign declaration");
        Some(decl)
    }

    // -- shapes ---------------------------------------------------------------

    fn parse_shape(&mut self) -> Option<SShape> {
        let start = self.here_span().start;
        let base: Shape = match self.cur().cloned() {
            Some(Tok::Ident(s)) => match s.as_str() {
                "text" => {
                    self.bump();
                    Shape::Text
                }
                "number" => {
                    self.bump();
                    Shape::Number
                }
                "bool" => {
                    self.bump();
                    Shape::Bool
                }
                "data" => {
                    self.bump();
                    Shape::Data
                }
                _ => {
                    let (name, _) = self.dotted_name()?;
                    Shape::Part(name)
                }
            },
            Some(Tok::LBracket) => {
                self.bump();
                self.skip_newlines();
                let inner = self.parse_shape()?;
                self.skip_newlines();
                if self.eat(&Tok::RBracket).is_none() {
                    self.err_expected("`]` to close the list shape");
                    return None;
                }
                Shape::List(Box::new(inner))
            }
            Some(Tok::LBrace) => {
                self.bump();
                self.skip_newlines();
                let inner = self.parse_shape()?;
                self.skip_newlines();
                if self.eat(&Tok::RBrace).is_none() {
                    self.err_expected("`}` to close the map shape");
                    return None;
                }
                Shape::Map(Box::new(inner))
            }
            Some(Tok::LParen) => {
                self.bump();
                self.skip_newlines();
                let mut params: Vec<(Option<String>, SShape)> = Vec::new();
                if !self.at(&Tok::RParen) {
                    loop {
                        self.skip_newlines();
                        let named = matches!(self.cur(), Some(Tok::Ident(_)))
                            && self.nth(1) == Some(&Tok::Colon);
                        let pname = if named {
                            let (n, _) = self.name_ident()?;
                            self.bump(); // colon
                            Some(n)
                        } else {
                            None
                        };
                        let pshape = self.parse_shape()?;
                        params.push((pname, pshape));
                        self.skip_newlines();
                        if self.eat(&Tok::Comma).is_some() {
                            self.skip_newlines();
                            if self.at(&Tok::RParen) {
                                break;
                            }
                        } else {
                            break;
                        }
                    }
                }
                if self.eat(&Tok::RParen).is_none() {
                    self.err_expected("`)` to close the parameter shapes");
                    return None;
                }
                if self.eat(&Tok::ThinArrow).is_none() {
                    self.err_expected("`->` and a return shape");
                    return None;
                }
                let ret = self.parse_shape()?;
                Shape::Fn(params, Box::new(ret))
            }
            _ => {
                self.err_expected("a shape");
                return None;
            }
        };
        let mut sh = SShape {
            shape: base,
            span: self.span_from(start),
        };
        if self.eat(&Tok::Question).is_some() {
            sh = SShape {
                shape: Shape::Opt(Box::new(sh)),
                span: self.span_from(start),
            };
        }
        Some(sh)
    }

    // -- expressions ----------------------------------------------------------
    //
    // Precedence, loosest to tightest (reference §6):
    //   or | and | not | == != | < <= > >= | ?? | + - | * / % | prefix -
    //   | postfix (! .field [index] (call)) | primary

    fn parse_expr(&mut self) -> Option<SExpr> {
        self.parse_or()
    }

    fn bin(&self, op: crate::ast::BinOp, l: SExpr, r: SExpr) -> SExpr {
        let span = Span {
            start: l.span.start,
            end: r.span.end,
        };
        SExpr {
            expr: Expr::Binary(op, Box::new(l), Box::new(r)),
            span,
        }
    }

    fn parse_or(&mut self) -> Option<SExpr> {
        let mut l = self.parse_and()?;
        while self.eat(&Tok::KwOr).is_some() {
            let r = self.parse_and()?;
            l = self.bin(crate::ast::BinOp::Or, l, r);
        }
        Some(l)
    }

    fn parse_and(&mut self) -> Option<SExpr> {
        let mut l = self.parse_not()?;
        while self.eat(&Tok::KwAnd).is_some() {
            let r = self.parse_not()?;
            l = self.bin(crate::ast::BinOp::And, l, r);
        }
        Some(l)
    }

    fn parse_not(&mut self) -> Option<SExpr> {
        if self.at(&Tok::KwNot) {
            let start = self.here_span().start;
            self.bump();
            let e = self.parse_not()?;
            let span = self.span_from(start);
            return Some(SExpr {
                expr: Expr::Unary(UnOp::Not, Box::new(e)),
                span,
            });
        }
        self.parse_eq()
    }

    fn parse_eq(&mut self) -> Option<SExpr> {
        let mut l = self.parse_cmp()?;
        loop {
            let op = match self.cur() {
                Some(Tok::EqEq) => crate::ast::BinOp::EqEq,
                Some(Tok::NotEq) => crate::ast::BinOp::NotEq,
                _ => break,
            };
            self.bump();
            let r = self.parse_cmp()?;
            l = self.bin(op, l, r);
        }
        Some(l)
    }

    fn parse_cmp(&mut self) -> Option<SExpr> {
        let mut l = self.parse_coalesce()?;
        loop {
            let op = match self.cur() {
                Some(Tok::Lt) => crate::ast::BinOp::Lt,
                Some(Tok::LtEq) => crate::ast::BinOp::LtEq,
                Some(Tok::Gt) => crate::ast::BinOp::Gt,
                Some(Tok::GtEq) => crate::ast::BinOp::GtEq,
                _ => break,
            };
            self.bump();
            let r = self.parse_coalesce()?;
            l = self.bin(op, l, r);
        }
        Some(l)
    }

    fn parse_coalesce(&mut self) -> Option<SExpr> {
        let mut l = self.parse_add()?;
        while self.eat(&Tok::Coalesce).is_some() {
            let r = self.parse_add()?;
            l = self.bin(crate::ast::BinOp::Coalesce, l, r);
        }
        Some(l)
    }

    fn parse_add(&mut self) -> Option<SExpr> {
        let mut l = self.parse_mul()?;
        loop {
            let op = match self.cur() {
                Some(Tok::Plus) => crate::ast::BinOp::Add,
                Some(Tok::Minus) => crate::ast::BinOp::Sub,
                _ => break,
            };
            self.bump();
            let r = self.parse_mul()?;
            l = self.bin(op, l, r);
        }
        Some(l)
    }

    fn parse_mul(&mut self) -> Option<SExpr> {
        let mut l = self.parse_unary()?;
        loop {
            let op = match self.cur() {
                Some(Tok::Star) => crate::ast::BinOp::Mul,
                Some(Tok::Slash) => crate::ast::BinOp::Div,
                Some(Tok::Percent) => crate::ast::BinOp::Rem,
                _ => break,
            };
            self.bump();
            let r = self.parse_unary()?;
            l = self.bin(op, l, r);
        }
        Some(l)
    }

    fn parse_unary(&mut self) -> Option<SExpr> {
        if self.at(&Tok::Minus) {
            let start = self.here_span().start;
            self.bump();
            let e = self.parse_unary()?;
            let span = self.span_from(start);
            return Some(SExpr {
                expr: Expr::Unary(UnOp::Neg, Box::new(e)),
                span,
            });
        }
        self.parse_postfix()
    }

    fn parse_postfix(&mut self) -> Option<SExpr> {
        let mut e = self.parse_primary()?;
        loop {
            match self.cur() {
                Some(Tok::Bang) => {
                    self.bump();
                    let span = Span {
                        start: e.span.start,
                        end: self.prev_end(),
                    };
                    e = SExpr {
                        expr: Expr::Assert(Box::new(e)),
                        span,
                    };
                }
                Some(Tok::Dot) => {
                    self.bump();
                    let (fname, fspan) = self.name_ident()?;
                    let span = Span {
                        start: e.span.start,
                        end: fspan.end,
                    };
                    e = SExpr {
                        expr: Expr::Field(Box::new(e), fname, fspan),
                        span,
                    };
                }
                Some(Tok::LBracket) => {
                    self.bump();
                    self.skip_newlines();
                    let idx = self.parse_expr()?;
                    self.skip_newlines();
                    if self.eat(&Tok::RBracket).is_none() {
                        self.err_expected("`]` to close the index");
                        return None;
                    }
                    let span = Span {
                        start: e.span.start,
                        end: self.prev_end(),
                    };
                    e = SExpr {
                        expr: Expr::Index(Box::new(e), Box::new(idx)),
                        span,
                    };
                }
                Some(Tok::LParen) => {
                    self.bump();
                    self.skip_newlines();
                    let mut args = Vec::new();
                    if !self.at(&Tok::RParen) {
                        loop {
                            self.skip_newlines();
                            let arg = self.parse_expr()?;
                            args.push(arg);
                            self.skip_newlines();
                            if self.eat(&Tok::Comma).is_some() {
                                self.skip_newlines();
                                if self.at(&Tok::RParen) {
                                    break;
                                }
                            } else {
                                break;
                            }
                        }
                    }
                    if self.eat(&Tok::RParen).is_none() {
                        self.err_expected("`)` to close the call");
                        return None;
                    }
                    let span = Span {
                        start: e.span.start,
                        end: self.prev_end(),
                    };
                    e = SExpr {
                        expr: Expr::Call(Box::new(e), args),
                        span,
                    };
                }
                _ => break,
            }
        }
        Some(e)
    }

    fn parse_primary(&mut self) -> Option<SExpr> {
        let start_span = self.here_span();
        let start = start_span.start;
        match self.cur().cloned() {
            Some(Tok::Text(s)) => {
                self.bump();
                Some(SExpr {
                    expr: Expr::Text(s),
                    span: start_span,
                })
            }
            Some(Tok::Number(n)) => {
                self.bump();
                Some(SExpr {
                    expr: Expr::Number(n),
                    span: start_span,
                })
            }
            Some(Tok::KwTrue) => {
                self.bump();
                Some(SExpr {
                    expr: Expr::Bool(true),
                    span: start_span,
                })
            }
            Some(Tok::KwFalse) => {
                self.bump();
                Some(SExpr {
                    expr: Expr::Bool(false),
                    span: start_span,
                })
            }
            Some(Tok::KwNone) => {
                self.bump();
                Some(SExpr {
                    expr: Expr::NoneLit,
                    span: start_span,
                })
            }
            Some(Tok::Ident(first)) => {
                // Maximal dotted chain -> one NameRef (ast.rs module docs).
                self.bump();
                let mut segs = vec![first];
                while self.at(&Tok::Dot) && matches!(self.nth(1), Some(Tok::Ident(_))) {
                    self.bump();
                    if let Some(Tok::Ident(s)) = self.cur().cloned() {
                        segs.push(s);
                        self.bump();
                    }
                }
                Some(SExpr {
                    expr: Expr::NameRef(segs),
                    span: self.span_from(start),
                })
            }
            Some(Tok::LParen) => {
                if self.fnlit_ahead() {
                    return self.parse_fnlit();
                }
                self.bump();
                self.skip_newlines();
                let inner = self.parse_expr()?;
                self.skip_newlines();
                if self.eat(&Tok::RParen).is_none() {
                    self.err_expected("`)` to close the group");
                    return None;
                }
                Some(SExpr {
                    expr: inner.expr,
                    span: self.span_from(start),
                })
            }
            Some(Tok::LBracket) => {
                self.bump();
                self.skip_newlines();
                let mut items = Vec::new();
                if !self.at(&Tok::RBracket) {
                    loop {
                        self.skip_newlines();
                        if self.eat(&Tok::Ellipsis).is_some() {
                            let e = self.parse_expr()?;
                            items.push(ListItem::Spread(e));
                        } else {
                            let e = self.parse_expr()?;
                            items.push(ListItem::Item(e));
                        }
                        self.skip_newlines();
                        if self.eat(&Tok::Comma).is_some() {
                            self.skip_newlines();
                            if self.at(&Tok::RBracket) {
                                break;
                            }
                        } else {
                            break;
                        }
                    }
                }
                if self.eat(&Tok::RBracket).is_none() {
                    self.err_expected("`]` to close the list");
                    return None;
                }
                Some(SExpr {
                    expr: Expr::List(items),
                    span: self.span_from(start),
                })
            }
            Some(Tok::LBrace) => {
                self.bump();
                self.skip_newlines();
                let mut items = Vec::new();
                if !self.at(&Tok::RBrace) {
                    loop {
                        self.skip_newlines();
                        if self.eat(&Tok::Ellipsis).is_some() {
                            let e = self.parse_expr()?;
                            items.push(MapItem::Spread(e));
                        } else {
                            let (key, kspan) = match self.cur().cloned() {
                                Some(Tok::Ident(s)) => {
                                    let sp = self.here_span();
                                    self.bump();
                                    (s, sp)
                                }
                                Some(Tok::Text(s)) => {
                                    let sp = self.here_span();
                                    self.bump();
                                    (s, sp)
                                }
                                _ => {
                                    self.err_expected("a map key (a name or a text literal)");
                                    return None;
                                }
                            };
                            if self.eat(&Tok::Colon).is_none() {
                                self.err_expected("`:` after the map key");
                                return None;
                            }
                            self.skip_newlines();
                            let v = self.parse_expr()?;
                            items.push(MapItem::Entry(key, kspan, v));
                        }
                        self.skip_newlines();
                        if self.eat(&Tok::Comma).is_some() {
                            self.skip_newlines();
                            if self.at(&Tok::RBrace) {
                                break;
                            }
                        } else {
                            break;
                        }
                    }
                }
                if self.eat(&Tok::RBrace).is_none() {
                    self.err_expected("`}` to close the map");
                    return None;
                }
                Some(SExpr {
                    expr: Expr::MapLit(items),
                    span: self.span_from(start),
                })
            }
            Some(Tok::KwIf) => self.parse_if_expr(),
            _ => {
                self.err_expected("an expression");
                None
            }
        }
    }

    /// From a `(` at the cursor, decide whether a `=>` follows the matching
    /// `)` — i.e. whether this is a function literal rather than a group.
    fn fnlit_ahead(&self) -> bool {
        let mut depth: i32 = 0;
        let mut i = self.pos;
        while let Some(t) = self.toks.get(i) {
            match t.tok {
                Tok::LParen => depth += 1,
                Tok::RParen => {
                    depth -= 1;
                    if depth == 0 {
                        return matches!(self.toks.get(i + 1).map(|t| &t.tok), Some(Tok::Arrow));
                    }
                }
                _ => {}
            }
            i += 1;
        }
        false
    }

    /// `( params ) => expr-or-block`, after `fnlit_ahead` said yes.
    fn parse_fnlit(&mut self) -> Option<SExpr> {
        let start = self.here_span().start;
        self.bump(); // `(`
        self.skip_newlines();
        let mut params = Vec::new();
        if !self.at(&Tok::RParen) {
            loop {
                self.skip_newlines();
                let (pname, pspan) = self.name_ident()?;
                if self.eat(&Tok::Colon).is_none() {
                    self.err_expected("`:` and a shape after the parameter name");
                    return None;
                }
                let pshape = self.parse_shape()?;
                params.push(Param {
                    name: pname,
                    name_span: pspan,
                    shape: pshape,
                });
                self.skip_newlines();
                if self.eat(&Tok::Comma).is_some() {
                    self.skip_newlines();
                    if self.at(&Tok::RParen) {
                        break;
                    }
                } else {
                    break;
                }
            }
        }
        if self.eat(&Tok::RParen).is_none() {
            self.err_expected("`)` to close the parameter list");
            return None;
        }
        if self.eat(&Tok::Arrow).is_none() {
            self.err_expected("`=>` after the parameter list");
            return None;
        }
        // `{` after `=>` is always a block body; a map literal result needs
        // parentheses or a `return` inside a block. Guessing it the other
        // way would silently mis-read blocks, and blocks are the common case.
        let body = if self.at(&Tok::LBrace) {
            FnBody::Block(self.parse_block()?)
        } else {
            FnBody::Expr(self.parse_expr()?)
        };
        Some(SExpr {
            expr: Expr::FnLit(params, Box::new(body)),
            span: self.span_from(start),
        })
    }

    /// `if` in expression position: both branches required.
    fn parse_if_expr(&mut self) -> Option<SExpr> {
        let start = self.here_span().start;
        self.bump(); // `if`
        let cond = self.parse_expr()?;
        let then = self.parse_block()?;
        if self.eat(&Tok::KwElse).is_none() {
            let span = self.span_from(start);
            self.err(
                span,
                "an `if` used as an expression needs an `else` branch.".to_string(),
            );
            return None;
        }
        let els: Vec<Stmt> = if self.at(&Tok::KwIf) {
            let nested = self.parse_if_expr()?;
            vec![Stmt::Expr(nested)]
        } else {
            self.parse_block()?
        };
        Some(SExpr {
            expr: Expr::IfExpr(Box::new(cond), then, els),
            span: self.span_from(start),
        })
    }

    // -- statements -----------------------------------------------------------

    fn parse_block(&mut self) -> Option<Vec<Stmt>> {
        if self.eat(&Tok::LBrace).is_none() {
            self.err_expected("`{`");
            return None;
        }
        let mut stmts = Vec::new();
        loop {
            self.skip_newlines();
            if self.eat(&Tok::RBrace).is_some() {
                break;
            }
            if self.cur().is_none() {
                self.err_expected("`}` to close the block");
                return None;
            }
            match self.parse_stmt() {
                Some(s) => stmts.push(s),
                None => self.skip_to_newline_at_depth(),
            }
        }
        Some(stmts)
    }

    fn parse_stmt(&mut self) -> Option<Stmt> {
        match self.cur().cloned() {
            Some(Tok::KwLet) => {
                self.bump();
                let (name, span) = self.name_ident()?;
                if self.eat(&Tok::Eq).is_none() {
                    self.err_expected("`=` after the `let` name");
                    return None;
                }
                let e = self.parse_expr()?;
                self.line_end("the `let` binding");
                Some(Stmt::Let(name, span, e))
            }
            Some(Tok::KwReturn) => {
                let rspan = self.here_span();
                self.bump();
                let value = if matches!(self.cur(), Some(Tok::Newline) | Some(Tok::RBrace) | None)
                {
                    None
                } else {
                    Some(self.parse_expr()?)
                };
                self.line_end("the `return`");
                Some(Stmt::Return(value, rspan))
            }
            Some(Tok::KwIf) => self.parse_if_stmt(),
            Some(Tok::KwFor) => {
                self.bump();
                let (v1, s1) = self.name_ident()?;
                let mut vars = vec![(v1, s1)];
                if self.eat(&Tok::Comma).is_some() {
                    let (v2, s2) = self.name_ident()?;
                    vars.push((v2, s2));
                }
                if self.eat(&Tok::KwIn).is_none() {
                    self.err_expected("`in` after the loop variables");
                    return None;
                }
                let iter = self.parse_expr()?;
                let body = self.parse_block()?;
                self.line_end("the `for` body");
                Some(Stmt::For(vars, iter, body))
            }
            Some(Tok::Ident(ref w)) if FOREIGN_STMT.contains(&w.as_str()) => {
                let span = self.here_span();
                let note = match w.as_str() {
                    "while" | "do" => "Iterate with `for x in xs`, or use recursion; there is no `while`.",
                    "switch" | "match" | "elif" => "Chain `if` / `else if` instead.",
                    "try" | "catch" | "throw" | "finally" => {
                        "Faults cannot be caught in-language; handle absence with `??` and `none`."
                    }
                    _ => "Declare locals with `let`.",
                };
                self.diags.push(
                    Diag::new(
                        E023_FOREIGN_STMT,
                        Level::Error,
                        &self.file,
                        span,
                        format!("`{}` is not an Ashlar statement.", w),
                    )
                    .with_fix(note.to_string(), vec![]),
                );
                None
            }
            Some(Tok::Ident(_)) if self.nth(1) == Some(&Tok::Eq) => {
                let (name, span) = self.name_ident()?;
                self.bump(); // `=`
                let e = self.parse_expr()?;
                self.line_end("the assignment");
                Some(Stmt::Assign(name, span, e))
            }
            Some(_) => {
                let e = self.parse_expr()?;
                if self.at(&Tok::Eq) {
                    self.err(
                        e.span,
                        "assignment targets must be a bare state property name.".to_string(),
                    );
                    self.bump();
                    let _ = self.parse_expr();
                    return None;
                }
                self.line_end("the expression");
                Some(Stmt::Expr(e))
            }
            None => {
                self.err_expected("a statement");
                None
            }
        }
    }

    /// `if` in statement position: `else` optional, `else if` nests.
    fn parse_if_stmt(&mut self) -> Option<Stmt> {
        self.bump(); // `if`
        let cond = self.parse_expr()?;
        let then = self.parse_block()?;
        let els = if self.eat(&Tok::KwElse).is_some() {
            if self.at(&Tok::KwIf) {
                let nested = self.parse_if_stmt()?;
                Some(vec![nested])
            } else {
                Some(self.parse_block()?)
            }
        } else {
            None
        };
        self.line_end("the `if`");
        Some(Stmt::If(cond, then, els))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::BinOp;
    use crate::lexer;

    /// Lex + parse. Callers that expect clean input assert no diagnostics.
    fn parse_src(src: &str) -> (Option<SrcFile>, Vec<Diag>) {
        let (toks, mut lex_diags) = lexer::lex("t.ash", src);
        let (ast, mut parse_diags) = parse("t.ash", &toks);
        lex_diags.append(&mut parse_diags);
        (ast, lex_diags)
    }

    fn parse_ok(src: &str) -> SrcFile {
        let (ast, diags) = parse_src(src);
        assert!(diags.is_empty(), "unexpected diagnostics: {:?}", diags);
        ast.expect("expected a parsed file")
    }

    fn ids(diags: &[Diag]) -> Vec<&'static str> {
        diags.iter().map(|d| d.id).collect()
    }

    // -- clean parses -------------------------------------------------------

    #[test]
    fn data_shape_part() {
        let f = parse_ok(
            "space chat.data\n\npart Message {\n  id: text\n  body: text\n  read: bool = false\n}\n",
        );
        assert_eq!(f.space, vec!["chat", "data"]);
        assert_eq!(f.parts.len(), 1);
        let p = &f.parts[0];
        assert_eq!(p.name, vec!["Message"]);
        assert_eq!(p.props.len(), 3);
        assert!(matches!(p.props[0].shape.as_ref().unwrap().shape, Shape::Text));
        assert!(p.props[0].value.is_none());
        assert!(matches!(p.props[2].shape.as_ref().unwrap().shape, Shape::Bool));
        assert!(matches!(
            p.props[2].value.as_ref().unwrap().expr,
            Expr::Bool(false)
        ));
    }

    #[test]
    fn append_prop_with_shape_and_value() {
        let f = parse_ok("space config\n\npart Config {\n  tags append: [text] = [\"core\"]\n}\n");
        let p = &f.parts[0].props[0];
        assert_eq!(p.name, "tags");
        let k = p.kind.as_ref().unwrap();
        assert_eq!(k.kind, MergeKind::Append);
        assert!(!k.reverse);
        assert!(matches!(&p.shape.as_ref().unwrap().shape, Shape::List(inner) if matches!(inner.shape, Shape::Text)));
        match &p.value.as_ref().unwrap().expr {
            Expr::List(items) => {
                assert_eq!(items.len(), 1);
                assert!(matches!(&items[0], ListItem::Item(e) if matches!(&e.expr, Expr::Text(t) if t == "core")));
            }
            other => panic!("expected list literal, got {:?}", other),
        }
    }

    #[test]
    fn dotted_layer_declaration() {
        let f = parse_ok("space chat.audit\nuse chat.data\n\npart chat.data.Message {\n  audit: text = \"none\"\n}\n");
        assert_eq!(f.uses.len(), 1);
        assert_eq!(f.uses[0].0, vec!["chat", "data"]);
        assert_eq!(f.parts[0].name, vec!["chat", "data", "Message"]);
    }

    #[test]
    fn stack_reverse_and_storage() {
        let f = parse_ok(
            "space srv\n\npart Server {\n  port = 8080\n  state ready: bool = false\n  stop stack reverse = () => {\n    return none\n  }\n}\n",
        );
        let props = &f.parts[0].props;
        assert!(props[0].kind.is_none());
        assert!(matches!(props[1].storage, Some((Storage::State, _))));
        let k = props[2].kind.as_ref().unwrap();
        assert_eq!(k.kind, MergeKind::Stack);
        assert!(k.reverse);
        match &props[2].value.as_ref().unwrap().expr {
            Expr::FnLit(params, body) => {
                assert!(params.is_empty());
                match body.as_ref() {
                    FnBody::Block(stmts) => {
                        assert!(matches!(&stmts[0], Stmt::Return(Some(e), _) if matches!(e.expr, Expr::NoneLit)));
                    }
                    _ => panic!("expected block body"),
                }
            }
            other => panic!("expected fn literal, got {:?}", other),
        }
    }

    #[test]
    fn pipe_handler_with_std_param() {
        let f = parse_ok(
            "space chat.api\n\npart messages {\n  route = \"/api/messages\"\n  handle pipe = (req: std.Request) => req\n}\n",
        );
        let h = &f.parts[0].props[1];
        assert_eq!(h.kind.as_ref().unwrap().kind, MergeKind::Pipe);
        match &h.value.as_ref().unwrap().expr {
            Expr::FnLit(params, body) => {
                assert_eq!(params.len(), 1);
                assert_eq!(params[0].name, "req");
                assert!(matches!(&params[0].shape.shape, Shape::Part(n) if n == &vec!["std".to_string(), "Request".to_string()]));
                assert!(matches!(body.as_ref(), FnBody::Expr(e) if matches!(&e.expr, Expr::NameRef(n) if n == &vec!["req".to_string()])));
            }
            other => panic!("expected fn literal, got {:?}", other),
        }
    }

    #[test]
    fn foreign_declaration() {
        let f = parse_ok("space net\n\nforeign fetch: (url: text) -> data\n");
        assert_eq!(f.foreigns.len(), 1);
        let d = &f.foreigns[0];
        assert_eq!(d.name, "fetch");
        assert_eq!(d.params.len(), 1);
        assert_eq!(d.params[0].0.as_deref(), Some("url"));
        assert!(matches!(d.params[0].1.shape, Shape::Text));
        assert!(matches!(d.ret.shape, Shape::Data));
    }

    #[test]
    fn nameref_chain_and_call() {
        let f = parse_ok(
            "space chat.api\nuse chat.audit\n\npart messages {\n  go = () => {\n    chat.data.Store.add({ id: id(), body: \"hello\" })\n  }\n}\n",
        );
        let v = f.parts[0].props[0].value.as_ref().unwrap();
        let Expr::FnLit(_, body) = &v.expr else { panic!() };
        let FnBody::Block(stmts) = body.as_ref() else { panic!() };
        let Stmt::Expr(call) = &stmts[0] else { panic!("got {:?}", stmts[0]) };
        match &call.expr {
            Expr::Call(callee, args) => {
                assert!(matches!(&callee.expr, Expr::NameRef(n) if n.len() == 4 && n[3] == "add"));
                assert_eq!(args.len(), 1);
                match &args[0].expr {
                    Expr::MapLit(items) => {
                        assert_eq!(items.len(), 2);
                        assert!(matches!(&items[0], MapItem::Entry(k, _, e) if k == "id" && matches!(&e.expr, Expr::Call(_, _))));
                    }
                    other => panic!("expected map literal, got {:?}", other),
                }
            }
            other => panic!("expected call, got {:?}", other),
        }
    }

    #[test]
    fn spreads_in_list_and_map() {
        let f = parse_ok(
            "space u\n\npart b {\n  extend = (base: [text], extra: text) => [...base, extra]\n  merge = (base: {text}, patch: {text}) => { return { ...base, ...patch } }\n}\n",
        );
        let v0 = f.parts[0].props[0].value.as_ref().unwrap();
        let Expr::FnLit(_, body) = &v0.expr else { panic!() };
        let FnBody::Expr(list) = body.as_ref() else { panic!() };
        let Expr::List(items) = &list.expr else { panic!() };
        assert!(matches!(&items[0], ListItem::Spread(_)));
        assert!(matches!(&items[1], ListItem::Item(_)));

        let v1 = f.parts[0].props[1].value.as_ref().unwrap();
        let Expr::FnLit(_, body) = &v1.expr else { panic!() };
        let FnBody::Block(stmts) = body.as_ref() else { panic!() };
        let Stmt::Return(Some(m), _) = &stmts[0] else { panic!() };
        let Expr::MapLit(items) = &m.expr else { panic!() };
        assert_eq!(items.len(), 2);
        assert!(matches!(&items[0], MapItem::Spread(_)));
        assert!(matches!(&items[1], MapItem::Spread(_)));
    }

    #[test]
    fn if_expression_bound_by_let() {
        let f = parse_ok(
            "space u\n\npart l {\n  d = (read: bool) => {\n    let status = if read { \"seen\" } else { \"new\" }\n    return status\n  }\n}\n",
        );
        let v = f.parts[0].props[0].value.as_ref().unwrap();
        let Expr::FnLit(_, body) = &v.expr else { panic!() };
        let FnBody::Block(stmts) = body.as_ref() else { panic!() };
        let Stmt::Let(name, _, e) = &stmts[0] else { panic!("got {:?}", stmts[0]) };
        assert_eq!(name, "status");
        let Expr::IfExpr(cond, then, els) = &e.expr else { panic!("got {:?}", e.expr) };
        assert!(matches!(&cond.expr, Expr::NameRef(n) if n == &vec!["read".to_string()]));
        assert!(matches!(&then[0], Stmt::Expr(t) if matches!(&t.expr, Expr::Text(s) if s == "seen")));
        assert!(matches!(&els[0], Stmt::Expr(t) if matches!(&t.expr, Expr::Text(s) if s == "new")));
    }

    #[test]
    fn fnlit_in_call_argument() {
        let f = parse_ok(
            "space n\n\npart a {\n  s = () => {\n    subscribe(\"alerts\", (msg: data) => log.info(\"alert\"))\n  }\n}\n",
        );
        let v = f.parts[0].props[0].value.as_ref().unwrap();
        let Expr::FnLit(_, body) = &v.expr else { panic!() };
        let FnBody::Block(stmts) = body.as_ref() else { panic!() };
        let Stmt::Expr(call) = &stmts[0] else { panic!() };
        let Expr::Call(_, args) = &call.expr else { panic!() };
        assert_eq!(args.len(), 2);
        assert!(matches!(&args[1].expr, Expr::FnLit(params, _) if params.len() == 1 && matches!(params[0].shape.shape, Shape::Data)));
    }

    #[test]
    fn precedence_and_operators() {
        let f = parse_ok("space m\n\npart c {\n  v = (a: number, b: number, c: number) => a + b * c\n  w = (x: text?) => x ?? \"d\"\n  y = (p: bool, q: bool) => not p and q\n}\n");
        let v = f.parts[0].props[0].value.as_ref().unwrap();
        let Expr::FnLit(_, body) = &v.expr else { panic!() };
        let FnBody::Expr(e) = body.as_ref() else { panic!() };
        let Expr::Binary(BinOp::Add, _, r) = &e.expr else { panic!("got {:?}", e.expr) };
        assert!(matches!(&r.expr, Expr::Binary(BinOp::Mul, _, _)));

        let w = f.parts[0].props[1].value.as_ref().unwrap();
        let Expr::FnLit(_, body) = &w.expr else { panic!() };
        let FnBody::Expr(e) = body.as_ref() else { panic!() };
        assert!(matches!(&e.expr, Expr::Binary(BinOp::Coalesce, _, _)));

        let y = f.parts[0].props[2].value.as_ref().unwrap();
        let Expr::FnLit(_, body) = &y.expr else { panic!() };
        let FnBody::Expr(e) = body.as_ref() else { panic!() };
        let Expr::Binary(BinOp::And, l, _) = &e.expr else { panic!("got {:?}", e.expr) };
        assert!(matches!(&l.expr, Expr::Unary(UnOp::Not, _)));
    }

    #[test]
    fn postfix_assert_index_field() {
        let f = parse_ok("space u\n\npart l {\n  first = (xs: [text]) => xs[0]!\n  path = (r: std.Request) => f(r).x\n  f = (r: std.Request) => r\n}\n");
        let v = f.parts[0].props[0].value.as_ref().unwrap();
        let Expr::FnLit(_, body) = &v.expr else { panic!() };
        let FnBody::Expr(e) = body.as_ref() else { panic!() };
        let Expr::Assert(inner) = &e.expr else { panic!("got {:?}", e.expr) };
        assert!(matches!(&inner.expr, Expr::Index(_, _)));

        let p = f.parts[0].props[1].value.as_ref().unwrap();
        let Expr::FnLit(_, body) = &p.expr else { panic!() };
        let FnBody::Expr(e) = body.as_ref() else { panic!() };
        assert!(matches!(&e.expr, Expr::Field(b, name, _) if name == "x" && matches!(&b.expr, Expr::Call(_, _))));
    }

    #[test]
    fn else_if_chain_nests() {
        let f = parse_ok(
            "space u\n\npart l {\n  go = (n: number) => {\n    if n > 2 {\n      log.info(\"big\")\n    } else if n > 1 {\n      log.info(\"mid\")\n    } else {\n      log.info(\"small\")\n    }\n  }\n}\n",
        );
        let v = f.parts[0].props[0].value.as_ref().unwrap();
        let Expr::FnLit(_, body) = &v.expr else { panic!() };
        let FnBody::Block(stmts) = body.as_ref() else { panic!() };
        let Stmt::If(_, _, Some(els)) = &stmts[0] else { panic!("got {:?}", stmts[0]) };
        assert_eq!(els.len(), 1);
        assert!(matches!(&els[0], Stmt::If(_, _, Some(_))));
    }

    #[test]
    fn for_two_vars_and_assign() {
        let f = parse_ok(
            "space u\n\npart r {\n  state lines: [text] = []\n  build = (counts: {number}) => {\n    lines = []\n    for k, v in counts {\n      lines = lines + [k + \": \" + text(v)]\n    }\n  }\n}\n",
        );
        let v = f.parts[0].props[1].value.as_ref().unwrap();
        let Expr::FnLit(_, body) = &v.expr else { panic!() };
        let FnBody::Block(stmts) = body.as_ref() else { panic!() };
        assert!(matches!(&stmts[0], Stmt::Assign(n, _, _) if n == "lines"));
        let Stmt::For(vars, _, inner) = &stmts[1] else { panic!("got {:?}", stmts[1]) };
        assert_eq!(vars.len(), 2);
        assert_eq!(vars[0].0, "k");
        assert_eq!(vars[1].0, "v");
        assert!(matches!(&inner[0], Stmt::Assign(_, _, _)));
    }

    // -- errors --------------------------------------------------------------

    #[test]
    fn e022_missing_space_header() {
        let (ast, diags) = parse_src("part Widget {\n}\n");
        assert!(ast.is_none());
        assert_eq!(ids(&diags), vec!["E022"]);
        assert!(diags[0].fix.is_some());
    }

    #[test]
    fn e022_use_after_part() {
        let (ast, diags) = parse_src("space a\n\npart W {\n  x: text\n}\n\nuse b\n");
        let f = ast.unwrap();
        assert_eq!(ids(&diags), vec!["E022"]);
        // The use is still recorded so resolution can continue.
        assert_eq!(f.uses.len(), 1);
    }

    #[test]
    fn e016_reserved_part_name() {
        let (_, diags) = parse_src("space a\n\npart if {\n  x: text\n}\n");
        assert!(ids(&diags).contains(&"E016"));
        assert!(diags[0].cause.contains("`if`"));
    }

    #[test]
    fn e018_class_toplevel() {
        let (ast, diags) = parse_src("space a\n\nclass Foo {\n  x: text\n}\n");
        assert_eq!(ids(&diags), vec!["E018"]);
        assert!(diags[0].cause.contains("`class`"));
        // The file survives; the class block is skipped.
        assert_eq!(ast.unwrap().parts.len(), 0);
    }

    #[test]
    fn e018_import_note_mentions_use() {
        let (_, diags) = parse_src("space a\n\nimport chat\n");
        assert_eq!(ids(&diags), vec!["E018"]);
        assert!(diags[0].fix.as_ref().unwrap().note.contains("`use <space>`"));
    }

    #[test]
    fn e020_reverse_on_append_has_delete_edit() {
        let (ast, diags) = parse_src("space a\n\npart C {\n  tags append reverse: [text] = []\n}\n");
        assert_eq!(ids(&diags), vec!["E020"]);
        let fix = diags[0].fix.as_ref().unwrap();
        assert_eq!(fix.edits.len(), 1);
        assert_eq!(fix.edits[0].text, "");
        // The prop still parses with append kind.
        let p = &ast.unwrap().parts[0].props[0];
        assert_eq!(p.kind.as_ref().unwrap().kind, MergeKind::Append);
        assert!(!p.kind.as_ref().unwrap().reverse);
    }

    #[test]
    fn e023_while_statement() {
        let (_, diags) = parse_src(
            "space a\n\npart W {\n  go = () => {\n    while true {\n      log.info(\"x\")\n    }\n  }\n}\n",
        );
        assert!(ids(&diags).contains(&"E023"));
        let d = diags.iter().find(|d| d.id == "E023").unwrap();
        assert!(d.cause.contains("`while`"));
    }

    #[test]
    fn e007_property_needs_shape_or_value() {
        let (_, diags) = parse_src("space a\n\npart W {\n  x\n}\n");
        assert_eq!(ids(&diags), vec!["E007"]);
        assert!(diags[0].cause.contains("shape, a value, or both"));
    }

    #[test]
    fn e007_dotted_assign_target() {
        let (_, diags) = parse_src(
            "space a\n\npart W {\n  state n: number = 0\n  go = () => {\n    a.b = 1\n  }\n}\n",
        );
        assert!(ids(&diags).contains(&"E007"));
        let d = diags.iter().find(|d| d.id == "E007").unwrap();
        assert!(d.cause.contains("bare state property name"));
    }

    #[test]
    fn recovery_keeps_following_props() {
        let (ast, diags) = parse_src("space a\n\npart W {\n  x: text\n  ] junk junk\n  y: text\n}\n");
        assert!(!diags.is_empty());
        let f = ast.unwrap();
        let names: Vec<&str> = f.parts[0].props.iter().map(|p| p.name.as_str()).collect();
        assert!(names.contains(&"x"));
        assert!(names.contains(&"y"));
    }

    #[test]
    fn never_panics_on_truncated_input() {
        // Truncations of a representative file must never panic.
        let src = "space a\nuse b\n\npart W {\n  tags append: [text] = [\"c\"]\n  go = (x: number) => {\n    if x > 1 {\n      return { a: [1, 2], b: x }\n    } else {\n      return none\n    }\n  }\n}\n\nforeign f: (text) -> data\n";
        for cut in 0..src.len() {
            if src.is_char_boundary(cut) {
                let _ = parse_src(&src[..cut]);
            }
        }
    }
}

/// Reserved-word spelling for E016 messages and `describe`.
fn kw_text(t: &Tok) -> Option<&'static str> {
    Some(match t {
        Tok::KwSpace => "space",
        Tok::KwUse => "use",
        Tok::KwPart => "part",
        Tok::KwForeign => "foreign",
        Tok::KwState => "state",
        Tok::KwStored => "stored",
        Tok::KwSynced => "synced",
        Tok::KwAppend => "append",
        Tok::KwDeep => "deep",
        Tok::KwStack => "stack",
        Tok::KwPipe => "pipe",
        Tok::KwReverse => "reverse",
        Tok::KwLet => "let",
        Tok::KwIf => "if",
        Tok::KwElse => "else",
        Tok::KwFor => "for",
        Tok::KwIn => "in",
        Tok::KwReturn => "return",
        Tok::KwTrue => "true",
        Tok::KwFalse => "false",
        Tok::KwNone => "none",
        Tok::KwAnd => "and",
        Tok::KwOr => "or",
        Tok::KwNot => "not",
        _ => return None,
    })
}

/// Human phrase for a token, used in "expected X, found Y" causes.
fn describe(t: Option<&Tok>) -> String {
    let Some(t) = t else {
        return "the end of the file".to_string();
    };
    if let Some(kw) = kw_text(t) {
        return format!("`{}`", kw);
    }
    match t {
        Tok::Ident(s) => format!("`{}`", s),
        Tok::Number(_) => "a number".to_string(),
        Tok::Text(_) => "a text literal".to_string(),
        Tok::Newline => "a line break".to_string(),
        Tok::LBrace => "`{`".to_string(),
        Tok::RBrace => "`}`".to_string(),
        Tok::LParen => "`(`".to_string(),
        Tok::RParen => "`)`".to_string(),
        Tok::LBracket => "`[`".to_string(),
        Tok::RBracket => "`]`".to_string(),
        Tok::Comma => "`,`".to_string(),
        Tok::Colon => "`:`".to_string(),
        Tok::Dot => "`.`".to_string(),
        Tok::Ellipsis => "`...`".to_string(),
        Tok::Question => "`?`".to_string(),
        Tok::Bang => "`!`".to_string(),
        Tok::Eq => "`=`".to_string(),
        Tok::EqEq => "`==`".to_string(),
        Tok::NotEq => "`!=`".to_string(),
        Tok::Lt => "`<`".to_string(),
        Tok::LtEq => "`<=`".to_string(),
        Tok::Gt => "`>`".to_string(),
        Tok::GtEq => "`>=`".to_string(),
        Tok::Plus => "`+`".to_string(),
        Tok::Minus => "`-`".to_string(),
        Tok::Star => "`*`".to_string(),
        Tok::Slash => "`/`".to_string(),
        Tok::Percent => "`%`".to_string(),
        Tok::Coalesce => "`??`".to_string(),
        Tok::Arrow => "`=>`".to_string(),
        Tok::ThinArrow => "`->`".to_string(),
        // Keywords were already handled via kw_text above.
        _ => "a reserved word".to_string(),
    }
}
