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

    /// Comments due before an item of a multi-line list or map literal.
    ///
    /// Without this the comment stays queued until the next STATEMENT
    /// opens, and is then printed there — so a note written above one key
    /// of a map silently ends up documenting whatever declaration follows
    /// the literal. The count is preserved either way, which is why the
    /// comment-count property never saw it; a comment attached to the
    /// wrong thing is worse than one that is missing, because it is
    /// confidently wrong.
    fn item_comments(&mut self, line: u32) {
        while self.next_comment < self.comments.len() {
            let c = &self.comments[self.next_comment];
            if c.line >= line || !c.own_line {
                break;
            }
            let (at, text) = (c.line, c.text.clone());
            self.pad();
            self.out.push_str(&text);
            self.out.push('\n');
            self.last_emitted_line = self.last_emitted_line.max(at);
            self.next_comment += 1;
        }
    }

    /// A comment sitting after an item on the item's own line.
    fn item_trailing(&mut self, line: u32) {
        if let Some(t) = self.trailing(line) {
            self.out.push_str("  ");
            self.out.push_str(&t);
        }
        self.last_emitted_line = self.last_emitted_line.max(line);
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

    /// End a construct that started at source `line` and whose source text
    /// ran through `end_line`.
    ///
    /// A comment written on an inner line of a construct that PRINTS on one
    /// line has no line of its own to go back to. It used to stay queued
    /// until the next declaration opened and print there, so a note about
    /// one term of an expression came out documenting the property below
    /// it — the count was preserved, the sentence was not, and a comment
    /// attached to the wrong thing is worse than a missing one because it
    /// is confidently wrong.
    ///
    /// Whatever is still queued from inside this construct belongs to this
    /// construct. The first leaves as its trailing comment; any others
    /// follow on their own lines at the same indent. That is as close to
    /// where they were written as a one-line form allows — the line they
    /// were on no longer exists.
    fn close_line_spanning(&mut self, line: u32, end_line: u32) {
        let mut claimed: Vec<String> = Vec::new();
        if let Some(t) = self.trailing(line) {
            claimed.push(t);
        }
        while self.next_comment < self.comments.len() {
            let c = &self.comments[self.next_comment];
            if c.line > end_line {
                break;
            }
            let (at, text) = (c.line, c.text.clone());
            claimed.push(text);
            self.last_emitted_line = self.last_emitted_line.max(at);
            self.next_comment += 1;
        }
        let mut claimed = claimed.into_iter();
        if let Some(first) = claimed.next() {
            self.out.push_str("  ");
            self.out.push_str(&first);
        }
        self.out.push('\n');
        for rest in claimed {
            self.pad();
            self.out.push_str(&rest);
            self.out.push('\n');
        }
        self.last_emitted_line = self.last_emitted_line.max(end_line);
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
        if let Some(r) = &d.react {
            self.out.push_str(if r.updates { " updates " } else { " watches " });
            self.out.push_str(&crate::ast::name_to_string(&r.collection));
        }
        self.close_line(line);
    }

    fn prop(&mut self, p: &Prop) {
        let line = p.name_span.start.line;
        self.open_line(line);
        // `setting` leads the declaration (§4). Dropping it here would silently
        // turn a deployment-bound value into a bare field — the formatter must
        // preserve meaning, not just shape.
        if p.setting {
            self.out.push_str("setting ");
        }
        if p.peruser {
            self.out.push_str("peruser ");
        }
        if let Some((s, _)) = &p.storage {
            self.out.push_str(match s {
                Storage::State => "state ",
                Storage::Stored => "stored ",
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
        let mut end_line = p.name_span.end.line;
        if let Some(v) = &p.value {
            self.out.push_str(" = ");
            self.expr(v, 0);
            end_line = end_line.max(v.span.end.line);
        }
        self.close_line_spanning(line, end_line);
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
                        self.item_comments(list_item_line(it));
                        self.pad();
                        self.list_item(it);
                        self.out.push(',');
                        self.item_trailing(list_item_line(it));
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
                        self.item_comments(map_item_line(it));
                        self.pad();
                        self.map_item(it);
                        self.out.push(',');
                        self.item_trailing(map_item_line(it));
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
                let inline = chain_inlineable(then, els);
                self.if_expr(cond, then, els, inline);
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

    /// `if c { a } else if c2 { b } else { c }` in EXPRESSION position.
    ///
    /// Two shapes have to survive the round trip, and both were once
    /// printed as `else { ... }`, which is a different program:
    ///
    /// * An `else` branch that is itself an if-expression chains as
    ///   `else if`. Wrapping it in braces makes the branch a statement
    ///   block, and a block whose statement is an `if` STATEMENT has the
    ///   value `none` (§6) — so `else { if ... }` quietly returns none for
    ///   every input the first branch missed.
    /// * A branch carrying statements (a `let`, a loop) cannot be a
    ///   one-liner at all, so the whole chain prints in block form.
    fn if_expr(&mut self, cond: &SExpr, then: &[Stmt], els: &[Stmt], inline: bool) {
        self.out.push_str("if ");
        self.expr(cond, 1);
        if inline {
            self.out.push_str(" { ");
            self.branch_inline(then);
            self.out.push_str(" }");
        } else {
            self.out.push(' ');
            self.stmt_block(then, cond.span.start.line);
        }
        self.out.push_str(" else ");
        if let Some((c2, t2, e2)) = as_chain(els) {
            self.if_expr(c2, t2, e2, inline);
            return;
        }
        if inline {
            self.out.push_str("{ ");
            self.branch_inline(els);
            self.out.push_str(" }");
        } else {
            self.stmt_block(els, cond.span.start.line);
        }
    }

    /// One inline if-expression branch: a lone expression prints bare.
    /// `chain_inlineable` decides before every call that this is the case,
    /// and the block form is the total fallback — a branch printer that
    /// can emit NOTHING is how a whole `else` once disappeared.
    fn branch_inline(&mut self, stmts: &[Stmt]) {
        match stmts {
            [Stmt::Expr(x)] => self.expr(x, 0),
            other => {
                let line = other.first().map(stmt_line).unwrap_or(0);
                self.stmt_block(other, line);
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

/// An `else` branch that is itself one if-expression, which prints as an
/// `else if` chain rather than as a braced block (see `if_expr`).
fn as_chain(els: &[Stmt]) -> Option<(&SExpr, &[Stmt], &[Stmt])> {
    match els {
        [Stmt::Expr(x)] => match &x.expr {
            Expr::IfExpr(c, t, e) => Some((c, t, e)),
            _ => None,
        },
        _ => None,
    }
}

/// Whether a whole if-expression chain fits the canonical one-line form:
/// every branch in it is a single expression. One statement-bearing branch
/// anywhere puts the entire chain in block form, so a `let` inside it has
/// somewhere to live.
fn chain_inlineable(then: &[Stmt], els: &[Stmt]) -> bool {
    if !matches!(then, [Stmt::Expr(_)]) {
        return false;
    }
    match as_chain(els) {
        Some((_, t2, e2)) => chain_inlineable(t2, e2),
        None => matches!(els, [Stmt::Expr(_)]),
    }
}

fn list_item_line(it: &ListItem) -> u32 {
    match it {
        ListItem::Item(x) | ListItem::Spread(x) => x.span.start.line,
    }
}

fn map_item_line(it: &MapItem) -> u32 {
    match it {
        MapItem::Entry(_, sp, _) => sp.start.line,
        MapItem::Spread(x) => x.span.start.line,
    }
}

fn stmt_line(s: &Stmt) -> u32 {
    match s {
        Stmt::Let(_, sp, _) | Stmt::Assign(_, sp, _) | Stmt::Return(_, sp) => sp.start.line,
        Stmt::If(c, _, _) => c.span.start.line,
        Stmt::For(vars, iter, _) => vars
            .first()
            .map(|(_, sp)| sp.start.line)
            .unwrap_or(iter.span.start.line),
        Stmt::Expr(x) => x.span.start.line,
    }
}

/// A map key that lexes as an identifier may print bare; anything else —
/// including reserved words, which would re-lex as keywords — stays quoted.
fn is_bare_key(k: &str) -> bool {
    const RESERVED: &[&str] = &[
        "space", "use", "part", "foreign", "state", "stored", "peruser", "setting",
        "append", "deep",
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
        // Property 3: comments preserved — count AND home.
        //
        // Count alone let two silent bugs through: a note above one map key
        // printing after the literal, and a note inside a one-line
        // expression printing above the NEXT property. Both preserved the
        // count and destroyed the sentence. A comment's home is the
        // declaration it was written in, so that is what gets compared.
        let before = comment_homes(name, src);
        let after = comment_homes(name, &formatted);
        assert_eq!(
            before, after,
            "{}: a comment changed the declaration it belongs to.\n--- formatted:\n{}",
            name, formatted
        );
    }

    /// Each comment paired with the declaration it belongs to.
    ///
    /// A comment INSIDE a declaration's source extent belongs to that
    /// declaration; one sitting between declarations documents the one that
    /// FOLLOWS it, which is what an own-line comment means everywhere. The
    /// distinction is the whole point: "the last declaration before it"
    /// cannot tell a note written inside `base`'s expression from one
    /// stranded between `base` and `other`, and those are exactly the two
    /// states this property has to separate.
    fn comment_homes(name: &str, src: &str) -> Vec<(String, String)> {
        let (toks, _) = crate::lexer::lex(name, src);
        let parsed = crate::parser::parse(name, &toks)
            .0
            .unwrap_or_else(|| panic!("{}: unparseable while locating comments", name));
        // (first line, last line, name), innermost first.
        let mut extents: Vec<(u32, u32, String)> = Vec::new();
        for part in &parsed.parts {
            let pname = part.name.join(".");
            for p in &part.props {
                let start = p.name_span.start.line;
                let end = p
                    .value
                    .as_ref()
                    .map(|v| v.span.end.line)
                    .unwrap_or(p.name_span.end.line);
                extents.push((start, end, format!("{}.{}", pname, p.name)));
            }
            extents.push((part.span.start.line, part.span.end.line, format!("part {}", pname)));
        }
        extract_comments(src)
            .iter()
            .map(|c| {
                let inside = extents
                    .iter()
                    .filter(|(s, e, _)| c.line >= *s && c.line <= *e)
                    .min_by_key(|(s, e, _)| e - s)
                    .map(|(_, _, n)| n.clone());
                let home = inside.unwrap_or_else(|| {
                    extents
                        .iter()
                        .filter(|(s, _, _)| *s > c.line)
                        .min_by_key(|(s, _, _)| *s)
                        .map(|(_, _, n)| format!("above {}", n))
                        .unwrap_or_else(|| "<file>".to_string())
                });
                (c.text.clone(), home)
            })
            .collect()
    }

    /// The shapes that once did not survive formatting, both of them
    /// silent. `else if` in EXPRESSION position was printed as
    /// `else { if ... }` — a statement block, whose value is `none` (§6) —
    /// so the first pass changed what the program returned and the second
    /// erased the branch entirely (`else {  }`), all while `ashlar check`
    /// stayed clean. A branch carrying a `let` fared worse: it printed as
    /// `{ ; doubled }`, dropping the binding and emitting a semicolon the
    /// language does not have.
    ///
    /// Both are caught by the three properties above — the AST fingerprint
    /// for the meaning change, idempotence for the erasure — so the fix is
    /// pinned by adding the inputs, not by asserting on bytes.
    #[test]
    fn if_expression_chains_and_statement_branches_survive_formatting() {
        assert_fmt_faithful(
            "elseif.ash",
            "space a\n\npart W {\n  pick = (n: number) => {\n    return if n > 9 {\n      \"high\"\n    } else if n < 0 {\n      \"low\"\n    } else {\n      \"mid\"\n    }\n  }\n}\n",
        );
        assert_fmt_faithful(
            "elseif-inline.ash",
            "space a\n\npart W {\n  pick = (n: number) => (if n > 9 { \"high\" } else if n < 0 { \"low\" } else { \"mid\" })\n}\n",
        );
        assert_fmt_faithful(
            "letbranch.ash",
            "space a\n\npart W {\n  pick = (n: number) => {\n    let out = if n > 9 {\n      let doubled = n * 2\n      doubled\n    } else {\n      0\n    }\n    return out\n  }\n}\n",
        );
        // The chain still prints on one line when every branch is one
        // expression: the fix must not blow every `if` into a block.
        let out = format_source(
            "t.ash",
            "space a\n\npart W {\n  pick = (n: number) => {\n    return if n > 9 {\n      \"high\"\n    } else if n < 0 {\n      \"low\"\n    } else {\n      \"mid\"\n    }\n  }\n}\n",
        )
        .unwrap();
        assert!(
            out.contains("return if n > 9 { \"high\" } else if n < 0 { \"low\" } else { \"mid\" }"),
            "{}",
            out
        );
    }

    /// A comment written between the parts of an expression that PRINTS on
    /// one line. There is no line to put it back on, so it used to move to
    /// the next declaration and document that instead — visible rather
    /// than silent, which is why it outlived the literal case, but wrong
    /// all the same. It now leaves as its own property's trailing comment.
    #[test]
    fn a_comment_inside_a_one_line_expression_stays_with_its_property() {
        let src = "space a\n\npart W {\n  base = 1 +\n    // the second term is the offset\n    2\n  other = 5\n}\n";
        assert_fmt_faithful("midexpr.ash", src);
        let out = format_source("t.ash", src).unwrap();
        assert!(
            out.contains("  base = 1 + 2  // the second term is the offset\n  other = 5"),
            "the comment must stay with `base`, not migrate onto `other`:\n{}",
            out
        );
    }

    /// A comment written inside a multi-line literal stays inside it. It
    /// used to sit in the queue until the next declaration opened and then
    /// print there, so a note about one map key came out documenting the
    /// property AFTER the literal — the count was preserved, the meaning
    /// of the sentence was not.
    #[test]
    fn comments_inside_literals_stay_where_they_were_written() {
        let src = "space a\n\npart W {\n  make = () => {\n    return {\n      a: 1,\n      // why b matters\n      b: 2,  // and a trailing one\n      c: 3,\n    }\n  }\n  tags = [\n    \"one\",\n    // the second tag names the policy\n    \"two\",\n  ]\n  other = () => 5\n}\n";
        assert_fmt_faithful("literal-comments.ash", src);
        let out = format_source("t.ash", src).unwrap();
        assert!(
            out.contains("      // why b matters\n      b: 2,  // and a trailing one"),
            "the comment must stay on the key it was written above:\n{}",
            out
        );
        assert!(
            out.contains("    // the second tag names the policy\n    \"two\","),
            "the same holds inside a list literal:\n{}",
            out
        );
        assert!(
            !out.contains("// why b matters\n  tags") && !out.contains("// the second tag names the policy\n  other"),
            "no comment may migrate onto a declaration it does not describe:\n{}",
            out
        );
    }

    fn ash_fenced_blocks(markdown: &str) -> Vec<String> {
        let mut blocks = Vec::new();
        let mut body = None;
        for line in markdown.lines() {
            match body.as_mut() {
                None if line == "```ash" => body = Some(String::new()),
                Some(_) if line == "```" => blocks.push(body.take().unwrap()),
                Some(body) => {
                    body.push_str(line);
                    body.push('\n');
                }
                None => {}
            }
        }
        assert!(body.is_none(), "unterminated ```ash block");
        blocks
    }

    #[test]
    fn ash_fenced_blocks_accepts_lf_and_crlf() {
        let lf = "before\n```ash\nspace demo\n```\nafter\n";
        let crlf = lf.replace('\n', "\r\n");
        assert_eq!(ash_fenced_blocks(lf), vec!["space demo\n"]);
        assert_eq!(ash_fenced_blocks(&crlf), vec!["space demo\n"]);
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
        // The CRLF-tolerant extractor is main's (#57); the path is this
        // branch's — the reference lives in AGENTS.md now (ADR-0031).
        let reference = std::fs::read_to_string(root.join("AGENTS.md")).unwrap();
        let blocks = ash_fenced_blocks(&reference);
        for (i, block) in blocks.iter().enumerate() {
            assert_fmt_faithful(&format!("refblock{}", i), block);
        }
        assert!(
            blocks.len() >= 10,
            "expected reference blocks, found {}",
            blocks.len()
        );

        // And every example. t_examples asserts `fmt(src) == src` over this
        // same tree, which is a fixpoint check on files that are ALREADY
        // canonical — it cannot see a construct the formatter mangles until
        // someone commits one. Running the three properties over the
        // examples closes that gap: the AST fingerprint would have caught
        // the `else if` meaning change the moment an example used one.
        let mut example_files = 0;
        for entry in std::fs::read_dir(root.join("examples")).unwrap() {
            let dir = entry.unwrap().path();
            if !dir.is_dir() {
                continue;
            }
            for file in crate::find_ash_files(&dir) {
                let src = std::fs::read_to_string(&file).unwrap();
                assert_fmt_faithful(&file.to_string_lossy(), &src);
                example_files += 1;
            }
        }
        assert!(
            example_files >= 20,
            "expected the example corpus, found {} files",
            example_files
        );
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

