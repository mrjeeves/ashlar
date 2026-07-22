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
//!   diagnostic in the project (radius over broken code is undefined)
//!   and a dotted reference chain that spans lines (prefix spans are
//!   computed by column arithmetic; `ashlar fmt` removes the obstacle).
//! * **Post-verify** — after applying in memory, the project is re-checked;
//!   any new diagnostic rolls the whole refactor back (nothing is written).
//!
//! `rename` covers spaces, parts, and properties — data-shape and view
//! fields included: the checker's field-site index (every literal key,
//! `el` key, and known-base access) supplies exactly the occurrences a
//! textual search cannot prove. `rekind` changes a property's merge kind
//! across every layer (the reference's escape hatch from C5's identity
//! rule). `move` relocates a part's home declaration to another space,
//! adding the `use` lines both sides need (it never removes any): forward
//! then back is byte-identical when the part sits at the canonical
//! position (end of file) and neither direction needs a `use` addition —
//! the position caveat is ADR-0009's recorded trade.

use crate::ast::{self, Expr, FnBody, ListItem, MapItem, SExpr, SShape, Shape, Stmt};
use crate::resolved::Program;
use crate::tokens::{Pos, Span};
use std::collections::{BTreeMap, BTreeSet};

/// One planned replacement. Spans are half-open, columns count chars.
#[derive(Debug, Clone, PartialEq)]
pub struct Change {
    pub file: String,
    pub span: Span,
    pub old: String,
    pub new: String,
}

