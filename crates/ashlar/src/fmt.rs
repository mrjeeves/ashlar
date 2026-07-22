//! `ashlar fmt` (reference §1): canonicalizes source to two-space indent,
//! `"` quotes, and one spacing convention, preserving comments and single
//! blank lines.
//!
//! Ground rules:
//!
//! * **Broken code is never rewritten.** A file with any lex/parse
//!   diagnostic is returned untouched with its diagnostics; formatting is
//!   only defined on programs the parser fully understood.
//! * **Formatting never changes meaning.** The test suite enforces two
//!   properties on the whole corpus: the formatted output parses to the
//!   same AST (spans aside), and formatting is idempotent.
//! * Comments are not tokens; they are re-extracted from the raw source
//!   (honoring text-literal rules) and re-attached by line: an own-line
//!   comment stays on its own line at the current indent, a trailing
//!   comment stays at the end of its construct's first line.
//! * A run of blank lines collapses to one; blank lines survive only
//!   between declarations, properties, and statements.
//!
//! Multiline-ness is derived from the source: a list, map, call, or block
//! that spanned multiple lines stays multiline (one item per line, with a
//! trailing comma); one that fit one line stays inline.

use crate::ast::{
    Expr, FnBody, ForeignDecl, ListItem, MapItem, MergeKind, Param, PartDecl, Prop, SExpr,
    SShape, Shape, SrcFile, Stmt, Storage, UnOp,
};
use crate::diag::Diag;
use crate::{lexer, parser};

/// Format one file. `Err` carries the diagnostics that made formatting
/// undefined (the file is not modified in that case).
pub fn format_source(file: &str, src: &str) -> Result<String, Vec<Diag>> {
    let (toks, lex_diags) = lexer::lex(file, src);
    let (ast, parse_diags) = parser::parse(file, &toks);
    let mut diags = lex_diags;
    diags.extend(parse_diags);
    if !diags.is_empty() || ast.is_none() {
        return Err(diags);
    }
    let ast = ast.unwrap();

    let comments = extract_comments(src);
    let blanks = blank_lines(src);
    let mut p = Printer {
        out: String::new(),
        indent: 0,
        comments,
        blanks,
        next_comment: 0,
        last_emitted_line: 0,
    };
    p.file(&ast);
    p.flush_comments(u32::MAX);
    // Exactly one trailing newline.
    let mut out = p.out.trim_end().to_string();
    out.push('\n');
    Ok(out)
}

/// A comment found in the raw source.
struct Comment {
    line: u32,
    /// Text including the `//`.
    text: String,
    /// True when nothing but whitespace preceded it on its line.
    own_line: bool,
}

/// Scan for `//` comments, honoring text literals (either quote, with
/// escapes) so `"http://x"` is never a comment.
fn extract_comments(src: &str) -> Vec<Comment> {
    let mut out = Vec::new();
    for (i, line) in src.lines().enumerate() {
        let mut quote: Option<char> = None;
        let mut escaped = false;
        let chars: Vec<char> = line.chars().collect();
        let mut j = 0;
        while j < chars.len() {
            let c = chars[j];
            match quote {
                Some(q) => {
                    if escaped {
                        escaped = false;
                    } else if c == '\\' {
                        escaped = true;
                    } else if c == q {
                        quote = None;
                    }
                }
                None => {
                    if c == '"' || c == '\'' {
                        quote = Some(c);
                    } else if c == '/' && chars.get(j + 1) == Some(&'/') {
                        let text: String = chars[j..].iter().collect();
                        let own_line = chars[..j].iter().all(|c| c.is_whitespace());
                        out.push(Comment {
                            line: (i + 1) as u32,
                            text: text.trim_end().to_string(),
                            own_line,
                        });
                        break;
                    }
                }
            }
            j += 1;
        }
    }
    out
}

/// 1-based numbers of lines that are entirely blank.
fn blank_lines(src: &str) -> Vec<u32> {
    src.lines()
        .enumerate()
        .filter(|(_, l)| l.trim().is_empty())
        .map(|(i, _)| (i + 1) as u32)
        .collect()
}

