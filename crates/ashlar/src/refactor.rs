//! Refactor commands (reference §12, requirements E1–E6): a refactor is a
//! command issued to the toolchain, never a text edit.
//!
//! The contract each command honors:
//!
//! * **E3** — the complete blast radius (every file/position/replacement)
//!   is computed first and reported before anything is applied.
//! * **E4** — application is atomic and reversible: edits are pure span
//!   substitutions, applied to an in-memory copy; the inverse command
//!   restores byte-identical sources (T-E proves the roundtrip).
//! * **E5** — a refactor that cannot compute its complete radius refuses
//!   with the reason and applies nothing. Today that includes: any
//!   diagnostic in the project (radius over broken code is undefined),
//!   renaming a data-shape field (map literals constructing the shape
//!   are not yet tracked), and a dotted reference chain that spans lines
//!   (prefix spans are computed by column arithmetic).
//! * **Post-verify** — after applying in memory, the project is re-checked;
//!   any new diagnostic rolls the whole refactor back (nothing is written).
//!
//! `rename` covers parts and non-field properties; `rekind` changes a
//! property's merge kind across every layer (the reference's escape hatch
//! from C5's identity rule).

use crate::ast::{self, Expr, FnBody, ListItem, MapItem, SExpr, SShape, Shape, Stmt};
use crate::resolved::Program;
use crate::tokens::{Pos, Span};
use std::collections::BTreeMap;

/// One planned replacement. Spans are half-open, columns count chars.
#[derive(Debug, Clone, PartialEq)]
pub struct Change {
    pub file: String,
    pub span: Span,
    pub old: String,
    pub new: String,
}

#[derive(Debug)]
pub struct Plan {
    pub description: String,
    pub changes: Vec<Change>,
}

/// A refusal: the reason radius could not be computed (E5).
#[derive(Debug)]
pub struct Refusal(pub String);

type PlanResult = Result<Plan, Refusal>;

// ---------------------------------------------------------------------------
// Planning: rename a part.
// ---------------------------------------------------------------------------

/// Rename part `old_full` (e.g. `chat.data.Message`) to bare `new_name`
/// in its home space. Touches: the bare declaration, every dotted layer
/// declaration, every resolved name reference, every shape reference.
pub fn plan_rename_part(
    sources: &[(String, String)],
    new_name: &str,
    old_full: &str,
) -> PlanResult {
    let checked = crate::check_sources(sources.to_vec());
    if !checked.diags.is_empty() {
        return Err(Refusal(format!(
            "the project has {} diagnostic(s); a refactor over unresolved code cannot compute its radius. Run `ashlar check` first.",
            checked.diags.len()
        )));
    }
    let program = &checked.program;
    let Some(info) = program.parts.get(old_full) else {
        return Err(Refusal(format!("`{}` is not a part in this program.", old_full)));
    };
    if old_full.starts_with("std.") {
        return Err(Refusal("`std` parts cannot be renamed.".to_string()));
    }
    if !valid_name(new_name) {
        return Err(Refusal(format!("`{}` is not a legal part name.", new_name)));
    }
    let home = info.home.clone();
    let new_full = format!("{}.{}", home, new_name);
    if program.parts.contains_key(&new_full) {
        return Err(Refusal(format!("`{}` already exists.", new_full)));
    }
    let old_bare = old_full.rsplit('.').next().unwrap_or(old_full).to_string();

    let mut changes: Vec<Change> = Vec::new();

    for (idx, entry) in program.files.iter().enumerate() {
        let space = ast::name_to_string(&entry.ast.space);
        let file = entry.path.clone();
        // Visibility: does `old_full` resolve here (home or in closure)?
        let sees = space == home
            || program
                .spaces
                .get(&space)
                .map(|s| s.closure.contains(&home))
                .unwrap_or(false);
        let bare_is_unambiguous = sees && bare_resolves_uniquely(program, &space, &old_bare, old_full);

        for part in &program.files[idx].ast.parts {
            // Declarations.
            let dotted = ast::name_to_string(&part.name);
            if part.name.len() == 1 && space == home && part.name[0] == old_bare {
                changes.push(Change {
                    file: file.clone(),
                    span: part.name_span,
                    old: old_bare.clone(),
                    new: new_name.to_string(),
                });
            } else if part.name.len() > 1 && dotted == old_full {
                changes.push(Change {
                    file: file.clone(),
                    span: part.name_span,
                    old: old_full.to_string(),
                    new: new_full.clone(),
                });
            }
            // Bodies and shapes.
            for prop in &part.props {
                if let Some(sh) = &prop.shape {
                    collect_shape(
                        sh, &file, old_full, &new_full, &old_bare, new_name,
                        bare_is_unambiguous, &mut changes,
                    )?;
                }
                if let Some(v) = &prop.value {
                    collect_expr(
                        v, &file, old_full, &new_full, &old_bare, new_name,
                        bare_is_unambiguous, &mut changes,
                    )?;
                }
            }
        }
        for fd in &program.files[idx].ast.foreigns {
            for (_, sh) in &fd.params {
                collect_shape(
                    sh, &file, old_full, &new_full, &old_bare, new_name,
                    bare_is_unambiguous, &mut changes,
                )?;
            }
            collect_shape(
                &fd.ret, &file, old_full, &new_full, &old_bare, new_name,
                bare_is_unambiguous, &mut changes,
            )?;
        }
    }

    changes.sort_by(|a, b| {
        (a.file.as_str(), a.span.start.line, a.span.start.col)
            .cmp(&(b.file.as_str(), b.span.start.line, b.span.start.col))
    });
    changes.dedup();
    Ok(Plan {
        description: format!("rename part `{}` -> `{}`", old_full, new_full),
        changes,
    })
}

