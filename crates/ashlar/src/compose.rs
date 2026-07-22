//! Composer: flattens a part's layers into `ComposedPart`s per reference §4.
//!
//! For every part, layers are walked base-first (the order the resolver
//! already computed). Within that walk:
//!
//! * The base-most layer that *declares* a property (i.e. has any `Prop`
//!   with that name) fixes the property's identity: merge kind (with the
//!   `reverse` flag), storage word, and — separately — the declared shape
//!   is taken from the base-most layer that actually states one (which may
//!   differ from the layer that first declares the property at all).
//! * Every later layer that touches the property must restate the same
//!   kind (E004/E005) and, if it states a storage word, the same one
//!   (E027); omitting storage always inherits.
//! * append/deep values are folded into a build-time literal when every
//!   definition is a pure literal of one mergeable family (text/list/map);
//!   otherwise (or on scalar/mixed-family errors) they become a `Chain`.
//! * stack/pipe values are always a `Chain`; execution order and `reverse`
//!   are a run-time concern, not this stage's.
//!
//! Determinism: `Program::parts` and every per-property map here are
//! `BTreeMap`s, and layers/definitions are walked in the order the
//! resolver/parser already fixed (space use-order, then source order) —
//! never resorted or hashed.

use crate::ast::{
    Expr, FnBody, KindDecl, ListItem, MapItem, MergeKind, Prop, SExpr, SShape, Stmt, Storage,
};
use crate::diag::{
    Diag, Edit, Level, E004_KIND_CHANGED, E005_KIND_OMITTED, E013_DUP_PROP, E019_STACK_PIPE_ARITY,
    E026_EVERY_NO_RUN, E027_STORAGE_CHANGED, E028_UNMERGEABLE,
};
use crate::resolved::{ComposedPart, ComposedProp, MergedValue, PartInfo, Program, PropRef};
use crate::tokens::Span;
use std::collections::BTreeMap;

/// Flatten every part's layers per reference §4: enforce kind identity
/// (E004/E005/E013/E019), compute literal merges for append/deep, and build
/// ordered chains for everything else.
pub fn compose(program: &Program) -> (BTreeMap<String, ComposedPart>, Vec<Diag>) {
    let mut composed: BTreeMap<String, ComposedPart> = BTreeMap::new();
    let mut diags: Vec<Diag> = Vec::new();
    for (full_name, part_info) in &program.parts {
        let (cp, mut d) = compose_part(program, full_name, part_info);
        diags.append(&mut d);
        composed.insert(full_name.clone(), cp);
    }
    (composed, diags)
}

// ---------------------------------------------------------------------------
// One textual property declaration, resolved back to its exact source slot.
// ---------------------------------------------------------------------------

struct Occ<'a> {
    /// Position of this occurrence's layer within `PartInfo::layers`, used
    /// only to collapse same-layer duplicates for the identity checks.
    layer_pos: usize,
    space: String,
    file_idx: usize,
    part_idx: usize,
    prop_idx: usize,
    file: String,
    prop: &'a Prop,
}

fn to_propref(o: &Occ) -> PropRef {
    PropRef {
        space: o.space.clone(),
        file_idx: o.file_idx,
        part_idx: o.part_idx,
        prop_idx: o.prop_idx,
    }
}

fn get_prop<'a>(program: &'a Program, pr: &PropRef) -> &'a Prop {
    &program.files[pr.file_idx].ast.parts[pr.part_idx].props[pr.prop_idx]
}

// ---------------------------------------------------------------------------
// Per-part composition.
// ---------------------------------------------------------------------------

fn compose_part(program: &Program, full_name: &str, part_info: &PartInfo) -> (ComposedPart, Vec<Diag>) {
    let mut diags: Vec<Diag> = Vec::new();
    let mut by_name: BTreeMap<String, Vec<Occ>> = BTreeMap::new();

    for (layer_pos, layer) in part_info.layers.iter().enumerate() {
        let decl = program.part_decl(layer);
        let file = program.file_path(layer).to_string();

        // E013 (duplicate property name within this one layer's block).
        let mut seen_names: BTreeMap<String, Span> = BTreeMap::new();
        for (prop_idx, propd) in decl.props.iter().enumerate() {
            if let Some(first_span) = seen_names.get(&propd.name) {
                let cause = format!("`{}` is declared twice in this layer.", propd.name);
                let note = format!(
                    "Merge the two declarations of `{}` (also declared at line {}, col {}).",
                    propd.name, first_span.start.line, first_span.start.col
                );
                diags.push(
                    Diag::new(E013_DUP_PROP, Level::Error, &file, propd.name_span, cause)
                        .with_fix(note, vec![]),
                );
            } else {
                seen_names.insert(propd.name.clone(), propd.name_span);
            }

            // E013 (duplicate key inside one map literal), walked recursively.
            if let Some(v) = &propd.value {
                walk_expr_dup_keys(v, &file, &mut diags);
            }

            by_name.entry(propd.name.clone()).or_default().push(Occ {
                layer_pos,
                space: layer.space.clone(),
                file_idx: layer.file_idx,
                part_idx: layer.part_idx,
                prop_idx,
                file: file.clone(),
                prop: propd,
            });
        }
    }

    let mut props: BTreeMap<String, ComposedProp> = BTreeMap::new();
    for (prop_name, occs) in &by_name {
        let (cp, mut pdiags) = compose_property(prop_name, occs);
        diags.append(&mut pdiags);
        props.insert(prop_name.clone(), cp);
    }

    // E026: `every` with no `run`, checked on the flattened result.
    if let Some(every_prop) = props.get("every") {
        if !props.contains_key("run") {
            let base_ref = &every_prop.defs[0];
            let base_prop = get_prop(program, base_ref);
            let file = &program.files[base_ref.file_idx].path;
            let cause = format!("`{}` has an `every` property but no `run` property.", full_name);
            diags.push(
                Diag::new(E026_EVERY_NO_RUN, Level::Error, file, base_prop.name_span, cause)
                    .with_fix("Add `run = () => { ... }`.".to_string(), vec![]),
            );
        }
    }

    (ComposedPart { props }, diags)
}

// ---------------------------------------------------------------------------
// Per-property composition: identity checks + value flattening.
// ---------------------------------------------------------------------------