#[derive(Debug, Default)]
pub struct Plan {
    pub description: String,
    pub changes: Vec<Change>,
    /// `.ashlar-state.json` migrations at the part level: every stored key
    /// `old.prop` becomes `new.prop` (ADR-0007's orphaned-rows note,
    /// closed). Applied by the CLI after sources verify.
    pub state_part_renames: Vec<(String, String)>,
    /// Exact stored-key migrations (`space.Part.prop` -> same with the
    /// property renamed).
    pub state_prop_renames: Vec<(String, String)>,
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
    let stored = checked
        .composed
        .get(old_full)
        .map(|cp| {
            cp.props
                .values()
                .any(|p| matches!(p.storage, Some(ast::Storage::Stored)))
        })
        .unwrap_or(false);
    Ok(Plan {
        description: format!("rename part `{}` -> `{}`", old_full, new_full),
        changes,
        state_part_renames: if stored {
            vec![(old_full.to_string(), new_full.clone())]
        } else {
            vec![]
        },
        state_prop_renames: vec![],
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
    let mut changes: Vec<Change> = Vec::new();

    // The checker's field-site index: literal keys checked against the
    // part (data-shape construction, `put` values, `el` field maps) and
    // field accesses on values whose shape the checker knows. This is
    // what makes field renames computable (E5, ADR-0009); sites the
    // checker could not pin to a span (multi-line chains) refuse.
    for site in &checked.field_sites {
        if site.part != part_full || site.field != old {
            continue;
        }
        if !site.precise {
            return Err(Refusal(format!(
                "{}:{}: a dotted chain spans lines; run `ashlar fmt` first.",
                site.file, site.span.start.line
            )));
        }
        changes.push(Change {
            file: site.file.clone(),
            span: site.span,
            old: old.to_string(),
            new: new.to_string(),
        });
    }
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
    let stored = matches!(prop.storage, Some(ast::Storage::Stored));
    Ok(Plan {
        description: format!("rename `{}.{}` -> `{}.{}`", part_full, old, part_full, new),
        changes,
        state_part_renames: vec![],
        state_prop_renames: if stored {
            vec![(
                format!("{}.{}", part_full, old),
                format!("{}.{}", part_full, new),
            )]
        } else {
            vec![]
        },
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
        state_part_renames: vec![],
        state_prop_renames: vec![],
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
// Planning: rename a space.
// ---------------------------------------------------------------------------

/// Rename space `old` to `new` (reference §11: `rename` covers spaces).
/// Touches: every `space` header declaring it, every `use` naming it, every
/// dotted layer declaration under it, and every full-name reference —
/// expression chains, shape positions, foreign-function chains. Bare
/// references never change (visibility is preserved edge-for-edge), so the
/// plan is pure prefix substitution and forward-then-back is
/// byte-identical.
pub fn plan_rename_space(
    sources: &[(String, String)],
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
    if old == "std" {
        return Err(Refusal("`std` cannot be renamed.".to_string()));
    }
    if !program.spaces.contains_key(old) {
        return Err(Refusal(format!("`{}` is not a space in this program.", old)));
    }
    let new_ok = !new.is_empty()
        && new != "std"
        && new.split('.').all(|seg| valid_name(seg));
    if !new_ok {
        return Err(Refusal(format!("`{}` is not a legal space name.", new)));
    }
    if program.spaces.contains_key(new) {
        return Err(Refusal(format!(
            "space `{}` already exists; renaming `{}` onto it would merge them.",
            new, old
        )));
    }
    let old_segs: Vec<&str> = old.split('.').collect();

    let mut changes: Vec<Change> = Vec::new();
    for entry in &program.files {
        let file = entry.path.clone();
        // The header.
        if ast::name_to_string(&entry.ast.space) == old {
            changes.push(Change {
                file: file.clone(),
                span: entry.ast.space_span,
                old: old.to_string(),
                new: new.to_string(),
            });
        }
        // `use` lines.
        for (name, span) in &entry.ast.uses {
            if ast::name_to_string(name) == old {
                changes.push(Change {
                    file: file.clone(),
                    span: *span,
                    old: old.to_string(),
                    new: new.to_string(),
                });
            }
        }
        for part in &entry.ast.parts {
            // Dotted layer declarations `part old.P { ... }`.
            if part.name.len() == old_segs.len() + 1
                && part.name[..old_segs.len()].iter().map(|s| s.as_str()).eq(old_segs.iter().copied())
            {
                prefix_change(&file, part.name_span, old, new, &mut changes)?;
            }
            for prop in &part.props {
                if let Some(sh) = &prop.shape {
                    space_prefix_in_shape(sh, &file, program, old, &old_segs, new, &mut changes)?;
                }
                if let Some(v) = &prop.value {
                    space_prefix_in_expr(v, &file, program, old, &old_segs, new, &mut changes)?;
                }
            }
        }
        for fd in &entry.ast.foreigns {
            for (_, sh) in &fd.params {
                space_prefix_in_shape(sh, &file, program, old, &old_segs, new, &mut changes)?;
            }
            space_prefix_in_shape(&fd.ret, &file, program, old, &old_segs, new, &mut changes)?;
        }
    }

    changes.sort_by(|a, b| {
        (a.file.as_str(), a.span.start.line, a.span.start.col)
            .cmp(&(b.file.as_str(), b.span.start.line, b.span.start.col))
    });
    changes.dedup();

    // Every stored key under a part homed in the space migrates.
    let mut state_part_renames = Vec::new();
    for (full, info) in &program.parts {
        if info.home == old {
            let bare = full.rsplit('.').next().unwrap_or(full);
            state_part_renames.push((full.clone(), format!("{}.{}", new, bare)));
        }
    }

    Ok(Plan {
        description: format!("rename space `{}` -> `{}`", old, new),
        changes,
        state_part_renames,
        state_prop_renames: vec![],
    })
}

/// A prefix rewrite of `old` at the start of `span` (single-line only).
fn prefix_change(
    file: &str,
    span: Span,
    old: &str,
    new: &str,
    out: &mut Vec<Change>,
) -> Result<(), Refusal> {
    if span.start.line != span.end.line {
        return Err(Refusal(format!(
            "{}:{}: a dotted chain spans lines; run `ashlar fmt` first.",
            file, span.start.line
        )));
    }
    let start = span.start;
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
    Ok(())
}

/// The longest leading run of `segs` that names a part or foreign
/// function, mirroring the resolver's longest-prefix rule. Returns the
/// full name and how many segments it spans.
fn resolved_full_prefix(program: &Program, segs: &[String]) -> Option<(String, usize)> {
    for k in (2..=segs.len()).rev() {
        let prefix = segs[..k].join(".");
        if program.parts.contains_key(&prefix) || program.foreigns.contains_key(&prefix) {
            return Some((prefix, k));
        }
    }
    None
}

/// Does `segs` resolve (longest prefix first, like the resolver) to a name
/// homed DIRECTLY in space `old`? Only then is the leading run of segments
/// a reference to the space — `old.b.c` may instead resolve into a space
/// named `old.b`, which a rename of `old` must not touch.
fn segs_reference_space(
    program: &Program,
    old_segs: &[&str],
    segs: &[String],
) -> bool {
    match resolved_full_prefix(program, segs) {
        Some((_, k)) => {
            k == old_segs.len() + 1
                && segs[..old_segs.len()].iter().map(|s| s.as_str()).eq(old_segs.iter().copied())
        }
        None => false,
    }
}

fn space_prefix_in_expr(
    e: &SExpr,
    file: &str,
    program: &Program,
    old: &str,
    old_segs: &[&str],
    new: &str,
    out: &mut Vec<Change>,
) -> Result<(), Refusal> {
    let mut stack = vec![e];
    while let Some(e) = stack.pop() {
        if let Expr::NameRef(segs) = &e.expr {
            if segs_reference_space(program, old_segs, segs) {
                prefix_change(file, e.span, old, new, out)?;
            }
        }
        if let Expr::FnLit(params, _) = &e.expr {
            for p in params {
                space_prefix_in_shape(&p.shape, file, program, old, old_segs, new, out)?;
            }
        }
        push_children(e, &mut stack);
    }
    Ok(())
}

fn space_prefix_in_shape(
    sh: &SShape,
    file: &str,
    program: &Program,
    old: &str,
    old_segs: &[&str],
    new: &str,
    out: &mut Vec<Change>,
) -> Result<(), Refusal> {
    match &sh.shape {
        Shape::Part(name) => {
            if segs_reference_space(program, old_segs, name)
                || (name.len() == old_segs.len() + 1
                    && name[..old_segs.len()].iter().map(|s| s.as_str()).eq(old_segs.iter().copied())
                    && program.parts.contains_key(&ast::name_to_string(name)))
            {
                prefix_change(file, sh.span, old, new, out)?;
            }
            Ok(())
        }
        Shape::List(i) | Shape::Map(i) | Shape::Opt(i) => {
            space_prefix_in_shape(i, file, program, old, old_segs, new, out)
        }
        Shape::Fn(params, ret) => {
            for (_, p) in params {
                space_prefix_in_shape(p, file, program, old, old_segs, new, out)?;
            }
            space_prefix_in_shape(ret, file, program, old, old_segs, new, out)
        }
        _ => Ok(()),
    }
}

// ---------------------------------------------------------------------------
// Planning: move a part to another space.
// ---------------------------------------------------------------------------

/// Move `part_full`'s home declaration into `target` space (E6: without
/// this, relocating a part is a text edit). The plan: excise the home
/// block (with its preceding blank line), append it canonically to the
/// target space's first file, rewrite every full-name reference and dotted
/// layer declaration, and ADD every `use` line either side newly needs —
/// the moved body's dependencies for the target space, `use target` for
/// every space that references the part. `move` never REMOVES a `use`:
/// stale breadth is harmless, silent breakage is not (ADR-0009).
pub fn plan_move(
    sources: &[(String, String)],
    part_full: &str,
    target: &str,
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
        return Err(Refusal(format!("`{}` is not a part in this program.", part_full)));
    };
    if part_full.starts_with("std.") || target == "std" {
        return Err(Refusal("`std` takes no part in moves.".to_string()));
    }
    let home = info.home.clone();
    if home == target {
        return Err(Refusal(format!("`{}` already lives in `{}`.", part_full, target)));
    }
    if !program.spaces.contains_key(target) {
        return Err(Refusal(format!(
            "`{}` is not a space in this program; declare it first (a file starting `space {}`).",
            target, target
        )));
    }
    let bare = part_full.rsplit('.').next().unwrap_or(part_full).to_string();
    let new_full = format!("{}.{}", target, bare);
    if program.parts.contains_key(&new_full) {
        return Err(Refusal(format!("`{}` already exists.", new_full)));
    }

    // The home declaration: the one bare-named layer in the home space.
    let home_layer = info.layers.iter().find(|l| {
        l.space == home && program.part_decl(l).name.len() == 1
    });
    let Some(home_layer) = home_layer else {
        return Err(Refusal(format!("`{}` has no home declaration.", part_full)));
    };
    let decl = program.part_decl(home_layer);
    let home_file = program.file_path(home_layer).to_string();
    let Some((_, home_src)) = sources.iter().find(|(p, _)| *p == home_file) else {
        return Err(Refusal(format!("internal: `{}` not in sources.", home_file)));
    };

    // 1. Reference rewrites everywhere — full-name chains, shape
    // positions, dotted layer declarations — including inside the moving
    // block itself (those are split out below and applied to the copy).
    let mut changes: Vec<Change> = Vec::new();
    for entry in &program.files {
        let file = entry.path.clone();
        for part in &entry.ast.parts {
            if ast::name_to_string(&part.name) == *part_full {
                prefix_change(&file, part.name_span, &home, target, &mut changes)?;
            }
            for prop in &part.props {
                if let Some(sh) = &prop.shape {
                    part_prefix_in_shape(sh, &file, part_full, &home, target, &mut changes)?;
                }
                if let Some(v) = &prop.value {
                    part_prefix_in_expr(v, &file, program, part_full, &home, target, &mut changes)?;
                }
            }
        }
        for fd in &entry.ast.foreigns {
            for (_, sh) in &fd.params {
                part_prefix_in_shape(sh, &file, part_full, &home, target, &mut changes)?;
            }
            part_prefix_in_shape(&fd.ret, &file, part_full, &home, target, &mut changes)?;
        }
    }

    // 2. Split out rewrites inside the moving block: they apply to the
    // appended COPY — the cut's byte verification needs the original text.
    let first = decl.span.start.line; // 1-based
    let last = decl.span.end.line;
    let mut inside: Vec<Change> = Vec::new();
    changes.retain(|c| {
        let within = c.file == home_file
            && c.span.start.line >= first
            && c.span.end.line <= last;
        if within {
            inside.push(c.clone());
        }
        !within
    });

    // 3. Excise the block (plus one preceding blank line when present).
    let lines: Vec<&str> = home_src.split('\n').collect();
    let first = first as usize;
    let last = last as usize;
    let take_blank_before =
        first >= 2 && lines.get(first - 2).map(|l| l.trim().is_empty()).unwrap_or(false);
    let cut_start_line = if take_blank_before { first - 1 } else { first };
    let block_text: String = lines[(first - 1)..last].join("\n");
    let mut old_text = String::new();
    if take_blank_before {
        old_text.push_str(lines[cut_start_line - 1]);
        old_text.push('\n');
    }
    old_text.push_str(&block_text);
    let cut_end = if last < lines.len() {
        old_text.push('\n');
        Pos {
            line: (last + 1) as u32,
            col: 1,
        }
    } else {
        Pos {
            line: last as u32,
            col: lines[last - 1].chars().count() as u32 + 1,
        }
    };
    changes.push(Change {
        file: home_file.clone(),
        span: Span {
            start: Pos {
                line: cut_start_line as u32,
                col: 1,
            },
            end: cut_end,
        },
        old: old_text,
        new: String::new(),
    });

    // 4. Apply the inside-rewrites to the copy, bottom-up.
    let mut block_lines: Vec<String> =
        lines[(first - 1)..last].iter().map(|s| s.to_string()).collect();
    inside.sort_by(|a, b| {
        (b.span.start.line, b.span.start.col).cmp(&(a.span.start.line, a.span.start.col))
    });
    inside.dedup();
    for c in &inside {
        let li = (c.span.start.line as usize) - first;
        let chars: Vec<char> = block_lines[li].chars().collect();
        let scol = (c.span.start.col - 1) as usize;
        let ecol = (c.span.end.col - 1) as usize;
        let mid: String = chars[scol..ecol].iter().collect();
        if mid != c.old {
            return Err(Refusal(format!(
                "internal: expected `{}` inside the moved block, found `{}`.",
                c.old, mid
            )));
        }
        let prefix: String = chars[..scol].iter().collect();
        let suffix: String = chars[ecol..].iter().collect();
        block_lines[li] = format!("{}{}{}", prefix, c.new, suffix);
    }
    let block_rewritten = block_lines.join("\n");

    // 5. Append the copy canonically to the target space's first file.
    let target_file = program
        .spaces
        .get(target)
        .and_then(|s| s.files.first())
        .cloned()
        .ok_or_else(|| Refusal(format!("`{}` has no files.", target)))?;
    let Some((_, target_src)) = sources.iter().find(|(p, _)| *p == target_file) else {
        return Err(Refusal(format!("internal: `{}` not in sources.", target_file)));
    };
    let t_lines: Vec<&str> = target_src.split('\n').collect();
    let (append_pos, mut append_text) = if target_src.ends_with('\n') {
        (
            Pos {
                line: t_lines.len() as u32,
                col: 1,
            },
            String::new(),
        )
    } else {
        (
            Pos {
                line: t_lines.len() as u32,
                col: t_lines.last().map(|l| l.chars().count() as u32 + 1).unwrap_or(1),
            },
            "\n".to_string(),
        )
    };
    append_text.push('\n');
    append_text.push_str(&block_rewritten);
    append_text.push('\n');
    changes.push(Change {
        file: target_file.clone(),
        span: Span {
            start: append_pos,
            end: append_pos,
        },
        old: String::new(),
        new: append_text,
    });

    // 6. `use` additions. (a) The target space must see everything the
    // moved body references.
    let mut needed: BTreeSet<String> = BTreeSet::new();
    let target_closure = program
        .spaces
        .get(target)
        .map(|s| s.closure.clone())
        .unwrap_or_default();
    collect_referenced_spaces(program, decl, &home, &mut needed);
    needed.remove(target);
    needed.remove("std");
    needed.retain(|s| !target_closure.contains(s));
    needed.remove(part_full); // safety: never a space
    if !needed.is_empty() {
        add_use_lines(program, target, &needed, &mut changes)?;
    }

    // (b) Every space that references the part must see the target. The
    // moving declaration itself is not a reference from home — it leaves.
    let mut referencing: BTreeSet<String> = BTreeSet::new();
    for entry in &program.files {
        let space = ast::name_to_string(&entry.ast.space);
        let mut refs = false;
        for part in &entry.ast.parts {
            let is_moved_decl = entry.path == home_file
                && part.name.len() == 1
                && part.name[0] == bare
                && space == home;
            if is_moved_decl {
                continue;
            }
            if ast::name_to_string(&part.name) == *part_full {
                refs = true;
            }
            for prop in &part.props {
                if let Some(sh) = &prop.shape {
                    refs = refs || shape_references_part(sh, part_full, &bare);
                }
                if let Some(v) = &prop.value {
                    refs = refs || expr_references_part(v, part_full, &bare);
                }
            }
        }
        if refs && space != target {
            referencing.insert(space);
        }
    }
    for space in referencing {
        let closure = program
            .spaces
            .get(&space)
            .map(|s| s.closure.clone())
            .unwrap_or_default();
        if space != target && !closure.contains(target) {
            let mut one = BTreeSet::new();
            one.insert(target.to_string());
            add_use_lines(program, &space, &one, &mut changes)?;
        }
    }

    changes.sort_by(|a, b| {
        (a.file.as_str(), a.span.start.line, a.span.start.col)
            .cmp(&(b.file.as_str(), b.span.start.line, b.span.start.col))
    });
    changes.dedup();

    let stored = checked
        .composed
        .get(part_full)
        .map(|cp| {
            cp.props
                .values()
                .any(|p| matches!(p.storage, Some(ast::Storage::Stored)))
        })
        .unwrap_or(false);
    Ok(Plan {
        description: format!("move `{}` -> `{}`", part_full, new_full),
        changes,
        state_part_renames: if stored {
            vec![(part_full.to_string(), new_full)]
        } else {
            vec![]
        },
        state_prop_renames: vec![],
    })
}

/// Full-name references `home.P...` in expressions -> `target.P...`,
/// with the resolver's longest-prefix discipline (a longer part name
/// beginning the same way wins and is left alone).
fn part_prefix_in_expr(
    e: &SExpr,
    file: &str,
    program: &Program,
    part_full: &str,
    home: &str,
    target: &str,
    out: &mut Vec<Change>,
) -> Result<(), Refusal> {
    let mut stack = vec![e];
    while let Some(e) = stack.pop() {
        if let Expr::NameRef(segs) = &e.expr {
            if let Some((full, _)) = resolved_full_prefix(program, segs) {
                if full == part_full {
                    prefix_change(file, e.span, home, target, out)?;
                }
            }
        }
        if let Expr::FnLit(params, _) = &e.expr {
            for p in params {
                part_prefix_in_shape(&p.shape, file, part_full, home, target, out)?;
            }
        }
        push_children(e, &mut stack);
    }
    Ok(())
}

fn part_prefix_in_shape(
    sh: &SShape,
    file: &str,
    part_full: &str,
    home: &str,
    target: &str,
    out: &mut Vec<Change>,
) -> Result<(), Refusal> {
    match &sh.shape {
        Shape::Part(name) => {
            if ast::name_to_string(name) == *part_full {
                prefix_change(file, sh.span, home, target, out)?;
            }
            Ok(())
        }
        Shape::List(i) | Shape::Map(i) | Shape::Opt(i) => {
            part_prefix_in_shape(i, file, part_full, home, target, out)
        }
        Shape::Fn(params, ret) => {
            for (_, p) in params {
                part_prefix_in_shape(p, file, part_full, home, target, out)?;
            }
            part_prefix_in_shape(ret, file, part_full, home, target, out)
        }
        _ => Ok(()),
    }
}

fn expr_references_part(e: &SExpr, part_full: &str, bare: &str) -> bool {
    let full_segs: Vec<&str> = part_full.split('.').collect();
    let mut stack = vec![e];
    while let Some(e) = stack.pop() {
        if let Expr::NameRef(segs) = &e.expr {
            if segs[0] == bare
                || (segs.len() >= full_segs.len()
                    && segs[..full_segs.len()].iter().map(|s| s.as_str()).eq(full_segs.iter().copied()))
            {
                return true;
            }
        }
        if let Expr::FnLit(params, _) = &e.expr {
            for p in params {
                if shape_references_part(&p.shape, part_full, bare) {
                    return true;
                }
            }
        }
        push_children(e, &mut stack);
    }
    false
}

fn shape_references_part(sh: &SShape, part_full: &str, bare: &str) -> bool {
    match &sh.shape {
        Shape::Part(name) => {
            ast::name_to_string(name) == *part_full || (name.len() == 1 && name[0] == bare)
        }
        Shape::List(i) | Shape::Map(i) | Shape::Opt(i) => {
            shape_references_part(i, part_full, bare)
        }
        Shape::Fn(params, ret) => {
            params.iter().any(|(_, p)| shape_references_part(p, part_full, bare))
                || shape_references_part(ret, part_full, bare)
        }
        _ => false,
    }
}

/// The home spaces of every name the declaration's bodies and shapes
/// resolve to, from `from_space`'s point of view.
fn collect_referenced_spaces(
    program: &Program,
    decl: &ast::PartDecl,
    from_space: &str,
    out: &mut BTreeSet<String>,
) {
    // Bare-name resolution table from `from_space`'s point of view.
    let visible = |home: &str| {
        home == from_space
            || program
                .spaces
                .get(from_space)
                .map(|s| s.closure.contains(home))
                .unwrap_or(false)
    };
    let resolve = |segs: &[String], out: &mut BTreeSet<String>| {
        // Longest full-name prefix.
        for k in (2..=segs.len()).rev() {
            let prefix = segs[..k].join(".");
            if let Some(info) = program.parts.get(&prefix) {
                out.insert(info.home.clone());
                return;
            }
            if let Some(info) = program.foreigns.get(&prefix) {
                out.insert(info.space.clone());
                return;
            }
        }
        // Unique visible bare name.
        let bare = &segs[0];
        for (full, info) in &program.parts {
            if full.rsplit('.').next() == Some(bare.as_str()) && visible(&info.home) {
                out.insert(info.home.clone());
                return;
            }
        }
        for (full, info) in &program.foreigns {
            if full.rsplit('.').next() == Some(bare.as_str()) && visible(&info.space) {
                out.insert(info.space.clone());
                return;
            }
        }
    };
    fn walk_shape(
        sh: &SShape,
        resolve: &dyn Fn(&[String], &mut BTreeSet<String>),
        out: &mut BTreeSet<String>,
    ) {
        match &sh.shape {
            Shape::Part(name) => resolve(name, out),
            Shape::List(i) | Shape::Map(i) | Shape::Opt(i) => walk_shape(i, resolve, out),
            Shape::Fn(params, ret) => {
                for (_, p) in params {
                    walk_shape(p, resolve, out);
                }
                walk_shape(ret, resolve, out);
            }
            _ => {}
        }
    }
    for prop in &decl.props {
        if let Some(sh) = &prop.shape {
            walk_shape(sh, &resolve, out);
        }
        if let Some(v) = &prop.value {
            let mut stack = vec![v];
            while let Some(e) = stack.pop() {
                if let Expr::NameRef(segs) = &e.expr {
                    resolve(segs, out);
                }
                if let Expr::FnLit(params, _) = &e.expr {
                    for p in params {
                        walk_shape(&p.shape, &resolve, out);
                    }
                }
                push_children(e, &mut stack);
            }
        }
    }
}

/// Insert `use` lines for `spaces` (sorted, one Change) into `space`'s
/// first file, after its last `use` line — or after the space header when
/// none exist.
fn add_use_lines(
    program: &Program,
    space: &str,
    spaces: &BTreeSet<String>,
    out: &mut Vec<Change>,
) -> Result<(), Refusal> {
    let file = program
        .spaces
        .get(space)
        .and_then(|s| s.files.first())
        .cloned()
        .ok_or_else(|| Refusal(format!("`{}` has no files.", space)))?;
    let entry = program
        .files
        .iter()
        .find(|f| f.path == file)
        .ok_or_else(|| Refusal(format!("internal: `{}` not indexed.", file)))?;
    let after_line = entry
        .ast
        .uses
        .iter()
        .map(|(_, sp)| sp.start.line)
        .max()
        .unwrap_or(entry.ast.space_span.start.line);
    let pos = Pos {
        line: after_line + 1,
        col: 1,
    };
    let text: String = spaces.iter().map(|s| format!("use {}\n", s)).collect();
    out.push(Change {
        file,
        span: Span { start: pos, end: pos },
        old: String::new(),
        new: text,
    });
    Ok(())
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