/// Is a bare reference to `bare` in `space` guaranteed to mean `full`?
fn bare_resolves_uniquely(program: &Program, space: &str, bare: &str, full: &str) -> bool {
    let mut hits = 0;
    for (f, info) in &program.parts {
        let visible = info.home == space
            || program
                .spaces
                .get(space)
                .map(|s| s.closure.contains(&info.home))
                .unwrap_or(false);
        if visible && f.rsplit('.').next() == Some(bare) {
            hits += 1;
            if f != full && hits > 0 {
                // Another part answers to the bare name: the resolver would
                // have rejected bare uses, so none exist — and we must not
                // touch dotted chains that begin the same way.
            }
        }
    }
    hits == 1
}

fn valid_name(n: &str) -> bool {
    !n.is_empty()
        && !n.contains('.')
        && n.chars().next().map(|c| c.is_alphabetic() || c == '_').unwrap_or(false)
        && n.chars().all(|c| c.is_alphanumeric() || c == '_')
}

/// A NameRef whose leading segments match the part gets its prefix
/// rewritten via column arithmetic — refusing multi-line chains (E5).
fn collect_expr(
    e: &SExpr,
    file: &str,
    old_full: &str,
    new_full: &str,
    old_bare: &str,
    new_bare: &str,
    bare_ok: bool,
    out: &mut Vec<Change>,
) -> Result<(), Refusal> {
    match &e.expr {
        Expr::NameRef(segs) => {
            let full_segs: Vec<&str> = old_full.split('.').collect();
            let (matched_len, old_text, new_text) = if segs.len() >= full_segs.len()
                && segs[..full_segs.len()]
                    .iter()
                    .map(|s| s.as_str())
                    .eq(full_segs.iter().copied())
            {
                (full_segs.len(), old_full.to_string(), new_full.to_string())
            } else if bare_ok && segs[0] == old_bare {
                (1, old_bare.to_string(), new_bare.to_string())
            } else {
                (0, String::new(), String::new())
            };
            if matched_len > 0 {
                if e.span.start.line != e.span.end.line {
                    return Err(Refusal(format!(
                        "{}:{}: a dotted chain spans lines; prefix spans cannot be computed. Reformat with `ashlar fmt` first.",
                        file, e.span.start.line
                    )));
                }
                let start = e.span.start;
                let end = Pos {
                    line: start.line,
                    col: start.col + old_text.chars().count() as u32,
                };
                out.push(Change {
                    file: file.to_string(),
                    span: Span { start, end },
                    old: old_text,
                    new: new_text,
                });
            }
            Ok(())
        }
        Expr::List(items) => {
            for it in items {
                let x = match it {
                    ListItem::Item(x) | ListItem::Spread(x) => x,
                };
                collect_expr(x, file, old_full, new_full, old_bare, new_bare, bare_ok, out)?;
            }
            Ok(())
        }
        Expr::MapLit(items) => {
            for it in items {
                match it {
                    MapItem::Entry(_, _, v) => {
                        collect_expr(v, file, old_full, new_full, old_bare, new_bare, bare_ok, out)?
                    }
                    MapItem::Spread(x) => {
                        collect_expr(x, file, old_full, new_full, old_bare, new_bare, bare_ok, out)?
                    }
                }
            }
            Ok(())
        }
        Expr::Field(b, _, _) => collect_expr(b, file, old_full, new_full, old_bare, new_bare, bare_ok, out),
        Expr::Index(b, i) => {
            collect_expr(b, file, old_full, new_full, old_bare, new_bare, bare_ok, out)?;
            collect_expr(i, file, old_full, new_full, old_bare, new_bare, bare_ok, out)
        }
        Expr::Call(c, args) => {
            collect_expr(c, file, old_full, new_full, old_bare, new_bare, bare_ok, out)?;
            for a in args {
                collect_expr(a, file, old_full, new_full, old_bare, new_bare, bare_ok, out)?;
            }
            Ok(())
        }
        Expr::Unary(_, x) | Expr::Assert(x) => {
            collect_expr(x, file, old_full, new_full, old_bare, new_bare, bare_ok, out)
        }
        Expr::Binary(_, l, r) => {
            collect_expr(l, file, old_full, new_full, old_bare, new_bare, bare_ok, out)?;
            collect_expr(r, file, old_full, new_full, old_bare, new_bare, bare_ok, out)
        }
        Expr::IfExpr(c, t, els) => {
            collect_expr(c, file, old_full, new_full, old_bare, new_bare, bare_ok, out)?;
            for s in t.iter().chain(els.iter()) {
                collect_stmt(s, file, old_full, new_full, old_bare, new_bare, bare_ok, out)?;
            }
            Ok(())
        }
        Expr::FnLit(params, body) => {
            for p in params {
                collect_shape(&p.shape, file, old_full, new_full, old_bare, new_bare, bare_ok, out)?;
            }
            match body.as_ref() {
                FnBody::Expr(x) => {
                    collect_expr(x, file, old_full, new_full, old_bare, new_bare, bare_ok, out)
                }
                FnBody::Block(stmts) => {
                    for s in stmts {
                        collect_stmt(s, file, old_full, new_full, old_bare, new_bare, bare_ok, out)?;
                    }
                    Ok(())
                }
            }
        }
        _ => Ok(()),
    }
}