fn compose_property(prop_name: &str, occs: &[Occ]) -> (ComposedProp, Vec<Diag>) {
    let mut diags: Vec<Diag> = Vec::new();

    // Collapse to one entry per layer (last declaration in that layer wins,
    // matching general replace semantics); the earlier duplicate was already
    // flagged E013 above. Used only for the identity/kind/storage checks,
    // which the reference frames in terms of *layers*, not individual defs.
    let mut per_layer: Vec<&Occ> = Vec::new();
    for o in occs {
        if let Some(last) = per_layer.last_mut() {
            if last.layer_pos == o.layer_pos {
                *last = o;
                continue;
            }
        }
        per_layer.push(o);
    }

    let base = per_layer[0];
    let identity_kind: Option<(MergeKind, bool)> =
        base.prop.kind.as_ref().map(|k: &KindDecl| (k.kind, k.reverse));
    let identity_storage: Option<Storage> = base.prop.storage.as_ref().map(|(s, _)| *s);
    // Shape identity is independent: the base-most layer that *states* one,
    // which may not be the same layer that first declares the property.
    let identity_shape: Option<SShape> = per_layer.iter().find_map(|o| o.prop.shape.clone());

    for later in &per_layer[1..] {
        let layer_kind: Option<(MergeKind, bool)> =
            later.prop.kind.as_ref().map(|k| (k.kind, k.reverse));
        if layer_kind != identity_kind {
            if layer_kind.is_none() {
                // Identity has a kind (else layer_kind == identity_kind and
                // we would not be here) and this layer omits it: E005.
                let insert_text = format!(" {}", kind_text(&identity_kind));
                let edit = Edit {
                    file: later.file.clone(),
                    start: later.prop.name_span.end,
                    end: later.prop.name_span.end,
                    text: insert_text,
                };
                let cause = format!(
                    "`{}`'s identity kind is {} but this layer omits it.",
                    prop_name,
                    kind_label(&identity_kind)
                );
                diags.push(
                    Diag::new(E005_KIND_OMITTED, Level::Error, &later.file, later.prop.name_span, cause)
                        .with_fix(format!("Restate the declared kind after `{}`.", prop_name), vec![edit]),
                );
            } else {
                let kind_span = later.prop.kind.as_ref().unwrap().span;
                let replace_text = kind_text(&identity_kind);
                let edit = Edit {
                    file: later.file.clone(),
                    start: kind_span.start,
                    end: kind_span.end,
                    text: replace_text,
                };
                let cause = format!(
                    "`{}` states {} here but its identity is {}.",
                    prop_name,
                    kind_label(&layer_kind),
                    kind_label(&identity_kind)
                );
                diags.push(
                    Diag::new(E004_KIND_CHANGED, Level::Error, &later.file, kind_span, cause)
                        .with_fix(format!("Restate `{}`'s declared kind.", prop_name), vec![edit]),
                );
            }
        }

        let layer_storage: Option<Storage> = later.prop.storage.as_ref().map(|(s, _)| *s);
        if let Some(ls) = layer_storage {
            if Some(ls) != identity_storage {
                let storage_span = later.prop.storage.as_ref().unwrap().1;
                let replace_text = storage_text(&identity_storage);
                let edit = Edit {
                    file: later.file.clone(),
                    start: storage_span.start,
                    end: storage_span.end,
                    text: replace_text,
                };
                let cause = format!(
                    "`{}` states storage `{}` here but its identity is {}.",
                    prop_name,
                    storage_token(ls),
                    storage_label(&identity_storage)
                );
                diags.push(
                    Diag::new(E027_STORAGE_CHANGED, Level::Error, &later.file, storage_span, cause)
                        .with_fix(format!("Restate `{}`'s declared storage.", prop_name), vec![edit]),
                );
            }
        }
    }

    let defs: Vec<PropRef> = occs.iter().map(to_propref).collect();
    let value_defs: Vec<&Occ> = occs.iter().filter(|o| o.prop.value.is_some()).collect();

    let value = match identity_kind.map(|(k, _)| k) {
        None => match value_defs.last() {
            Some(last) => MergedValue::Single(to_propref(last)),
            None => MergedValue::FieldOnly,
        },
        Some(MergeKind::Stack) => {
            for vd in &value_defs {
                check_fn_arity(vd, 0, "stack", &mut diags);
            }
            MergedValue::Chain(value_defs.iter().map(|o| to_propref(o)).collect())
        }
        Some(MergeKind::Pipe) => {
            for vd in &value_defs {
                check_fn_arity(vd, 1, "pipe", &mut diags);
            }
            MergedValue::Chain(value_defs.iter().map(|o| to_propref(o)).collect())
        }
        Some(k @ MergeKind::Append) | Some(k @ MergeKind::Deep) => {
            compute_append_deep(k, prop_name, &value_defs, &mut diags)
        }
    };

    let cp = ComposedProp {
        name: prop_name.to_string(),
        storage: identity_storage,
        kind: identity_kind,
        shape: identity_shape,
        defs,
        value,
    };
    (cp, diags)
}

fn check_fn_arity(vd: &Occ, want: usize, kindname: &str, diags: &mut Vec<Diag>) {
    let v = vd.prop.value.as_ref().expect("value_defs only holds defs with a value");
    match &v.expr {
        Expr::FnLit(params, _) => {
            if params.len() != want {
                let cause = format!(
                    "`{}` functions take {} parameter{}, but `{}` declares {}.",
                    kindname,
                    want,
                    if want == 1 { "" } else { "s" },
                    vd.prop.name,
                    params.len()
                );
                diags.push(
                    Diag::new(E019_STACK_PIPE_ARITY, Level::Error, &vd.file, v.span, cause).with_fix(
                        format!(
                            "Change `{}` to take exactly {} parameter{}.",
                            vd.prop.name,
                            want,
                            if want == 1 { "" } else { "s" }
                        ),
                        vec![],
                    ),
                );
            }
        }
        _ => {
            let cause = "`stack` and `pipe` properties hold functions.".to_string();
            diags.push(
                Diag::new(E019_STACK_PIPE_ARITY, Level::Error, &vd.file, v.span, cause).with_fix(
                    format!("Make `{}`'s value a function literal.", vd.prop.name),
                    vec![],
                ),
            );
        }
    }
}

// ---------------------------------------------------------------------------
// append/deep value flattening.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Family {
    Text,
    List,
    Map,
}

fn family_of(e: &Expr) -> Option<Family> {
    match e {
        Expr::Text(_) => Some(Family::Text),
        Expr::List(_) => Some(Family::List),
        Expr::MapLit(_) => Some(Family::Map),
        _ => None,
    }
}

fn is_illegal_scalar(e: &Expr) -> bool {
    matches!(e, Expr::Number(_) | Expr::Bool(_) | Expr::FnLit(..))
}

fn scalar_word(e: &Expr) -> &'static str {
    match e {
        Expr::Number(_) => "number",
        Expr::Bool(_) => "bool",
        Expr::FnLit(..) => "function",
        _ => "value",
    }
}

/// True if `e` is built entirely from literal syntax (text/number/bool/none,
/// lists and maps of literals) with no spreads, names, calls, or operators
/// anywhere inside — the precondition for computing a merge at build time.
fn is_pure_literal(e: &Expr) -> bool {
    match e {
        Expr::Text(_) | Expr::Number(_) | Expr::Bool(_) | Expr::NoneLit => true,
        Expr::List(items) => {
            items.iter().all(|it| matches!(it, ListItem::Item(se) if is_pure_literal(&se.expr)))
        }
        Expr::MapLit(items) => {
            items.iter().all(|it| matches!(it, MapItem::Entry(_, _, se) if is_pure_literal(&se.expr)))
        }
        _ => false,
    }
}