struct Printer {
    out: String,
    indent: usize,
    comments: Vec<Comment>,
    blanks: Vec<u32>,
    next_comment: usize,
    last_emitted_line: u32,
}

impl Printer {
    fn pad(&mut self) {
        for _ in 0..self.indent {
            self.out.push_str("  ");
        }
    }

    /// Emit every not-yet-emitted comment whose source line is before
    /// `upto`, as own-line comments at the current indent, keeping a blank
    /// line where the source had one. (A trailing comment normally leaves
    /// through `trailing()`; one that was never claimed — e.g. on an inner
    /// line of a multiline literal — resurfaces here as an own-line
    /// comment, preserving its content and approximate position.)
    fn flush_comments(&mut self, upto: u32) {
        while self.next_comment < self.comments.len() {
            let c = &self.comments[self.next_comment];
            if c.line >= upto {
                break;
            }
            let line = c.line;
            let text = c.text.clone();
            self.blank_gap(line);
            self.pad();
            self.out.push_str(&text);
            self.out.push('\n');
            self.last_emitted_line = self.last_emitted_line.max(line);
            self.next_comment += 1;
        }
    }

    /// The trailing comment for source line `line`, if one exists and has
    /// not been emitted yet.
    fn trailing(&mut self, line: u32) -> Option<String> {
        if self.next_comment < self.comments.len() {
            let c = &self.comments[self.next_comment];
            if c.line == line && !c.own_line {
                let text = c.text.clone();
                self.next_comment += 1;
                return Some(text);
            }
        }
        None
    }

    /// Emit one blank line if the source had any blank line strictly
    /// between the last emitted construct and `next_line`.
    fn blank_gap(&mut self, next_line: u32) {
        if self.last_emitted_line == 0 {
            return;
        }
        let had_blank = self
            .blanks
            .iter()
            .any(|&b| b > self.last_emitted_line && b < next_line);
        if had_blank && !self.out.ends_with("\n\n") && !self.out.is_empty() {
            self.out.push('\n');
        }
    }

    /// Start a construct that begins at source `line`: flush comments due
    /// before it, honor the blank gap, then indent.
    fn open_line(&mut self, line: u32) {
        self.flush_comments(line);
        self.blank_gap(line);
        self.pad();
    }

    /// End a construct line that started at source `line`.
    fn close_line(&mut self, line: u32) {
        if let Some(t) = self.trailing(line) {
            self.out.push_str("  ");
            self.out.push_str(&t);
        }
        self.out.push('\n');
        self.last_emitted_line = self.last_emitted_line.max(line);
    }

    // -- declarations -------------------------------------------------------