fn collect_stmt(
    s: &Stmt,
    file: &str,
    old_full: &str,
    new_full: &str,
    old_bare: &str,
    new_bare: &str,
    bare_ok: bool,
    out: &mut Vec<Change>,
) -> Result<(), Refusal> {
    match s {
        Stmt::Let(_, _, e) | Stmt::Assign(_, _, e) | Stmt::Return(Some(e), _) | Stmt::Expr(e) => {
            collect_expr(e, file, old_full, new_full, old_bare, new_bare, bare_ok, out)
        }
        Stmt::Return(None, _) => Ok(()),
        Stmt::If(c, t, els) => {
            collect_expr(c, file, old_full, new_full, old_bare, new_bare, bare_ok, out)?;
            for st in t.iter().chain(els.iter().flatten()) {
                collect_stmt(st, file, old_full, new_full, old_bare, new_bare, bare_ok, out)?;
            }
            Ok(())
        }
        Stmt::For(_, it, body) => {
            collect_expr(it, file, old_full, new_full, old_bare, new_bare, bare_ok, out)?;
            for st in body {
                collect_stmt(st, file, old_full, new_full, old_bare, new_bare, bare_ok, out)?;
            }
            Ok(())
        }
    }
}

fn collect_shape(
    sh: &SShape,
    file: &str,
    old_full: &str,
    new_full: &str,
    old_bare: &str,
    new_bare: &str,
    bare_ok: bool,
    out: &mut Vec<Change>,
) -> Result<(), Refusal> {
    match &sh.shape {
        Shape::Part(name) => {
            let text = ast::name_to_string(name);
            let (old_text, new_text) = if text == old_full {
                (old_full.to_string(), new_full.to_string())
            } else if bare_ok && name.len() == 1 && name[0] == old_bare {
                (old_bare.to_string(), new_bare.to_string())
            } else {
                return Ok(());
            };
            out.push(Change {
                file: file.to_string(),
                span: sh.span,
                old: old_text,
                new: new_text,
            });
            Ok(())
        }
        Shape::List(i) | Shape::Map(i) | Shape::Opt(i) => {
            collect_shape(i, file, old_full, new_full, old_bare, new_bare, bare_ok, out)
        }
        Shape::Fn(params, ret) => {
            for (_, p) in params {
                collect_shape(p, file, old_full, new_full, old_bare, new_bare, bare_ok, out)?;
            }
            collect_shape(ret, file, old_full, new_full, old_bare, new_bare, bare_ok, out)
        }
        _ => Ok(()),
    }
}