fn compute_append_deep(
    kind: MergeKind,
    prop_name: &str,
    value_defs: &[&Occ],
    diags: &mut Vec<Diag>,
) -> MergedValue {
    if value_defs.is_empty() {
        return MergedValue::FieldOnly;
    }

    let mut scalar_error = false;
    for vd in value_defs {
        let v = vd.prop.value.as_ref().unwrap();
        if is_illegal_scalar(&v.expr) {
            scalar_error = true;
            let cause = format!(
                "`{}` declares `{}` on a {} value, but `append` and `deep` apply only to text, lists, and maps.",
                prop_name,
                kind_token(kind),
                scalar_word(&v.expr)
            );
            diags.push(
                Diag::new(E028_UNMERGEABLE, Level::Error, &vd.file, v.span, cause).with_fix(
                    format!(
                        "Change `{}`'s value to text, a list, or a map, or remove the `{}` kind.",
                        prop_name,
                        kind_token(kind)
                    ),
                    vec![],
                ),
            );
        }
    }
    if scalar_error {
        return MergedValue::Chain(value_defs.iter().map(|o| to_propref(o)).collect());
    }

    let all_pure = value_defs
        .iter()
        .all(|vd| is_pure_literal(&vd.prop.value.as_ref().unwrap().expr));
    if !all_pure {
        // Some operand is a name reference, call, operator expression, or a
        // literal containing a spread: its value cannot be folded now.
        // Build-time evaluation of these merges arrives with the evaluator
        // (next increment); the chain preserves the deterministic base-first
        // order so that stage has a fixed sequence to fold at run time (C6).
        return MergedValue::Chain(value_defs.iter().map(|o| to_propref(o)).collect());
    }

    let families: Vec<Option<Family>> = value_defs
        .iter()
        .map(|vd| family_of(&vd.prop.value.as_ref().unwrap().expr))
        .collect();
    let base_family = families[0];
    let mut mismatch_idx = None;
    for (i, f) in families.iter().enumerate().skip(1) {
        if *f != base_family || f.is_none() {
            mismatch_idx = Some(i);
            break;
        }
    }
    if mismatch_idx.is_none() && base_family.is_none() {
        mismatch_idx = Some(0);
    }
    if let Some(i) = mismatch_idx {
        let vd = value_defs[i];
        let v = vd.prop.value.as_ref().unwrap();
        let cause = format!(
            "`{}`'s layered values mix incompatible shapes for `{}`: `append` and `deep` merge only text, lists, and maps of one shape.",
            prop_name,
            kind_token(kind)
        );
        diags.push(
            Diag::new(E028_UNMERGEABLE, Level::Error, &vd.file, v.span, cause).with_fix(
                "Make every layer's value the same shape: text, a list, or a map.".to_string(),
                vec![],
            ),
        );
        return MergedValue::Chain(value_defs.iter().map(|o| to_propref(o)).collect());
    }

    let mut acc: SExpr = value_defs[0].prop.value.as_ref().unwrap().clone();
    for vd in &value_defs[1..] {
        let nextv = vd.prop.value.as_ref().unwrap();
        acc = if kind == MergeKind::Deep {
            merge_deep_expr(&acc, nextv)
        } else {
            merge_append_expr(&acc, nextv)
        };
    }
    MergedValue::Literal(acc)
}

/// `append`: text/lists concatenate; maps merge one level (later keys
/// replace earlier entirely, with no recursion into their values).
fn merge_append_expr(a: &SExpr, b: &SExpr) -> SExpr {
    match (&a.expr, &b.expr) {
        (Expr::Text(ta), Expr::Text(tb)) => SExpr {
            expr: Expr::Text(format!("{}{}", ta, tb)),
            span: a.span,
        },
        (Expr::List(la), Expr::List(lb)) => {
            let mut items = la.clone();
            items.extend(lb.clone());
            SExpr {
                expr: Expr::List(items),
                span: a.span,
            }
        }
        (Expr::MapLit(ma), Expr::MapLit(mb)) => SExpr {
            expr: Expr::MapLit(merge_map_one_level(ma, mb)),
            span: a.span,
        },
        _ => b.clone(),
    }
}

fn merge_map_one_level(ma: &[MapItem], mb: &[MapItem]) -> Vec<MapItem> {
    let mut result: Vec<MapItem> = ma.to_vec();
    let mut key_index: BTreeMap<String, usize> = BTreeMap::new();
    for (i, item) in result.iter().enumerate() {
        if let MapItem::Entry(k, _, _) = item {
            key_index.insert(k.clone(), i);
        }
    }
    for item in mb {
        if let MapItem::Entry(k, ks, v) = item {
            if let Some(&idx) = key_index.get(k) {
                result[idx] = MapItem::Entry(k.clone(), *ks, v.clone());
            } else {
                key_index.insert(k.clone(), result.len());
                result.push(MapItem::Entry(k.clone(), *ks, v.clone()));
            }
        }
    }
    result
}

/// `deep`: like `append`, but maps merge at every depth — a key present on
/// both sides recurses when both values are maps, concatenates when both
/// are lists or both are text, and otherwise the later value replaces.
fn merge_deep_expr(a: &SExpr, b: &SExpr) -> SExpr {
    match (&a.expr, &b.expr) {
        (Expr::Text(ta), Expr::Text(tb)) => SExpr {
            expr: Expr::Text(format!("{}{}", ta, tb)),
            span: a.span,
        },
        (Expr::List(la), Expr::List(lb)) => {
            let mut items = la.clone();
            items.extend(lb.clone());
            SExpr {
                expr: Expr::List(items),
                span: a.span,
            }
        }
        (Expr::MapLit(ma), Expr::MapLit(mb)) => {
            let mut result: Vec<MapItem> = ma.to_vec();
            let mut key_index: BTreeMap<String, usize> = BTreeMap::new();
            for (i, item) in result.iter().enumerate() {
                if let MapItem::Entry(k, _, _) = item {
                    key_index.insert(k.clone(), i);
                }
            }
            for item in mb {
                if let MapItem::Entry(k, ks, v) = item {
                    if let Some(&idx) = key_index.get(k) {
                        let existing = match &result[idx] {
                            MapItem::Entry(_, _, ev) => ev.clone(),
                            MapItem::Spread(_) => unreachable!("pure literals never contain spreads"),
                        };
                        let merged_v = match (&existing.expr, &v.expr) {
                            (Expr::MapLit(_), Expr::MapLit(_)) => merge_deep_expr(&existing, v),
                            (Expr::List(_), Expr::List(_)) => merge_deep_expr(&existing, v),
                            (Expr::Text(_), Expr::Text(_)) => merge_deep_expr(&existing, v),
                            _ => v.clone(),
                        };
                        result[idx] = MapItem::Entry(k.clone(), *ks, merged_v);
                    } else {
                        key_index.insert(k.clone(), result.len());
                        result.push(MapItem::Entry(k.clone(), *ks, v.clone()));
                    }
                }
            }
            SExpr {
                expr: Expr::MapLit(result),
                span: a.span,
            }
        }
        _ => b.clone(),
    }
}

// ---------------------------------------------------------------------------
// Duplicate map-literal keys (E013), walked through every expression shape.
// ---------------------------------------------------------------------------