    fn file(&mut self, f: &SrcFile) {
        let line = f.space_span.start.line;
        self.open_line(line);
        self.out.push_str("space ");
        self.out.push_str(&f.space.join("."));
        self.close_line(line);

        for (name, span) in &f.uses {
            let line = span.start.line;
            self.open_line(line);
            self.out.push_str("use ");
            self.out.push_str(&name.join("."));
            self.close_line(line);
        }

        // Parts and foreigns interleave by source position.
        enum Decl<'a> {
            P(&'a PartDecl),
            F(&'a ForeignDecl),
        }
        let mut decls: Vec<(u32, Decl)> = f
            .parts
            .iter()
            .map(|p| (p.name_span.start.line, Decl::P(p)))
            .chain(f.foreigns.iter().map(|d| (d.name_span.start.line, Decl::F(d))))
            .collect();
        decls.sort_by_key(|(l, _)| *l);
        for (_, d) in decls {
            match d {
                Decl::P(p) => self.part(p),
                Decl::F(d) => self.foreign(d),
            }
        }
    }

    fn part(&mut self, p: &PartDecl) {
        let line = p.name_span.start.line;
        self.open_line(line);
        self.out.push_str("part ");
        self.out.push_str(&p.name.join("."));
        self.out.push_str(" {");
        self.close_line(line);
        self.indent += 1;
        for prop in &p.props {
            self.prop(prop);
        }
        self.indent -= 1;
        // The closing brace: comments inside the body but after the last
        // property flush at one indent deeper? No — they belong to the
        // body; flush them before dedenting visually at body indent.
        self.pad();
        self.out.push_str("}");
        self.out.push('\n');
        self.last_emitted_line += 1;
    }

    fn foreign(&mut self, d: &ForeignDecl) {
        let line = d.name_span.start.line;
        self.open_line(line);
        self.out.push_str("foreign ");
        self.out.push_str(&d.name);
        self.out.push_str(": (");
        for (i, (name, sh)) in d.params.iter().enumerate() {
            if i > 0 {
                self.out.push_str(", ");
            }
            if let Some(n) = name {
                self.out.push_str(n);
                self.out.push_str(": ");
            }
            self.out.push_str(&shape_text(sh));
        }
        self.out.push_str(") -> ");
        self.out.push_str(&shape_text(&d.ret));
        self.close_line(line);
    }

    fn prop(&mut self, p: &Prop) {
        let line = p.name_span.start.line;
        self.open_line(line);
        if let Some((s, _)) = &p.storage {
            self.out.push_str(match s {
                Storage::State => "state ",
                Storage::Stored => "stored ",
                Storage::Synced => "synced ",
            });
        }
        self.out.push_str(&p.name);
        if let Some(k) = &p.kind {
            self.out.push(' ');
            self.out.push_str(match k.kind {
                MergeKind::Append => "append",
                MergeKind::Deep => "deep",
                MergeKind::Stack => "stack",
                MergeKind::Pipe => "pipe",
            });
            if k.reverse {
                self.out.push_str(" reverse");
            }
        }
        if let Some(sh) = &p.shape {
            self.out.push_str(": ");
            self.out.push_str(&shape_text(sh));
        }
        if let Some(v) = &p.value {
            self.out.push_str(" = ");
            self.expr(v, 0);
        }
        self.close_line(line);
    }

    // -- statements ---------------------------------------------------------

    fn stmt(&mut self, s: &Stmt) {
        match s {
            Stmt::Let(name, span, e) => {
                let line = span.start.line;
                self.open_line(line);
                self.out.push_str("let ");
                self.out.push_str(name);
                self.out.push_str(" = ");
                self.expr(e, 0);
                self.close_line(line);
            }
            Stmt::Assign(name, span, e) => {
                let line = span.start.line;
                self.open_line(line);
                self.out.push_str(name);
                self.out.push_str(" = ");
                self.expr(e, 0);
                self.close_line(line);
            }
            Stmt::Return(value, span) => {
                let line = span.start.line;
                self.open_line(line);
                self.out.push_str("return");
                if let Some(e) = value {
                    self.out.push(' ');
                    self.expr(e, 0);
                }
                self.close_line(line);
            }
            Stmt::Expr(e) => {
                let line = e.span.start.line;
                self.open_line(line);
                self.expr(e, 0);
                self.close_line(line);
            }
            Stmt::If(cond, then, els) => {
                let line = cond.span.start.line;
                self.open_line(line);
                self.if_chain(cond, then, els.as_deref());
                self.out.push('\n');
            }
            Stmt::For(vars, iter, body) => {
                let line = vars
                    .first()
                    .map(|(_, sp)| sp.start.line)
                    .unwrap_or(iter.span.start.line);
                self.open_line(line);
                self.out.push_str("for ");
                for (i, (v, _)) in vars.iter().enumerate() {
                    if i > 0 {
                        self.out.push_str(", ");
                    }
                    self.out.push_str(v);
                }
                self.out.push_str(" in ");
                self.expr(iter, 1);
                self.out.push_str(" ");
                self.stmt_block(body, line);
                self.out.push('\n');
            }
        }
    }

    /// `if c { ... } else if c2 { ... } else { ... }`, no trailing newline.
    fn if_chain(&mut self, cond: &SExpr, then: &[Stmt], els: Option<&[Stmt]>) {
        self.out.push_str("if ");
        self.expr(cond, 1);
        self.out.push_str(" ");
        self.stmt_block(then, cond.span.start.line);
        if let Some(els) = els {
            self.out.push_str(" else ");
            // `else if` chains are a single nested If statement.
            if els.len() == 1 {
                if let Stmt::If(c2, t2, e2) = &els[0] {
                    self.if_chain(c2, t2, e2.as_deref());
                    return;
                }
            }
            self.stmt_block(els, cond.span.start.line);
        }
    }

    /// `{ stmts }` with the brace on the current line; no trailing newline.
    fn stmt_block(&mut self, stmts: &[Stmt], open_source_line: u32) {
        self.out.push('{');
        if let Some(t) = self.trailing(open_source_line) {
            self.out.push_str("  ");
            self.out.push_str(&t);
        }
        self.out.push('\n');
        self.last_emitted_line = self.last_emitted_line.max(open_source_line);
        self.indent += 1;
        for s in stmts {
            self.stmt(s);
        }
        self.indent -= 1;
        self.pad();
        self.out.push('}');
    }

    // -- expressions ---------------------------------------------------------
    //
    // Precedence for re-parenthesization, loosest to tightest (§6):
    //   0 if/fn-literal | 1 or | 2 and | 3 not | 4 == != | 5 < <= > >=
    //   | 6 ?? | 7 + - | 8 * / % | 9 unary - | 10 postfix | 11 atoms

    fn expr(&mut self, e: &SExpr, min_prec: u8) {
        let p = prec(&e.expr);
        if p < min_prec {
            self.out.push('(');
            self.expr_inner(e);
            self.out.push(')');
        } else {
            self.expr_inner(e);
        }
    }

    fn expr_inner(&mut self, e: &SExpr) {
        match &e.expr {
            Expr::Text(s) => self.out.push_str(&text_literal(s)),
            Expr::Number(n) => self.out.push_str(&number_literal(*n)),
            Expr::Bool(true) => self.out.push_str("true"),
            Expr::Bool(false) => self.out.push_str("false"),
            Expr::NoneLit => self.out.push_str("none"),
            Expr::NameRef(segs) => self.out.push_str(&segs.join(".")),
            Expr::Field(b, name, _) => {
                self.expr(b, 10);
                self.out.push('.');
                self.out.push_str(name);
            }
            Expr::Index(b, i) => {
                self.expr(b, 10);
                self.out.push('[');
                self.expr(i, 0);
                self.out.push(']');
            }
            Expr::Call(callee, args) => {
                self.expr(callee, 10);
                self.out.push('(');
                for (i, a) in args.iter().enumerate() {
                    if i > 0 {
                        self.out.push_str(", ");
                    }
                    self.expr(a, 0);
                }
                self.out.push(')');
            }
            Expr::Assert(x) => {
                self.expr(x, 10);
                self.out.push('!');
            }
            Expr::Unary(UnOp::Not, x) => {
                self.out.push_str("not ");
                self.expr(x, 3);
            }
            Expr::Unary(UnOp::Neg, x) => {
                self.out.push('-');
                self.expr(x, 9);
            }
            Expr::Binary(op, l, r) => {
                use crate::ast::BinOp::*;
                let p = prec(&e.expr);
                self.expr(l, p);
                self.out.push_str(match op {
                    Or => " or ",
                    And => " and ",
                    EqEq => " == ",
                    NotEq => " != ",
                    Lt => " < ",
                    LtEq => " <= ",
                    Gt => " > ",
                    GtEq => " >= ",
                    Coalesce => " ?? ",
                    Add => " + ",
                    Sub => " - ",
                    Mul => " * ",
                    Div => " / ",
                    Rem => " % ",
                });
                self.expr(r, p + 1);
            }
            Expr::List(items) => {
                let multiline = e.span.start.line != e.span.end.line;
                self.out.push('[');
                if multiline {
                    self.indent += 1;
                    for it in items {
                        self.out.push('\n');
                        self.pad();
                        self.list_item(it);
                        self.out.push(',');
                    }
                    self.indent -= 1;
                    self.out.push('\n');
                    self.pad();
                } else {
                    for (i, it) in items.iter().enumerate() {
                        if i > 0 {
                            self.out.push_str(", ");
                        }
                        self.list_item(it);
                    }
                }
                self.out.push(']');
            }
            Expr::MapLit(items) => {
                let multiline = e.span.start.line != e.span.end.line;
                if items.is_empty() {
                    self.out.push_str("{}");
                    return;
                }
                self.out.push('{');
                if multiline {
                    self.indent += 1;
                    for it in items {
                        self.out.push('\n');
                        self.pad();
                        self.map_item(it);
                        self.out.push(',');
                    }
                    self.indent -= 1;
                    self.out.push('\n');
                    self.pad();
                    self.out.push('}');
                } else {
                    self.out.push(' ');
                    for (i, it) in items.iter().enumerate() {
                        if i > 0 {
                            self.out.push_str(", ");
                        }
                        self.map_item(it);
                    }
                    self.out.push_str(" }");
                }
            }
            Expr::IfExpr(cond, then, els) => {
                self.out.push_str("if ");
                self.expr(cond, 1);
                self.out.push_str(" { ");
                self.branch_inline(then);
                self.out.push_str(" } else { ");
                self.branch_inline(els);
                self.out.push_str(" }");
            }
            Expr::FnLit(params, body) => {
                self.out.push('(');
                for (i, p) in params.iter().enumerate() {
                    if i > 0 {
                        self.out.push_str(", ");
                    }
                    self.param(p);
                }
                self.out.push_str(") => ");
                match body.as_ref() {
                    FnBody::Expr(x) => self.expr(x, 1),
                    FnBody::Block(stmts) => {
                        self.stmt_block(stmts, e.span.start.line);
                    }
                }
            }
        }
    }

    /// If-expression branches hold statements but are canonically inline;
    /// a branch that is a single expression prints bare.
    fn branch_inline(&mut self, stmts: &[Stmt]) {
        if stmts.len() == 1 {
            if let Stmt::Expr(x) = &stmts[0] {
                self.expr(x, 0);
                return;
            }
        }
        // Rare: statement-bearing branches fall back to block form.
        for (i, s) in stmts.iter().enumerate() {
            if i > 0 {
                self.out.push_str("; ");
            }
            if let Stmt::Expr(x) = s {
                self.expr(x, 0);
            }
        }
    }

    fn list_item(&mut self, it: &ListItem) {
        match it {
            ListItem::Item(x) => self.expr(x, 0),
            ListItem::Spread(x) => {
                self.out.push_str("...");
                self.expr(x, 10);
            }
        }
    }

    fn map_item(&mut self, it: &MapItem) {
        match it {
            MapItem::Entry(k, _, v) => {
                if is_bare_key(k) {
                    self.out.push_str(k);
                } else {
                    self.out.push_str(&text_literal(k));
                }
                self.out.push_str(": ");
                self.expr(v, 0);
            }
            MapItem::Spread(x) => {
                self.out.push_str("...");
                self.expr(x, 10);
            }
        }
    }

    fn param(&mut self, p: &Param) {
        self.out.push_str(&p.name);
        self.out.push_str(": ");
        self.out.push_str(&shape_text(&p.shape));
    }
}

fn prec(e: &Expr) -> u8 {
    use crate::ast::BinOp::*;
    match e {
        Expr::IfExpr(..) | Expr::FnLit(..) => 0,
        Expr::Binary(op, _, _) => match op {
            Or => 1,
            And => 2,
            EqEq | NotEq => 4,
            Lt | LtEq | Gt | GtEq => 5,
            Coalesce => 6,
            Add | Sub => 7,
            Mul | Div | Rem => 8,
        },
        Expr::Unary(UnOp::Not, _) => 3,
        Expr::Unary(UnOp::Neg, _) => 9,
        Expr::Field(..) | Expr::Index(..) | Expr::Call(..) | Expr::Assert(..) => 10,
        _ => 11,
    }
}

/// A map key that lexes as an identifier may print bare; anything else —
/// including reserved words, which would re-lex as keywords — stays quoted.
fn is_bare_key(k: &str) -> bool {
    const RESERVED: &[&str] = &[
        "space", "use", "part", "foreign", "state", "stored", "synced", "append", "deep",
        "stack", "pipe", "reverse", "let", "if", "else", "for", "in", "return", "true",
        "false", "none", "and", "or", "not",
    ];
    if RESERVED.contains(&k) {
        return false;
    }
    let mut chars = k.chars();
    match chars.next() {
        Some(c) if c.is_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_alphanumeric() || c == '_')
}

/// Render a shape annotation canonically.
fn shape_text(sh: &SShape) -> String {
    match &sh.shape {
        Shape::Text => "text".into(),
        Shape::Number => "number".into(),
        Shape::Bool => "bool".into(),
        Shape::Data => "data".into(),
        Shape::List(i) => format!("[{}]", shape_text(i)),
        Shape::Map(v) => format!("{{text: {}}}", shape_text(v)),
        Shape::Part(n) => n.join("."),
        Shape::Opt(i) => format!("{}?", shape_text(i)),
        Shape::Fn(params, ret) => {
            let ps: Vec<String> = params
                .iter()
                .map(|(n, s)| match n {
                    Some(n) => format!("{}: {}", n, shape_text(s)),
                    None => shape_text(s),
                })
                .collect();
            format!("({}) -> {}", ps.join(", "), shape_text(ret))
        }
    }
}

/// Canonical text literal: `"` quotes, minimal escapes.
fn text_literal(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Canonical number rendering: integers without a fraction.
fn number_literal(n: f64) -> String {
    if n.fract() == 0.0 && n.abs() < 9.0e15 {
        format!("{}", n as i64)
    } else {
        format!("{}", n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{lexer, parser};

    /// AST equality modulo spans: debug-print, then erase every span/pos
    /// rendering. Test-only; the Debug format is deterministic.
    fn ast_fingerprint(file: &str, src: &str) -> String {
        let (toks, lex_diags) = lexer::lex(file, src);
        assert!(lex_diags.is_empty(), "{}: lex diags {:?}", file, lex_diags);
        let (ast, parse_diags) = parser::parse(file, &toks);
        assert!(parse_diags.is_empty(), "{}: parse diags {:?}", file, parse_diags);
        let dbg = format!("{:?}", ast.expect("parses"));
        // Erase `Pos { line: N, col: N }` so only structure remains.
        let mut out = String::new();
        let mut rest = dbg.as_str();
        while let Some(i) = rest.find("Pos {") {
            out.push_str(&rest[..i]);
            out.push_str("Pos");
            match rest[i..].find('}') {
                Some(j) => rest = &rest[i + j + 1..],
                None => {
                    rest = "";
                }
            }
        }
        out.push_str(rest);
        out
    }

    fn assert_fmt_faithful(name: &str, src: &str) {
        let formatted = format_source(name, src)
            .unwrap_or_else(|d| panic!("{}: fmt refused: {:?}", name, d));
        // Property 1: same program.
        assert_eq!(
            ast_fingerprint(name, src),
            ast_fingerprint(name, &formatted),
            "{}: formatting changed the AST.\n--- formatted:\n{}",
            name,
            formatted
        );
        // Property 2: idempotent.
        let second = format_source(name, &formatted)
            .unwrap_or_else(|d| panic!("{}: refmt refused: {:?}\n{}", name, d, formatted));
        assert_eq!(formatted, second, "{}: fmt is not idempotent", name);
        // Property 3: comments preserved (count).
        let before = extract_comments(src).len();
        let after = extract_comments(&formatted).len();
        assert_eq!(before, after, "{}: comment count changed", name);
    }

    #[test]
    fn corpus_and_reference_survive_formatting() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()
            .unwrap();
        // Every t_a3 snippet; multi-file snippets split at their
        // `// file:` presentation markers into the virtual files they model.
        let mut checked = 0;
        for entry in std::fs::read_dir(root.join("suites/t_a3")).unwrap() {
            let p = entry.unwrap().path();
            if p.extension().map(|e| e == "ash").unwrap_or(false) {
                let src = std::fs::read_to_string(&p).unwrap();
                let mut pieces: Vec<String> = vec![String::new()];
                for line in src.lines() {
                    if line.trim_start().starts_with("// file:") {
                        pieces.push(String::new());
                    } else {
                        let cur = pieces.last_mut().unwrap();
                        cur.push_str(line);
                        cur.push('\n');
                    }
                }
                for (i, piece) in pieces.iter().enumerate() {
                    if !piece.trim().is_empty() {
                        let name = format!(
                            "{}#{}",
                            p.file_name().unwrap().to_string_lossy(),
                            i
                        );
                        assert_fmt_faithful(&name, piece);
                    }
                }
                checked += 1;
            }
        }
        assert!(checked >= 20, "expected the t_a3 corpus, found {}", checked);
        // Every ```ash block in the reference.
        let reference = std::fs::read_to_string(root.join("reference/ashlar.md")).unwrap();
        let mut rest = reference.as_str();
        let mut blocks = 0;
        while let Some(i) = rest.find("```ash\n") {
            rest = &rest[i + 7..];
            let end = rest.find("```").unwrap();
            assert_fmt_faithful(&format!("refblock{}", blocks), &rest[..end]);
            rest = &rest[end + 3..];
            blocks += 1;
        }
        assert!(blocks >= 10, "expected reference blocks, found {}", blocks);
    }

    #[test]
    fn canonicalizes_quotes_and_spacing() {
        let src = "space a\n\npart W {\n  greeting = 'hello'\n  go=(n:number)=>n*2\n}\n";
        let out = format_source("t.ash", src).unwrap();
        assert!(out.contains("greeting = \"hello\""), "{}", out);
        assert!(out.contains("go = (n: number) => n * 2"), "{}", out);
    }

    #[test]
    fn preserves_comments_and_blank_lines() {
        let src = "space a\n\n// the widget\npart W {\n  x: text  // trailing\n\n  y: text\n}\n";
        let out = format_source("t.ash", src).unwrap();
        assert!(out.contains("// the widget\npart W {"), "{}", out);
        assert!(out.contains("x: text  // trailing"), "{}", out);
        assert!(out.contains("// trailing\n\n  y: text"), "{}", out);
    }

    #[test]
    fn reparenthesizes_precedence_faithfully() {
        let src = "space a\n\npart W {\n  f = (x: number, y: number, z: number) => (x + y) * z\n  g = (p: bool, q: bool) => not (p and q)\n}\n";
        let out = format_source("t.ash", src).unwrap();
        assert!(out.contains("(x + y) * z"), "{}", out);
        assert!(out.contains("not (p and q)"), "{}", out);
    }

    #[test]
    fn broken_source_is_refused() {
        assert!(format_source("t.ash", "part W {\n}\n").is_err());
        assert!(format_source("t.ash", "space a\n\npart W {\n  x: text;\n}\n").is_err());
    }

    #[test]
    fn multiline_literals_stay_multiline_with_trailing_commas() {
        let src = "space a\n\npart W {\n  tags append: [text] = [\n    \"one\",\n    \"two\"\n  ]\n}\n";
        let out = format_source("t.ash", src).unwrap();
        assert!(out.contains("[\n    \"one\",\n    \"two\",\n  ]"), "{}", out);
    }
}