// ---------------------------------------------------------------------------
// Planning: rename a property.
// ---------------------------------------------------------------------------

/// Rename property `old` of part `part_full` to `new`. Fields of data
/// shapes are refused (E5): map literals constructing the shape are not
/// yet tracked, so their radius cannot be computed.
pub fn plan_rename_prop(
    sources: &[(String, String)],
    part_full: &str,
    old: &str,
    new: &str,
) -> PlanResult {
    let checked = crate::check_sources(sources.to_vec());
    if !checked.diags.is_empty() {
        return Err(Refusal(format!(
            "the project has {} diagnostic(s); run `ashlar check` first.",
            checked.diags.len()
        )));
    }
    let program = &checked.program;
    let Some(info) = program.parts.get(part_full) else {
        return Err(Refusal(format!("`{}` is not a part.", part_full)));
    };
    let Some(cp) = checked.composed.get(part_full) else {
        return Err(Refusal(format!("`{}` did not compose.", part_full)));
    };
    let Some(prop) = cp.props.get(old) else {
        return Err(Refusal(format!("`{}` has no property `{}`.", part_full, old)));
    };
    if !valid_name(new) {
        return Err(Refusal(format!("`{}` is not a legal property name.", new)));
    }
    if cp.props.contains_key(new) {
        return Err(Refusal(format!("`{}.{}` already exists.", part_full, new)));
    }
    // E5: a field (shape, no value, no storage on every definition) may be
    // constructed by map literals anywhere; that radius is not computable
    // yet, so the rename refuses rather than half-applying.
    let is_field = prop.storage.is_none()
        && matches!(prop.value, crate::resolved::MergedValue::FieldOnly);
    if is_field {
        return Err(Refusal(format!(
            "`{}.{}` is a data-shape field; literals constructing `{}` are not yet tracked, so the radius cannot be computed.",
            part_full, old, part_full
        )));
    }

    let mut changes: Vec<Change> = Vec::new();
    let layers = info.layers.clone();
    let old_bare = part_full.rsplit('.').next().unwrap_or(part_full).to_string();

    for (idx, entry) in program.files.iter().enumerate() {
        let file = entry.path.clone();
        let space = ast::name_to_string(&entry.ast.space);
        let bare_part_ok = bare_resolves_uniquely(program, &space, &old_bare, part_full);
        for (pi, part) in entry.ast.parts.iter().enumerate() {
            let is_layer = layers.iter().any(|l| l.file_idx == idx && l.part_idx == pi);
            if is_layer {
                for p in &part.props {
                    if p.name == old {
                        changes.push(Change {
                            file: file.clone(),
                            span: p.name_span,
                            old: old.to_string(),
                            new: new.to_string(),
                        });
                    }
                    if let Some(v) = &p.value {
                        collect_prop_refs_inside(
                            v,
                            &file,
                            old,
                            new,
                            matches!(
                                p.kind.as_ref().map(|k| k.kind),
                                Some(ast::MergeKind::Stack)
                            ),
                            &mut changes,
                        )?;
                    }
                }
            }
            // Chains `part_full.old` (and `Bare.old` where unambiguous)
            // anywhere in the program.
            for p in &part.props {
                if let Some(v) = &p.value {
                    collect_chain_prop_refs(
                        v, &file, part_full, &old_bare, bare_part_ok, old, new, &mut changes,
                    )?;
                }
            }
        }
    }

    changes.sort_by(|a, b| {
        (a.file.as_str(), a.span.start.line, a.span.start.col)
            .cmp(&(b.file.as_str(), b.span.start.line, b.span.start.col))
    });
    changes.dedup();
    Ok(Plan {
        description: format!("rename `{}.{}` -> `{}.{}`", part_full, old, part_full, new),
        changes,
    })
}