fn walk_expr_dup_keys(e: &SExpr, file: &str, diags: &mut Vec<Diag>) {
    match &e.expr {
        Expr::Text(_) | Expr::Number(_) | Expr::Bool(_) | Expr::NoneLit | Expr::NameRef(_) => {}
        Expr::List(items) => {
            for it in items {
                match it {
                    ListItem::Item(se) => walk_expr_dup_keys(se, file, diags),
                    ListItem::Spread(se) => walk_expr_dup_keys(se, file, diags),
                }
            }
        }
        Expr::MapLit(items) => {
            let mut seen: BTreeMap<String, Span> = BTreeMap::new();
            for it in items {
                match it {
                    MapItem::Entry(key, kspan, se) => {
                        if let Some(first) = seen.get(key) {
                            let cause = format!("`{}` is a duplicate key in this map literal.", key);
                            let note = format!(
                                "Remove or merge the duplicate `{}` key (also set at line {}, col {}).",
                                key, first.start.line, first.start.col
                            );
                            diags.push(
                                Diag::new(E013_DUP_PROP, Level::Error, file, *kspan, cause)
                                    .with_fix(note, vec![]),
                            );
                        } else {
                            seen.insert(key.clone(), *kspan);
                        }
                        walk_expr_dup_keys(se, file, diags);
                    }
                    MapItem::Spread(se) => walk_expr_dup_keys(se, file, diags),
                }
            }
        }
        Expr::Field(inner, _, _) => walk_expr_dup_keys(inner, file, diags),
        Expr::Index(a, b) => {
            walk_expr_dup_keys(a, file, diags);
            walk_expr_dup_keys(b, file, diags);
        }
        Expr::Call(f, args) => {
            walk_expr_dup_keys(f, file, diags);
            for a in args {
                walk_expr_dup_keys(a, file, diags);
            }
        }
        Expr::Unary(_, inner) => walk_expr_dup_keys(inner, file, diags),
        Expr::Assert(inner) => walk_expr_dup_keys(inner, file, diags),
        Expr::Binary(_, a, b) => {
            walk_expr_dup_keys(a, file, diags);
            walk_expr_dup_keys(b, file, diags);
        }
        Expr::IfExpr(cond, then_b, else_b) => {
            walk_expr_dup_keys(cond, file, diags);
            walk_stmts_dup_keys(then_b, file, diags);
            walk_stmts_dup_keys(else_b, file, diags);
        }
        Expr::FnLit(_, body) => match body.as_ref() {
            FnBody::Expr(se) => walk_expr_dup_keys(se, file, diags),
            FnBody::Block(stmts) => walk_stmts_dup_keys(stmts, file, diags),
        },
    }
}

fn walk_stmts_dup_keys(stmts: &[Stmt], file: &str, diags: &mut Vec<Diag>) {
    for s in stmts {
        match s {
            Stmt::Let(_, _, e) => walk_expr_dup_keys(e, file, diags),
            Stmt::Assign(_, _, e) => walk_expr_dup_keys(e, file, diags),
            Stmt::If(cond, thenb, elseb) => {
                walk_expr_dup_keys(cond, file, diags);
                walk_stmts_dup_keys(thenb, file, diags);
                if let Some(eb) = elseb {
                    walk_stmts_dup_keys(eb, file, diags);
                }
            }
            Stmt::For(_, iter, body) => {
                walk_expr_dup_keys(iter, file, diags);
                walk_stmts_dup_keys(body, file, diags);
            }
            Stmt::Return(opt, _) => {
                if let Some(e) = opt {
                    walk_expr_dup_keys(e, file, diags);
                }
            }
            Stmt::Expr(e) => walk_expr_dup_keys(e, file, diags),
        }
    }
}

// ---------------------------------------------------------------------------
// Token/label helpers for diagnostic text and edits.
// ---------------------------------------------------------------------------

fn kind_token(k: MergeKind) -> &'static str {
    match k {
        MergeKind::Append => "append",
        MergeKind::Deep => "deep",
        MergeKind::Stack => "stack",
        MergeKind::Pipe => "pipe",
    }
}

/// The exact source text that restates `opt` (empty string when `opt` is
/// `None`, i.e. the identity is "no kind" and the fix deletes the stated
/// kind entirely).
fn kind_text(opt: &Option<(MergeKind, bool)>) -> String {
    match opt {
        Some((k, true)) => format!("{} reverse", kind_token(*k)),
        Some((k, false)) => kind_token(*k).to_string(),
        None => String::new(),
    }
}

fn kind_label(opt: &Option<(MergeKind, bool)>) -> String {
    match opt {
        Some(_) => format!("`{}`", kind_text(opt)),
        None => "no kind".to_string(),
    }
}

fn storage_token(s: Storage) -> &'static str {
    match s {
        Storage::State => "state",
        Storage::Stored => "stored",
        Storage::Synced => "synced",
    }
}

fn storage_text(opt: &Option<Storage>) -> String {
    match opt {
        Some(s) => storage_token(*s).to_string(),
        None => String::new(),
    }
}

fn storage_label(opt: &Option<Storage>) -> String {
    match opt {
        Some(s) => format!("`{}`", storage_token(*s)),
        None => "no storage".to_string(),
    }
}