/// Inside the part's own layers: bare references (no shadowing exists in
/// Ashlar, so a bare match IS the property), assignment targets, and —
/// in `stack` properties — returned map-literal keys, which merge onto
/// state by name. A matching key anywhere else in a stack body refuses.
fn collect_prop_refs_inside(
    e: &SExpr,
    file: &str,
    old: &str,
    new: &str,
    in_stack: bool,
    out: &mut Vec<Change>,
) -> Result<(), Refusal> {
    fn walk_expr(
        e: &SExpr,
        file: &str,
        old: &str,
        new: &str,
        in_stack: bool,
        return_root: bool,
        out: &mut Vec<Change>,
    ) -> Result<(), Refusal> {
        match &e.expr {
            Expr::NameRef(segs) => {
                if segs[0] == old {
                    if e.span.start.line != e.span.end.line {
                        return Err(Refusal(format!(
                            "{}:{}: a dotted chain spans lines; run `ashlar fmt` first.",
                            file, e.span.start.line
                        )));
                    }
                    let start = e.span.start;
                    let end = Pos {
                        line: start.line,
                        col: start.col + old.chars().count() as u32,
                    };
                    out.push(Change {
                        file: file.to_string(),
                        span: Span { start, end },
                        old: old.to_string(),
                        new: new.to_string(),
                    });
                }
                Ok(())
            }
            Expr::MapLit(items) => {
                for it in items {
                    match it {
                        MapItem::Entry(k, kspan, v) => {
                            if k == old && in_stack {
                                if return_root {
                                    out.push(Change {
                                        file: file.to_string(),
                                        span: *kspan,
                                        old: old.to_string(),
                                        new: new.to_string(),
                                    });
                                } else {
                                    return Err(Refusal(format!(
                                        "{}:{}: a map key `{}` inside a `stack` body is not a direct `return` literal; its meaning cannot be decided.",
                                        file, kspan.start.line, old
                                    )));
                                }
                            }
                            walk_expr(v, file, old, new, in_stack, false, out)?;
                        }
                        MapItem::Spread(x) => walk_expr(x, file, old, new, in_stack, false, out)?,
                    }
                }
                Ok(())
            }
            Expr::List(items) => {
                for it in items {
                    let x = match it {
                        ListItem::Item(x) | ListItem::Spread(x) => x,
                    };
                    walk_expr(x, file, old, new, in_stack, false, out)?;
                }
                Ok(())
            }
            Expr::Field(b, _, _) => walk_expr(b, file, old, new, in_stack, false, out),
            Expr::Index(b, i) => {
                walk_expr(b, file, old, new, in_stack, false, out)?;
                walk_expr(i, file, old, new, in_stack, false, out)
            }
            Expr::Call(c, args) => {
                walk_expr(c, file, old, new, in_stack, false, out)?;
                for a in args {
                    walk_expr(a, file, old, new, in_stack, false, out)?;
                }
                Ok(())
            }
            Expr::Unary(_, x) | Expr::Assert(x) => walk_expr(x, file, old, new, in_stack, false, out),
            Expr::Binary(_, l, r) => {
                walk_expr(l, file, old, new, in_stack, false, out)?;
                walk_expr(r, file, old, new, in_stack, false, out)
            }
            Expr::IfExpr(c, t, els) => {
                walk_expr(c, file, old, new, in_stack, false, out)?;
                for s in t.iter().chain(els.iter()) {
                    walk_stmt(s, file, old, new, in_stack, out)?;
                }
                Ok(())
            }
            Expr::FnLit(_, body) => match body.as_ref() {
                FnBody::Expr(x) => walk_expr(x, file, old, new, in_stack, true, out),
                FnBody::Block(stmts) => {
                    for s in stmts {
                        walk_stmt(s, file, old, new, in_stack, out)?;
                    }
                    Ok(())
                }
            },
            _ => Ok(()),
        }
    }
    fn walk_stmt(
        s: &Stmt,
        file: &str,
        old: &str,
        new: &str,
        in_stack: bool,
        out: &mut Vec<Change>,
    ) -> Result<(), Refusal> {
        match s {
            Stmt::Assign(name, span, e) => {
                if name == old {
                    out.push(Change {
                        file: file.to_string(),
                        span: *span,
                        old: old.to_string(),
                        new: new.to_string(),
                    });
                }
                walk_expr(e, file, old, new, in_stack, false, out)
            }
            Stmt::Let(_, _, e) | Stmt::Expr(e) => walk_expr(e, file, old, new, in_stack, false, out),
            Stmt::Return(Some(e), _) => walk_expr(e, file, old, new, in_stack, true, out),
            Stmt::Return(None, _) => Ok(()),
            Stmt::If(c, t, els) => {
                walk_expr(c, file, old, new, in_stack, false, out)?;
                for st in t.iter().chain(els.iter().flatten()) {
                    walk_stmt(st, file, old, new, in_stack, out)?;
                }
                Ok(())
            }
            Stmt::For(_, it, body) => {
                walk_expr(it, file, old, new, in_stack, false, out)?;
                for st in body {
                    walk_stmt(st, file, old, new, in_stack, out)?;
                }
                Ok(())
            }
        }
    }
    walk_expr(e, file, old, new, in_stack, true, out)
}