// ---------------------------------------------------------------------------
// Tests. No parser/resolver calls: `resolved::Program` values are fabricated
// by hand via the `program(...)` helper below, per the task instructions.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{self, Param, Shape};
    use crate::resolved::FileEntry;
    use crate::tokens::Pos;

    // --- span/expr/prop builders --------------------------------------

    fn sp() -> Span {
        Span {
            start: Pos { line: 1, col: 1 },
            end: Pos { line: 1, col: 2 },
        }
    }

    fn sexpr(e: Expr) -> SExpr {
        SExpr { expr: e, span: sp() }
    }

    fn text_v(s: &str) -> Expr {
        Expr::Text(s.to_string())
    }
    fn num_v(n: f64) -> Expr {
        Expr::Number(n)
    }
    fn bool_v(b: bool) -> Expr {
        Expr::Bool(b)
    }
    fn list_v(items: Vec<Expr>) -> Expr {
        Expr::List(items.into_iter().map(|e| ListItem::Item(sexpr(e))).collect())
    }
    fn map_v(entries: Vec<(&str, Expr)>) -> Expr {
        Expr::MapLit(
            entries
                .into_iter()
                .map(|(k, v)| MapItem::Entry(k.to_string(), sp(), sexpr(v)))
                .collect(),
        )
    }
    fn name_ref_v(n: &str) -> Expr {
        Expr::NameRef(vec![n.to_string()])
    }
    fn fnlit_v(param_names: Vec<&str>) -> Expr {
        Expr::FnLit(
            param_names
                .into_iter()
                .map(|p| Param {
                    name: p.to_string(),
                    name_span: sp(),
                    shape: SShape { shape: Shape::Text, span: sp() },
                })
                .collect(),
            Box::new(FnBody::Expr(sexpr(Expr::NoneLit))),
        )
    }

    fn mk_prop(name: &str) -> Prop {
        Prop {
            name: name.to_string(),
            name_span: sp(),
            storage: None,
            kind: None,
            shape: None,
            value: None,
        }
    }

    fn kinded(name: &str, k: MergeKind, reverse: bool, value: Expr) -> Prop {
        Prop {
            kind: Some(KindDecl { kind: k, reverse, span: sp() }),
            value: Some(sexpr(value)),
            ..mk_prop(name)
        }
    }

    fn replace_prop(name: &str, value: Expr) -> Prop {
        Prop {
            value: Some(sexpr(value)),
            ..mk_prop(name)
        }
    }

    fn stored_prop(name: &str, s: Storage, value: Expr) -> Prop {
        Prop {
            storage: Some((s, sp())),
            value: Some(sexpr(value)),
            ..mk_prop(name)
        }
    }

    /// Fabricates a `resolved::Program` from `(space, full_part_name, props)`
    /// tuples, one per layer, listed base-first — the plumbing compose()
    /// needs without going through the parser or resolver.
    fn program(layers: Vec<(&str, &str, Vec<Prop>)>) -> Program {
        let mut files = Vec::new();
        let mut parts: BTreeMap<String, PartInfo> = BTreeMap::new();
        for (space, part_name, props) in layers {
            let file_idx = files.len();
            let path = format!("{}_{}.ash", space, file_idx);
            let part_decl = ast::PartDecl {
                name: vec![part_name.to_string()],
                name_span: sp(),
                props,
            };
            let src = ast::SrcFile {
                space: vec![space.to_string()],
                space_span: sp(),
                uses: vec![],
                parts: vec![part_decl],
                foreigns: vec![],
            };
            files.push(FileEntry { path, ast: src });
            let layer = crate::resolved::Layer {
                space: space.to_string(),
                file_idx,
                part_idx: 0,
            };
            parts
                .entry(part_name.to_string())
                .or_insert_with(|| PartInfo { home: space.to_string(), layers: vec![] })
                .layers
                .push(layer);
        }
        Program {
            files,
            spaces: BTreeMap::new(),
            parts,
            foreigns: BTreeMap::new(),
            order: vec![],
        }
    }

    fn ids(diags: &[Diag]) -> Vec<&str> {
        diags.iter().map(|d| d.id).collect()
    }

    fn only<'a>(diags: &'a [Diag], id: &str) -> Vec<&'a Diag> {
        diags.iter().filter(|d| d.id == id).collect()
    }

    // -------------------------------------------------------------------
    // Replace (no kind), across the value families, 2 and 3 layers.
    // -------------------------------------------------------------------

    #[test]
    fn replace_text_two_layers_last_wins() {
        let prog = program(vec![
            ("a", "p.X", vec![replace_prop("greeting", text_v("hi"))]),
            ("b", "p.X", vec![replace_prop("greeting", text_v("yo"))]),
        ]);
        let (composed, diags) = compose(&prog);
        assert!(diags.is_empty(), "unexpected diags: {:?}", diags);
        let prop = &composed["p.X"].props["greeting"];
        match &prop.value {
            MergedValue::Single(pr) => assert_eq!(pr.space, "b"),
            other => panic!("expected Single, got {:?}", other),
        }
        assert_eq!(prop.defs.len(), 2);
    }

    #[test]
    fn replace_map_wins_entirely_no_merge() {
        // "replace-wins-entirely": a later replace layer's map REPLACES,
        // no merging happens, even though both are maps.
        let prog = program(vec![
            ("a", "p.X", vec![replace_prop("limits", map_v(vec![("max", num_v(10.0))]))]),
            (
                "b",
                "p.X",
                vec![replace_prop("limits", map_v(vec![("timeout", num_v(5.0))]))],
            ),
        ]);
        let (composed, diags) = compose(&prog);
        assert!(diags.is_empty());
        let prop = &composed["p.X"].props["limits"];
        match &prop.value {
            MergedValue::Single(pr) => assert_eq!(pr.space, "b"),
            other => panic!("expected Single, got {:?}", other),
        }
        // The composed value is the winning def only; nothing was merged in.
    }

    #[test]
    fn replace_list_three_layers_last_wins() {
        let prog = program(vec![
            ("a", "p.X", vec![replace_prop("tags", list_v(vec![text_v("a")]))]),
            ("b", "p.X", vec![replace_prop("tags", list_v(vec![text_v("b")]))]),
            ("c", "p.X", vec![replace_prop("tags", list_v(vec![text_v("c")]))]),
        ]);
        let (composed, diags) = compose(&prog);
        assert!(diags.is_empty());
        match &composed["p.X"].props["tags"].value {
            MergedValue::Single(pr) => assert_eq!(pr.space, "c"),
            other => panic!("expected Single, got {:?}", other),
        }
    }

    #[test]
    fn replace_function_is_single_not_chain() {
        // Replace applies uniformly, including to function-valued properties
        // (no kind stated at all -- distinct from stack/pipe).
        let prog = program(vec![
            ("a", "p.X", vec![replace_prop("greet", fnlit_v(vec!["n"]))]),
            ("b", "p.X", vec![replace_prop("greet", fnlit_v(vec!["n"]))]),
        ]);
        let (composed, diags) = compose(&prog);
        assert!(diags.is_empty());
        match &composed["p.X"].props["greet"].value {
            MergedValue::Single(pr) => assert_eq!(pr.space, "b"),
            other => panic!("expected Single, got {:?}", other),
        }
    }

    #[test]
    fn field_only_when_no_def_has_a_value() {
        let mut f = mk_prop("id");
        f.shape = Some(SShape { shape: Shape::Text, span: sp() });
        let prog = program(vec![("a", "p.X", vec![f])]);
        let (composed, diags) = compose(&prog);
        assert!(diags.is_empty());
        assert!(matches!(composed["p.X"].props["id"].value, MergedValue::FieldOnly));
    }

    // -------------------------------------------------------------------
    // append / deep: kind restated correctly across value families.
    // -------------------------------------------------------------------

    #[test]
    fn append_text_two_layers_literal_concat() {
        let prog = program(vec![
            ("a", "p.X", vec![kinded("greeting", MergeKind::Append, false, text_v("hel"))]),
            ("b", "p.X", vec![kinded("greeting", MergeKind::Append, false, text_v("lo"))]),
        ]);
        let (composed, diags) = compose(&prog);
        assert!(diags.is_empty(), "unexpected diags: {:?}", diags);
        match &composed["p.X"].props["greeting"].value {
            MergedValue::Literal(se) => assert_eq!(se.expr, Expr::Text("hello".to_string())),
            other => panic!("expected Literal, got {:?}", other),
        }
    }

    #[test]
    fn append_list_three_layers_literal_concat() {
        let prog = program(vec![
            (
                "a",
                "p.X",
                vec![kinded("tags", MergeKind::Append, false, list_v(vec![text_v("a")]))],
            ),
            (
                "b",
                "p.X",
                vec![kinded("tags", MergeKind::Append, false, list_v(vec![text_v("b")]))],
            ),
            (
                "c",
                "p.X",
                vec![kinded("tags", MergeKind::Append, false, list_v(vec![text_v("c")]))],
            ),
        ]);
        let (composed, diags) = compose(&prog);
        assert!(diags.is_empty());
        match &composed["p.X"].props["tags"].value {
            MergedValue::Literal(se) => match &se.expr {
                Expr::List(items) => {
                    let texts: Vec<String> = items
                        .iter()
                        .map(|it| match it {
                            ListItem::Item(e) => match &e.expr {
                                Expr::Text(t) => t.clone(),
                                _ => panic!("expected text item"),
                            },
                            _ => panic!("expected item, not spread"),
                        })
                        .collect();
                    assert_eq!(texts, vec!["a", "b", "c"]);
                }
                other => panic!("expected List, got {:?}", other),
            },
            other => panic!("expected Literal, got {:?}", other),
        }
    }

    #[test]
    fn append_map_one_level_two_layers() {
        let prog = program(vec![
            (
                "a",
                "p.X",
                vec![kinded(
                    "limits",
                    MergeKind::Append,
                    false,
                    map_v(vec![("http", map_v(vec![("max", num_v(10.0))]))]),
                )],
            ),
            (
                "b",
                "p.X",
                vec![kinded(
                    "limits",
                    MergeKind::Append,
                    false,
                    map_v(vec![("http", map_v(vec![("timeout", num_v(5.0))])), ("db", map_v(vec![("max", num_v(2.0))]))]),
                )],
            ),
        ]);
        let (composed, diags) = compose(&prog);
        assert!(diags.is_empty());
        match &composed["p.X"].props["limits"].value {
            MergedValue::Literal(se) => {
                // append merges ONE level: the later "http" value replaces
                // the earlier entirely (no recursive merge), so "max" is gone.
                let http = map_get(&se.expr, "http").expect("http key");
                assert!(map_get(http, "max").is_none(), "append must not merge nested maps");
                assert!(map_get(http, "timeout").is_some());
                assert!(map_get(&se.expr, "db").is_some());
            }
            other => panic!("expected Literal, got {:?}", other),
        }
    }

    #[test]
    fn deep_map_recurses_nested_keys_reference_example() {
        // The reference's `limits deep` example shape (§4), extended with a
        // second layer to exercise the recursive merge.
        let prog = program(vec![
            (
                "a",
                "p.X",
                vec![kinded(
                    "limits",
                    MergeKind::Deep,
                    false,
                    map_v(vec![("http", map_v(vec![("max", num_v(10.0))]))]),
                )],
            ),
            (
                "b",
                "p.X",
                vec![kinded(
                    "limits",
                    MergeKind::Deep,
                    false,
                    map_v(vec![("http", map_v(vec![("timeout", num_v(5.0))])), ("db", map_v(vec![("max", num_v(2.0))]))]),
                )],
            ),
        ]);
        let (composed, diags) = compose(&prog);
        assert!(diags.is_empty());
        match &composed["p.X"].props["limits"].value {
            MergedValue::Literal(se) => {
                // deep merges at EVERY depth: "http" keeps both "max" and
                // "timeout" because both sides' "http" value is a map.
                let http = map_get(&se.expr, "http").expect("http key");
                assert!(map_get(http, "max").is_some(), "deep must merge nested maps");
                assert!(map_get(http, "timeout").is_some());
                assert!(map_get(&se.expr, "db").is_some());
            }
            other => panic!("expected Literal, got {:?}", other),
        }
    }

    fn map_get<'a>(e: &'a Expr, key: &str) -> Option<&'a Expr> {
        match e {
            Expr::MapLit(items) => items.iter().find_map(|it| match it {
                MapItem::Entry(k, _, v) if k == key => Some(&v.expr),
                _ => None,
            }),
            _ => None,
        }
    }

    #[test]
    fn append_with_non_literal_operand_is_chain_not_error() {
        let prog = program(vec![
            ("a", "p.X", vec![kinded("tags", MergeKind::Append, false, list_v(vec![text_v("a")]))]),
            ("b", "p.X", vec![kinded("tags", MergeKind::Append, false, name_ref_v("dynamicTags"))]),
        ]);
        let (composed, diags) = compose(&prog);
        assert!(diags.is_empty(), "a name reference is not an error, just non-literal: {:?}", diags);
        match &composed["p.X"].props["tags"].value {
            MergedValue::Chain(defs) => {
                assert_eq!(defs.len(), 2);
                assert_eq!(defs[0].space, "a");
                assert_eq!(defs[1].space, "b");
            }
            other => panic!("expected Chain, got {:?}", other),
        }
    }

    // -------------------------------------------------------------------
    // stack / pipe: always Chain, base-first order preserved across 3 layers.
    // -------------------------------------------------------------------

    #[test]
    fn stack_chain_order_three_layers() {
        let prog = program(vec![
            ("a", "p.X", vec![kinded("start", MergeKind::Stack, false, fnlit_v(vec![]))]),
            ("b", "p.X", vec![kinded("start", MergeKind::Stack, false, fnlit_v(vec![]))]),
            ("c", "p.X", vec![kinded("start", MergeKind::Stack, false, fnlit_v(vec![]))]),
        ]);
        let (composed, diags) = compose(&prog);
        assert!(diags.is_empty(), "unexpected diags: {:?}", diags);
        match &composed["p.X"].props["start"].value {
            MergedValue::Chain(defs) => {
                assert_eq!(defs.len(), 3);
                assert_eq!(defs.iter().map(|d| d.space.as_str()).collect::<Vec<_>>(), vec!["a", "b", "c"]);
            }
            other => panic!("expected Chain, got {:?}", other),
        }
        // `reverse` is not stated: identity carries reverse=false, order is
        // unaffected (applied at run time, not by reordering here).
        assert_eq!(composed["p.X"].props["start"].kind, Some((MergeKind::Stack, false)));
    }

    #[test]
    fn pipe_chain_order_three_layers_with_reverse_identity() {
        let prog = program(vec![
            ("a", "p.X", vec![kinded("handle", MergeKind::Pipe, true, fnlit_v(vec!["req"]))]),
            ("b", "p.X", vec![kinded("handle", MergeKind::Pipe, true, fnlit_v(vec!["req"]))]),
            ("c", "p.X", vec![kinded("handle", MergeKind::Pipe, true, fnlit_v(vec!["req"]))]),
        ]);
        let (composed, diags) = compose(&prog);
        assert!(diags.is_empty(), "unexpected diags: {:?}", diags);
        match &composed["p.X"].props["handle"].value {
            MergedValue::Chain(defs) => {
                // Chain stays base-first regardless of `reverse`; reversing
                // happens at run time.
                assert_eq!(defs.iter().map(|d| d.space.as_str()).collect::<Vec<_>>(), vec!["a", "b", "c"]);
            }
            other => panic!("expected Chain, got {:?}", other),
        }
        assert_eq!(composed["p.X"].props["handle"].kind, Some((MergeKind::Pipe, true)));
    }

    // -------------------------------------------------------------------
    // E004: kind changed, exact fix edit text.
    // -------------------------------------------------------------------

    #[test]
    fn e004_kind_changed_exact_edit() {
        let kind_span = Span {
            start: Pos { line: 5, col: 10 },
            end: Pos { line: 5, col: 14 },
        };
        let base = kinded("tags", MergeKind::Append, false, list_v(vec![text_v("a")]));
        let mut later = kinded("tags", MergeKind::Deep, false, list_v(vec![text_v("b")]));
        later.kind = Some(KindDecl { kind: MergeKind::Deep, reverse: false, span: kind_span });

        let prog = program(vec![("a", "p.X", vec![base]), ("b", "p.X", vec![later])]);
        let (_composed, diags) = compose(&prog);
        let e004 = only(&diags, "E004");
        assert_eq!(e004.len(), 1, "diags: {:?}", ids(&diags));
        let fix = e004[0].fix.as_ref().expect("E004 must carry a fix");
        assert_eq!(fix.edits.len(), 1);
        let edit = &fix.edits[0];
        assert_eq!(edit.start, kind_span.start);
        assert_eq!(edit.end, kind_span.end);
        assert_eq!(edit.text, "append");
    }

    #[test]
    fn e004_kind_changed_reverse_mismatch_exact_edit() {
        let kind_span = Span {
            start: Pos { line: 2, col: 3 },
            end: Pos { line: 2, col: 16 },
        };
        let base = kinded("stop", MergeKind::Stack, true, fnlit_v(vec![]));
        let mut later = kinded("stop", MergeKind::Stack, false, fnlit_v(vec![]));
        later.kind = Some(KindDecl { kind: MergeKind::Stack, reverse: false, span: kind_span });

        let prog = program(vec![("a", "p.X", vec![base]), ("b", "p.X", vec![later])]);
        let (_composed, diags) = compose(&prog);
        let e004 = only(&diags, "E004");
        assert_eq!(e004.len(), 1, "diags: {:?}", ids(&diags));
        let edit = &e004[0].fix.as_ref().unwrap().edits[0];
        assert_eq!(edit.text, "stack reverse");
    }

    #[test]
    fn e004_kind_stated_when_identity_has_none() {
        // identity is replace (no kind); a later layer states `append`.
        let kind_span = Span {
            start: Pos { line: 8, col: 4 },
            end: Pos { line: 8, col: 10 },
        };
        let base = replace_prop("greeting", text_v("hi"));
        let mut later = kinded("greeting", MergeKind::Append, false, text_v("yo"));
        later.kind = Some(KindDecl { kind: MergeKind::Append, reverse: false, span: kind_span });

        let prog = program(vec![("a", "p.X", vec![base]), ("b", "p.X", vec![later])]);
        let (_composed, diags) = compose(&prog);
        let e004 = only(&diags, "E004");
        assert_eq!(e004.len(), 1, "diags: {:?}", ids(&diags));
        let edit = &e004[0].fix.as_ref().unwrap().edits[0];
        // Identity has no kind: the fix deletes the stated kind entirely.
        assert_eq!(edit.text, "");
        assert_eq!(edit.start, kind_span.start);
        assert_eq!(edit.end, kind_span.end);
    }

    // -------------------------------------------------------------------
    // E005: kind omitted, exact insertion.
    // -------------------------------------------------------------------

    #[test]
    fn e005_kind_omitted_exact_insertion() {
        let name_span = Span {
            start: Pos { line: 3, col: 3 },
            end: Pos { line: 3, col: 7 },
        };
        let base = kinded("tags", MergeKind::Append, false, list_v(vec![text_v("a")]));
        let mut later = replace_prop("tags", list_v(vec![text_v("b")]));
        later.name_span = name_span;

        let prog = program(vec![("a", "p.X", vec![base]), ("b", "p.X", vec![later])]);
        let (_composed, diags) = compose(&prog);
        let e005 = only(&diags, "E005");
        assert_eq!(e005.len(), 1, "diags: {:?}", ids(&diags));
        let edit = &e005[0].fix.as_ref().unwrap().edits[0];
        assert_eq!(edit.start, name_span.end);
        assert_eq!(edit.end, name_span.end);
        assert_eq!(edit.text, " append");
    }

    #[test]
    fn e005_kind_omitted_with_reverse_exact_insertion() {
        let name_span = Span {
            start: Pos { line: 4, col: 3 },
            end: Pos { line: 4, col: 8 },
        };
        let base = kinded("stop", MergeKind::Stack, true, fnlit_v(vec![]));
        let mut later = replace_prop("stop", fnlit_v(vec![]));
        later.name_span = name_span;

        let prog = program(vec![("a", "p.X", vec![base]), ("b", "p.X", vec![later])]);
        let (_composed, diags) = compose(&prog);
        let e005 = only(&diags, "E005");
        assert_eq!(e005.len(), 1);
        let edit = &e005[0].fix.as_ref().unwrap().edits[0];
        assert_eq!(edit.text, " stack reverse");
    }

    #[test]
    fn no_kind_no_diag_when_both_layers_replace() {
        let prog = program(vec![
            ("a", "p.X", vec![replace_prop("greeting", text_v("hi"))]),
            ("b", "p.X", vec![replace_prop("greeting", text_v("yo"))]),
        ]);
        let (_composed, diags) = compose(&prog);
        assert!(diags.is_empty(), "replace needs no restatement: {:?}", diags);
    }

    // -------------------------------------------------------------------
    // E013: both forms (duplicate property in one layer; duplicate map key).
    // -------------------------------------------------------------------

    #[test]
    fn e013_duplicate_property_in_one_layer() {
        let prog = program(vec![(
            "a",
            "p.X",
            vec![replace_prop("greeting", text_v("hi")), replace_prop("greeting", text_v("bye"))],
        )]);
        let (_composed, diags) = compose(&prog);
        let e013 = only(&diags, "E013");
        assert_eq!(e013.len(), 1, "diags: {:?}", ids(&diags));
        assert!(e013[0].cause.contains("`greeting`"));
        assert!(e013[0].cause.contains("twice"));
    }

    #[test]
    fn e013_duplicate_map_literal_key() {
        let prog = program(vec![(
            "a",
            "p.X",
            vec![replace_prop("limits", map_v(vec![("max", num_v(1.0)), ("max", num_v(2.0))]))],
        )]);
        let (_composed, diags) = compose(&prog);
        let e013 = only(&diags, "E013");
        assert_eq!(e013.len(), 1, "diags: {:?}", ids(&diags));
        assert!(e013[0].cause.contains("`max`"));
        assert!(e013[0].cause.contains("duplicate key"));
    }

    #[test]
    fn e013_duplicate_map_literal_key_nested_inside_function_body() {
        // Walk into a stack/pipe function's body to find nested map literals.
        let body = Expr::FnLit(
            vec![],
            Box::new(FnBody::Block(vec![Stmt::Return(
                Some(sexpr(map_v(vec![("ready", bool_v(true)), ("ready", bool_v(false))]))),
                sp(),
            )])),
        );
        let prog = program(vec![("a", "p.X", vec![kinded("start", MergeKind::Stack, false, body)])]);
        let (_composed, diags) = compose(&prog);
        let e013 = only(&diags, "E013");
        assert_eq!(e013.len(), 1, "diags: {:?}", ids(&diags));
        assert!(e013[0].cause.contains("`ready`"));
    }

    // -------------------------------------------------------------------
    // E019: all three arities (stack w/ params, pipe w/ 0, pipe w/ 2), plus
    // the "not a function literal at all" case.
    // -------------------------------------------------------------------

    #[test]
    fn e019_stack_with_parameters() {
        let prog = program(vec![("a", "p.X", vec![kinded("start", MergeKind::Stack, false, fnlit_v(vec!["x"]))])]);
        let (_composed, diags) = compose(&prog);
        let e019 = only(&diags, "E019");
        assert_eq!(e019.len(), 1, "diags: {:?}", ids(&diags));
        assert!(e019[0].cause.contains("stack"));
    }

    #[test]
    fn e019_pipe_with_zero_parameters() {
        let prog = program(vec![("a", "p.X", vec![kinded("handle", MergeKind::Pipe, false, fnlit_v(vec![]))])]);
        let (_composed, diags) = compose(&prog);
        let e019 = only(&diags, "E019");
        assert_eq!(e019.len(), 1, "diags: {:?}", ids(&diags));
    }

    #[test]
    fn e019_pipe_with_two_parameters() {
        let prog = program(vec![(
            "a",
            "p.X",
            vec![kinded("handle", MergeKind::Pipe, false, fnlit_v(vec!["req", "extra"]))],
        )]);
        let (_composed, diags) = compose(&prog);
        let e019 = only(&diags, "E019");
        assert_eq!(e019.len(), 1, "diags: {:?}", ids(&diags));
    }

    #[test]
    fn e019_value_not_a_function_literal_at_all() {
        let prog = program(vec![("a", "p.X", vec![kinded("start", MergeKind::Stack, false, text_v("nope"))])]);
        let (_composed, diags) = compose(&prog);
        let e019 = only(&diags, "E019");
        assert_eq!(e019.len(), 1, "diags: {:?}", ids(&diags));
        assert_eq!(e019[0].cause, "`stack` and `pipe` properties hold functions.");
    }

    #[test]
    fn stack_no_params_and_pipe_one_param_are_ok() {
        let prog = program(vec![
            ("a", "p.X", vec![kinded("start", MergeKind::Stack, false, fnlit_v(vec![]))]),
            ("a", "p.Y", vec![kinded("handle", MergeKind::Pipe, false, fnlit_v(vec!["req"]))]),
        ]);
        let (_composed, diags) = compose(&prog);
        assert!(diags.is_empty(), "diags: {:?}", diags);
    }

    // -------------------------------------------------------------------
    // E026: `every` with no `run`.
    // -------------------------------------------------------------------

    #[test]
    fn e026_every_without_run() {
        let prog = program(vec![("a", "p.X", vec![replace_prop("every", text_v("10m"))])]);
        let (_composed, diags) = compose(&prog);
        let e026 = only(&diags, "E026");
        assert_eq!(e026.len(), 1, "diags: {:?}", ids(&diags));
        assert!(e026[0].cause.contains("`every`"));
        assert!(e026[0].cause.contains("`run`"));
    }

    #[test]
    fn every_with_run_is_ok() {
        let prog = program(vec![(
            "a",
            "p.X",
            vec![replace_prop("every", text_v("10m")), replace_prop("run", fnlit_v(vec![]))],
        )]);
        let (_composed, diags) = compose(&prog);
        assert!(only(&diags, "E026").is_empty(), "diags: {:?}", diags);
    }

    // -------------------------------------------------------------------
    // E027: storage changed; omission always inherits.
    // -------------------------------------------------------------------

    #[test]
    fn e027_storage_changed() {
        let storage_span = Span {
            start: Pos { line: 6, col: 1 },
            end: Pos { line: 6, col: 7 },
        };
        let base = stored_prop("draft", Storage::State, text_v(""));
        let mut later = stored_prop("draft", Storage::Stored, text_v("x"));
        later.storage = Some((Storage::Stored, storage_span));

        let prog = program(vec![("a", "p.X", vec![base]), ("b", "p.X", vec![later])]);
        let (_composed, diags) = compose(&prog);
        let e027 = only(&diags, "E027");
        assert_eq!(e027.len(), 1, "diags: {:?}", ids(&diags));
        let edit = &e027[0].fix.as_ref().unwrap().edits[0];
        assert_eq!(edit.start, storage_span.start);
        assert_eq!(edit.end, storage_span.end);
        assert_eq!(edit.text, "state");
    }

    #[test]
    fn e027_omitting_storage_on_later_layer_inherits_no_error() {
        let base = stored_prop("draft", Storage::State, text_v(""));
        let later = replace_prop("draft", text_v("x")); // no storage stated
        let prog = program(vec![("a", "p.X", vec![base]), ("b", "p.X", vec![later])]);
        let (composed, diags) = compose(&prog);
        assert!(only(&diags, "E027").is_empty(), "diags: {:?}", diags);
        assert_eq!(composed["p.X"].props["draft"].storage, Some(Storage::State));
    }

    // -------------------------------------------------------------------
    // E028: unmergeable scalar values, and mixed literal families.
    // -------------------------------------------------------------------

    #[test]
    fn e028_append_on_number() {
        let prog = program(vec![("a", "p.X", vec![kinded("count", MergeKind::Append, false, num_v(5.0))])]);
        let (_composed, diags) = compose(&prog);
        let e028 = only(&diags, "E028");
        assert_eq!(e028.len(), 1, "diags: {:?}", ids(&diags));
        assert!(e028[0].cause.contains("text, lists, and maps"));
    }

    #[test]
    fn e028_deep_on_bool() {
        let prog = program(vec![("a", "p.X", vec![kinded("flag", MergeKind::Deep, false, bool_v(true))])]);
        let (_composed, diags) = compose(&prog);
        let e028 = only(&diags, "E028");
        assert_eq!(e028.len(), 1, "diags: {:?}", ids(&diags));
    }

    #[test]
    fn e028_append_on_function_literal() {
        let prog = program(vec![(
            "a",
            "p.X",
            vec![kinded("greet", MergeKind::Append, false, fnlit_v(vec!["n"]))],
        )]);
        let (_composed, diags) = compose(&prog);
        let e028 = only(&diags, "E028");
        assert_eq!(e028.len(), 1, "diags: {:?}", ids(&diags));
    }

    #[test]
    fn e028_mixed_family_text_vs_list() {
        let prog = program(vec![
            ("a", "p.X", vec![kinded("tags", MergeKind::Append, false, text_v("core"))]),
            ("b", "p.X", vec![kinded("tags", MergeKind::Append, false, list_v(vec![text_v("x")]))]),
        ]);
        let (_composed, diags) = compose(&prog);
        let e028 = only(&diags, "E028");
        assert_eq!(e028.len(), 1, "diags: {:?}", ids(&diags));
        assert!(e028[0].cause.contains("text, lists, and maps"));
    }

    #[test]
    fn e028_mixed_family_list_vs_map() {
        let prog = program(vec![
            ("a", "p.X", vec![kinded("tags", MergeKind::Deep, false, list_v(vec![text_v("x")]))]),
            ("b", "p.X", vec![kinded("tags", MergeKind::Deep, false, map_v(vec![("k", text_v("v"))]))]),
        ]);
        let (_composed, diags) = compose(&prog);
        let e028 = only(&diags, "E028");
        assert_eq!(e028.len(), 1, "diags: {:?}", ids(&diags));
    }

    // -------------------------------------------------------------------
    // Shape identity: taken from the first layer that states one, which may
    // not be the same layer that first declares the property.
    // -------------------------------------------------------------------

    #[test]
    fn shape_identity_from_first_layer_that_states_one() {
        let mut base = replace_prop("id", name_ref_v("nope")); // no shape here
        base.value = None;
        base.shape = None;
        let mut later = mk_prop("id");
        later.shape = Some(SShape { shape: Shape::Text, span: sp() });
        let prog = program(vec![("a", "p.X", vec![base]), ("b", "p.X", vec![later])]);
        let (composed, diags) = compose(&prog);
        assert!(diags.is_empty(), "diags: {:?}", diags);
        assert!(matches!(composed["p.X"].props["id"].shape.as_ref().unwrap().shape, Shape::Text));
    }

    // -------------------------------------------------------------------
    // Determinism smoke test across multiple parts/properties.
    // -------------------------------------------------------------------

    #[test]
    fn multiple_parts_and_props_all_present_deterministically() {
        let prog = program(vec![
            ("a", "p.X", vec![replace_prop("greeting", text_v("hi")), kinded("tags", MergeKind::Append, false, list_v(vec![text_v("x")]))]),
            ("b", "p.Y", vec![replace_prop("name", text_v("y"))]),
        ]);
        let (composed, diags) = compose(&prog);
        assert!(diags.is_empty());
        assert_eq!(composed.len(), 2);
        assert!(composed.contains_key("p.X"));
        assert!(composed.contains_key("p.Y"));
        assert_eq!(composed["p.X"].props.len(), 2);
        assert_eq!(composed["p.Y"].props.len(), 1);
    }
}