/// Chains `part_full.prop` (or `Bare.prop` where the bare part name is
/// unambiguous) anywhere: rewrite the property segment by column math.
#[allow(clippy::too_many_arguments)]
fn collect_chain_prop_refs(
    e: &SExpr,
    file: &str,
    part_full: &str,
    part_bare: &str,
    bare_ok: bool,
    old: &str,
    new: &str,
    out: &mut Vec<Change>,
) -> Result<(), Refusal> {
    let mut stack = vec![e];
    while let Some(e) = stack.pop() {
        if let Expr::NameRef(segs) = &e.expr {
            let full_segs: Vec<&str> = part_full.split('.').collect();
            let prefix_len = if segs.len() > full_segs.len()
                && segs[..full_segs.len()].iter().map(|s| s.as_str()).eq(full_segs.iter().copied())
            {
                Some(full_segs.len())
            } else if bare_ok && segs.len() > 1 && segs[0] == part_bare {
                Some(1)
            } else {
                None
            };
            if let Some(k) = prefix_len {
                if segs[k] == old {
                    if e.span.start.line != e.span.end.line {
                        return Err(Refusal(format!(
                            "{}:{}: a dotted chain spans lines; run `ashlar fmt` first.",
                            file, e.span.start.line
                        )));
                    }
                    let prefix_chars: u32 = segs[..k]
                        .iter()
                        .map(|s| s.chars().count() as u32 + 1)
                        .sum();
                    let start = Pos {
                        line: e.span.start.line,
                        col: e.span.start.col + prefix_chars,
                    };
                    let end = Pos {
                        line: start.line,
                        col: start.col + old.chars().count() as u32,
                    };
                    out.push(Change {
                        file: file.to_string(),
                        span: Span { start, end },
                        old: old.to_string(),
                        new: new.to_string(),
                    });
                }
            }
        }
        push_children(e, &mut stack);
    }
    Ok(())
}

/// Push every child expression of `e` (statements included) onto `stack`.
fn push_children<'a>(e: &'a SExpr, stack: &mut Vec<&'a SExpr>) {
    fn stmt_children<'a>(s: &'a Stmt, stack: &mut Vec<&'a SExpr>) {
        match s {
            Stmt::Let(_, _, e) | Stmt::Assign(_, _, e) | Stmt::Return(Some(e), _) | Stmt::Expr(e) => {
                stack.push(e)
            }
            Stmt::Return(None, _) => {}
            Stmt::If(c, t, els) => {
                stack.push(c);
                for st in t.iter().chain(els.iter().flatten()) {
                    stmt_children(st, stack);
                }
            }
            Stmt::For(_, it, body) => {
                stack.push(it);
                for st in body {
                    stmt_children(st, stack);
                }
            }
        }
    }
    match &e.expr {
        Expr::List(items) => {
            for it in items {
                match it {
                    ListItem::Item(x) | ListItem::Spread(x) => stack.push(x),
                }
            }
        }
        Expr::MapLit(items) => {
            for it in items {
                match it {
                    MapItem::Entry(_, _, v) => stack.push(v),
                    MapItem::Spread(x) => stack.push(x),
                }
            }
        }
        Expr::Field(b, _, _) => stack.push(b),
        Expr::Index(b, i) => {
            stack.push(b);
            stack.push(i);
        }
        Expr::Call(c, args) => {
            stack.push(c);
            for a in args {
                stack.push(a);
            }
        }
        Expr::Unary(_, x) | Expr::Assert(x) => stack.push(x),
        Expr::Binary(_, l, r) => {
            stack.push(l);
            stack.push(r);
        }
        Expr::IfExpr(c, t, els) => {
            stack.push(c);
            for s in t.iter().chain(els.iter()) {
                stmt_children(s, stack);
            }
        }
        Expr::FnLit(_, body) => match body.as_ref() {
            FnBody::Expr(x) => stack.push(x),
            FnBody::Block(stmts) => {
                for s in stmts {
                    stmt_children(s, stack);
                }
            }
        },
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// Planning: rekind.
// ---------------------------------------------------------------------------

/// Change `part_full.prop`'s merge kind on every layer (reference §4's
/// `rekind`). `new_kind` is `replace`, `append`, `deep`, `stack`,
/// `pipe`, `stack reverse`, or `pipe reverse`. Post-verify (in
/// `execute`) rolls back anything the checker rejects.
pub fn plan_rekind(
    sources: &[(String, String)],
    part_full: &str,
    prop: &str,
    new_kind: &str,
) -> PlanResult {
    let allowed = [
        "replace", "append", "deep", "stack", "pipe", "stack reverse", "pipe reverse",
    ];
    if !allowed.contains(&new_kind) {
        return Err(Refusal(format!(
            "`{}` is not a merge kind (use one of: {}).",
            new_kind,
            allowed.join(", ")
        )));
    }
    let checked = crate::check_sources(sources.to_vec());
    if !checked.diags.is_empty() {
        return Err(Refusal(format!(
            "the project has {} diagnostic(s); run `ashlar check` first.",
            checked.diags.len()
        )));
    }
    let program = &checked.program;
    let Some(info) = program.parts.get(part_full) else {
        return Err(Refusal(format!("`{}` is not a part.", part_full)));
    };

    let mut changes = Vec::new();
    let mut touched = false;
    for l in &info.layers {
        let file = program.files[l.file_idx].path.clone();
        for p in &program.files[l.file_idx].ast.parts[l.part_idx].props {
            if p.name != prop {
                continue;
            }
            touched = true;
            match (&p.kind, new_kind) {
                (Some(k), "replace") => {
                    // Delete `" kind"`: from the name's end to the kind's end.
                    changes.push(Change {
                        file: file.clone(),
                        span: Span {
                            start: p.name_span.end,
                            end: k.span.end,
                        },
                        old: format!(" {}", kind_source_text(k)),
                        new: String::new(),
                    });
                }
                (Some(k), nk) => {
                    changes.push(Change {
                        file: file.clone(),
                        span: k.span,
                        old: kind_source_text(k),
                        new: nk.to_string(),
                    });
                }
                (None, "replace") => {}
                (None, nk) => {
                    changes.push(Change {
                        file: file.clone(),
                        span: Span {
                            start: p.name_span.end,
                            end: p.name_span.end,
                        },
                        old: String::new(),
                        new: format!(" {}", nk),
                    });
                }
            }
        }
    }
    if !touched {
        return Err(Refusal(format!("`{}` has no property `{}`.", part_full, prop)));
    }
    changes.sort_by(|a, b| {
        (a.file.as_str(), a.span.start.line, a.span.start.col)
            .cmp(&(b.file.as_str(), b.span.start.line, b.span.start.col))
    });
    Ok(Plan {
        description: format!("rekind `{}.{}` -> `{}`", part_full, prop, new_kind),
        changes,
    })
}

fn kind_source_text(k: &ast::KindDecl) -> String {
    let base = match k.kind {
        ast::MergeKind::Append => "append",
        ast::MergeKind::Deep => "deep",
        ast::MergeKind::Stack => "stack",
        ast::MergeKind::Pipe => "pipe",
    };
    if k.reverse {
        format!("{} reverse", base)
    } else {
        base.to_string()
    }
}

// ---------------------------------------------------------------------------
// Applying: atomic, verified, reversible.
// ---------------------------------------------------------------------------

/// Apply a plan to sources, in memory, then re-check. Any diagnostic in
/// the result rolls the whole refactor back (E4/E5: nothing partial is
/// ever produced).
pub fn execute(
    sources: &[(String, String)],
    plan: &Plan,
) -> Result<BTreeMap<String, String>, Refusal> {
    let mut by_file: BTreeMap<String, String> = sources.iter().cloned().collect();
    let mut per_file: BTreeMap<String, Vec<&Change>> = BTreeMap::new();
    for c in &plan.changes {
        per_file.entry(c.file.clone()).or_default().push(c);
    }
    for (file, mut changes) in per_file {
        let Some(src) = by_file.get(&file) else {
            return Err(Refusal(format!("internal: `{}` not in sources.", file)));
        };
        changes.sort_by(|a, b| {
            (b.span.start.line, b.span.start.col).cmp(&(a.span.start.line, a.span.start.col))
        });
        let mut text = src.clone();
        for c in changes {
            let start = char_pos_to_byte(&text, c.span.start);
            let end = char_pos_to_byte(&text, c.span.end);
            let actual: String = text[start..end].to_string();
            if actual != c.old {
                return Err(Refusal(format!(
                    "{}:{}:{}: expected `{}` at the change site, found `{}` — refusing.",
                    file, c.span.start.line, c.span.start.col, c.old, actual
                )));
            }
            text.replace_range(start..end, &c.new);
        }
        by_file.insert(file, text);
    }

    let verify = crate::check_sources(by_file.iter().map(|(k, v)| (k.clone(), v.clone())).collect());
    if !verify.diags.is_empty() {
        return Err(Refusal(format!(
            "the refactor would introduce {} diagnostic(s) (first: {}); rolled back, nothing written.",
            verify.diags.len(),
            verify.diags[0].human()
        )));
    }
    Ok(by_file)
}

/// 1-based (line, char-col) to byte offset.
fn char_pos_to_byte(src: &str, pos: Pos) -> usize {
    let mut line = 1u32;
    let mut col = 1u32;
    for (i, ch) in src.char_indices() {
        if line == pos.line && col == pos.col {
            return i;
        }
        if ch == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    src.len()
}
