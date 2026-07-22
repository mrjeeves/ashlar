//! Shape checker (reference §5–§7, requirement D3): every expression has a
//! shape known at build time, and every detectable mismatch is E006 with
//! the expected and actual shape stated and, where a safe mechanical edit
//! exists, a correction.
//!
//! Design rules:
//!
//! * **No false positives.** `Unknown` absorbs anything the checker cannot
//!   determine (unannotated recursion, permissive std calls, `data` field
//!   walks). An `Unknown` operand never produces a diagnostic. Precision
//!   grows increment by increment; wrong errors would poison trust in the
//!   corrections (D2) immediately.
//! * Optionality is structural: `S` fits `S?`, `none` fits any `S?`, and
//!   `S?` does NOT fit `S` — that mismatch carries the "handle `none`
//!   first" note (`??`, `!`, or an `if`), which is what converts the
//!   accepted F3 reading risk (ADR-0008) into a compile-time correction.
//! * `data` admits text, number, bool, none, lists of data, and maps of
//!   data (reference §5); data-shape part values also fit `data`, since
//!   their literal form is a map.
//!
//! The checker re-resolves name prefixes the same way the resolver does
//! (longest visible prefix), because the resolver deliberately leaves
//! trailing segments unvalidated for this stage to check as field access.

use crate::ast::{
    self, Expr, FnBody, ListItem, MapItem, PartDecl, SExpr, SShape, Shape, Stmt,
};
use crate::diag::{Diag, Edit, Level, E006_SHAPE, E021_ROUTE_CONFLICT};
use crate::resolved::{ComposedPart, MergedValue, Program, STD_FNS, STD_PARTS};
use crate::tokens::Span;
use std::collections::BTreeMap;

/// Check every property value in every part declaration of `program`.
pub fn check(
    program: &Program,
    composed: &BTreeMap<String, ComposedPart>,
) -> Vec<Diag> {
    let mut diags = Vec::new();
    let tables = Tables::build(program, composed);
    for idx in 0..program.files.len() {
        let space = ast::name_to_string(&program.files[idx].ast.space);
        if !program.spaces.contains_key(&space) {
            continue;
        }
        let file = program.files[idx].path.clone();
        for decl in &program.files[idx].ast.parts {
            let part_full = if decl.name.len() == 1 {
                format!("{}.{}", space, decl.name[0])
            } else {
                ast::name_to_string(&decl.name)
            };
            let mut cx = Cx {
                tables: &tables,
                space: space.clone(),
                file: file.clone(),
                part_full,
                locals: Vec::new(),
                diags: Vec::new(),
            };
            cx.check_part_decl(decl);
            diags.append(&mut cx.diags);
        }
    }
    diags.append(&mut check_routes(program, composed));
    diags
}

/// E021 (reference §9.2): two routes matching one path is a compile error
/// naming both — exact duplicates and capture overlaps alike, since a
/// `{name}` segment can match any literal.
fn check_routes(
    program: &Program,
    composed: &BTreeMap<String, ComposedPart>,
) -> Vec<Diag> {
    let mut diags = Vec::new();
    // (part full name, pattern, file, span), in BTreeMap order — sorted by
    // part name, so the "later" of a conflicting pair is deterministic.
    let mut routes: Vec<(String, String, String, Span)> = Vec::new();
    for (full, cp) in composed {
        let Some(prop) = cp.props.get("route") else { continue };
        let source = match &prop.value {
            MergedValue::Single(pr) => program.files[pr.file_idx].ast.parts[pr.part_idx].props
                [pr.prop_idx]
                .value
                .as_ref(),
            MergedValue::Literal(e) => Some(e),
            _ => None,
        };
        if let Some(e) = source {
            if let Expr::Text(pattern) = &e.expr {
                let file = match &prop.value {
                    MergedValue::Single(pr) => program.files[pr.file_idx].path.clone(),
                    _ => prop
                        .defs
                        .first()
                        .map(|pr| program.files[pr.file_idx].path.clone())
                        .unwrap_or_default(),
                };
                routes.push((full.clone(), pattern.clone(), file, e.span));
            }
        }
    }
    for i in 0..routes.len() {
        for j in (i + 1)..routes.len() {
            if patterns_overlap(&routes[i].1, &routes[j].1) {
                let (a, b) = (&routes[i], &routes[j]);
                diags.push(
                    Diag::new(
                        E021_ROUTE_CONFLICT,
                        Level::Error,
                        &b.2,
                        b.3,
                        format!(
                            "`{}` and `{}` can match the same path (`{}` and `{}`).",
                            a.0, b.0, a.1, b.1
                        ),
                        )
                    .with_fix(
                        "Change one route so no single path matches both.".to_string(),
                        vec![],
                    ),
                );
            }
        }
    }
    diags
}

/// Two route patterns overlap when they have the same segment count and
/// every segment pair can match the same text — equal literals, or either
/// side a `{capture}`.
fn patterns_overlap(a: &str, b: &str) -> bool {
    let seg = |p: &str| -> Vec<String> {
        p.trim_matches('/').split('/').map(|s| s.to_string()).collect()
    };
    let (sa, sb) = (seg(a), seg(b));
    if sa.len() != sb.len() {
        return false;
    }
    sa.iter().zip(&sb).all(|(x, y)| {
        let cap = |s: &str| s.starts_with('{') && s.ends_with('}');
        cap(x) || cap(y) || x == y
    })
}

// ---------------------------------------------------------------------------
// The inference shape model.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
enum S {
    Text,
    Number,
    Bool,
    Data,
    List(Box<S>),
    Map(Box<S>),
    /// Full part name. Data-shape parts double as record value shapes.
    Part(String),
    Opt(Box<S>),
    Fn(Vec<S>, Box<S>),
    /// The shape of the literal `none` before it joins an optional.
    NoneS,
    /// Cannot be determined; never produces a diagnostic.
    Unknown,
}

impl S {
    fn opt(self) -> S {
        match self {
            S::Opt(_) | S::Unknown | S::NoneS => self,
            s => S::Opt(Box::new(s)),
        }
    }

    fn is_unknown(&self) -> bool {
        matches!(self, S::Unknown)
    }
}

/// Surface rendering for diagnostics: `{text: number}`, `chat.data.Message?`.
fn render(s: &S) -> String {
    match s {
        S::Text => "text".into(),
        S::Number => "number".into(),
        S::Bool => "bool".into(),
        S::Data => "data".into(),
        S::List(i) => format!("[{}]", render(i)),
        S::Map(v) => format!("{{text: {}}}", render(v)),
        S::Part(p) => p.clone(),
        S::Opt(i) => format!("{}?", render(i)),
        S::Fn(ps, r) => format!(
            "({}) -> {}",
            ps.iter().map(render).collect::<Vec<_>>().join(", "),
            render(r)
        ),
        S::NoneS => "none".into(),
        S::Unknown => "?".into(),
    }
}

/// Does a value of shape `actual` fit a position expecting `expected`?
fn fits(tables: &Tables, actual: &S, expected: &S) -> bool {
    use S::*;
    match (actual, expected) {
        (Unknown, _) | (_, Unknown) => true,
        (NoneS, Opt(_)) | (NoneS, Data) | (NoneS, NoneS) => true,
        (a, Opt(e)) => fits(tables, a, e) || matches!(a, Opt(i) if fits(tables, i, e)),
        (Opt(_), _) => false, // optional never fits a plain position
        (Text, Text) | (Number, Number) | (Bool, Bool) | (Data, Data) => true,
        (List(a), List(e)) => fits(tables, a, e),
        (Map(a), Map(e)) => fits(tables, a, e),
        (Part(a), Part(e)) => a == e,
        (Fn(ap, ar), Fn(ep, er)) => {
            ap.len() == ep.len()
                && ap.iter().zip(ep).all(|(a, e)| fits(tables, e, a))
                && fits(tables, ar, er)
        }
        // data admits its constituents (reference §5), and data-shape part
        // values whose literal form is a map.
        (Text, Data) | (Number, Data) | (Bool, Data) => true,
        (List(i), Data) => fits(tables, i, &Data),
        (Map(v), Data) => fits(tables, v, &Data),
        (Part(p), Data) => tables.data_shape_fields.contains_key(p),
        _ => false,
    }
}

/// The common shape of two branches/elements, if any.
fn join(tables: &Tables, a: &S, b: &S) -> Option<S> {
    use S::*;
    if a == b {
        return Some(a.clone());
    }
    match (a, b) {
        (Unknown, _) | (_, Unknown) => Some(Unknown),
        (NoneS, s) | (s, NoneS) => Some(s.clone().opt()),
        (Opt(i), s) | (s, Opt(i)) => join(tables, i, s).map(|j| j.opt()),
        (List(x), List(y)) => join(tables, x, y).map(|j| List(Box::new(j))),
        (Map(x), Map(y)) => join(tables, x, y).map(|j| Map(Box::new(j))),
        // `data` absorbs its constituents when one side already is data.
        (Data, s) | (s, Data) if fits(tables, s, &Data) => Some(Data),
        // Strict otherwise: "one shape" means one shape (reference §6);
        // heterogeneous literals are legal only where `data` is expected,
        // which the literal call sites handle explicitly.
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Program-wide tables, built once.
// ---------------------------------------------------------------------------

struct Tables {
    /// space -> (bare part name -> unique full name; ambiguous bare names
    /// are simply absent — the resolver already reported E002).
    bare: BTreeMap<String, BTreeMap<String, String>>,
    /// space -> visible full part names.
    fulls: BTreeMap<String, Vec<String>>,
    /// full part name -> property name -> declared-or-inferred shape.
    part_props: BTreeMap<String, BTreeMap<String, S>>,
    /// data-shape parts only: full name -> field name -> (shape, has default).
    data_shape_fields: BTreeMap<String, BTreeMap<String, (S, bool)>>,
    /// foreign full name and per-space bare -> function shape.
    foreigns: BTreeMap<String, S>,
}

impl Tables {
    fn build(program: &Program, composed: &BTreeMap<String, ComposedPart>) -> Tables {
        // Home-space index first: linear in (spaces × closure), not
        // (spaces × parts) — this is on the F1 path.
        let mut parts_by_home: BTreeMap<&str, Vec<&String>> = BTreeMap::new();
        for (full, info) in &program.parts {
            parts_by_home.entry(info.home.as_str()).or_default().push(full);
        }
        let mut bare: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
        let mut fulls: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for space in program.spaces.keys() {
            let closure = &program.spaces[space].closure;
            let mut b: BTreeMap<String, Vec<String>> = BTreeMap::new();
            let mut f: Vec<String> = Vec::new();
            let visible = std::iter::once(space.as_str())
                .chain(closure.iter().map(|s| s.as_str()));
            for home in visible {
                for full in parts_by_home.get(home).into_iter().flatten() {
                    let bn = full.rsplit('.').next().unwrap_or(full).to_string();
                    b.entry(bn).or_default().push((*full).clone());
                    f.push((*full).clone());
                }
            }
            for p in STD_PARTS {
                b.entry((*p).to_string())
                    .or_default()
                    .push(format!("std.{}", p));
                f.push(format!("std.{}", p));
            }
            bare.insert(
                space.clone(),
                b.into_iter()
                    .filter(|(_, v)| v.len() == 1)
                    .map(|(k, mut v)| (k, v.remove(0)))
                    .collect(),
            );
            fulls.insert(space.clone(), f);
        }

        // Property shapes per part, from the composed view: the declared
        // shape when one exists, else inferred from a literal/function
        // value, else Unknown. Shape names resolve in the home space.
        let mut part_props: BTreeMap<String, BTreeMap<String, S>> = BTreeMap::new();
        let mut data_shape_fields: BTreeMap<String, BTreeMap<String, (S, bool)>> =
            BTreeMap::new();
        for (full, cp) in composed {
            let home = program
                .parts
                .get(full)
                .map(|i| i.home.clone())
                .unwrap_or_default();
            let mut props: BTreeMap<String, S> = BTreeMap::new();
            let mut fields: BTreeMap<String, (S, bool)> = BTreeMap::new();
            let mut all_fields = true;
            for (name, prop) in &cp.props {
                let declared = prop
                    .shape
                    .as_ref()
                    .map(|sh| resolve_shape_in(&bare, &home, sh));
                let value_ref = prop.defs.last();
                let value = value_ref.and_then(|r| {
                    program.files[r.file_idx].ast.parts[r.part_idx].props[r.prop_idx]
                        .value
                        .as_ref()
                        .map(|v| (r.space.clone(), v))
                });
                let has_value = value.is_some();
                // Function-valued properties expose their parameter shapes
                // even without a declared shape, so calls to them check.
                let inferred = match &value {
                    Some((vspace, v)) => match &v.expr {
                        Expr::FnLit(params, _) => Some(S::Fn(
                            params
                                .iter()
                                .map(|p| resolve_shape_in(&bare, vspace, &p.shape))
                                .collect(),
                            Box::new(S::Unknown),
                        )),
                        Expr::Text(_) => Some(S::Text),
                        Expr::Number(_) => Some(S::Number),
                        Expr::Bool(_) => Some(S::Bool),
                        _ => None,
                    },
                    None => None,
                };
                let s = declared.clone().or(inferred).unwrap_or(S::Unknown);
                props.insert(name.clone(), s.clone());
                if has_value || prop.storage.is_some() {
                    all_fields = false;
                } else {
                    fields.insert(name.clone(), (s, false));
                }
                // A field with a default (shape + value, no storage) still
                // counts as a field for literal checking.
                if declared.is_some() && has_value && prop.storage.is_none() {
                    fields.insert(name.clone(), (props[name].clone(), true));
                }
            }
            part_props.insert(full.clone(), props);
            if all_fields && !fields.is_empty() {
                data_shape_fields.insert(full.clone(), fields);
            } else if !fields.is_empty()
                && fields.len() == cp.props.len()
            {
                data_shape_fields.insert(full.clone(), fields);
            }
        }

        // std part properties.
        let mut std_req: BTreeMap<String, S> = BTreeMap::new();
        std_req.insert("path".into(), S::Text);
        std_req.insert("method".into(), S::Text);
        std_req.insert("params".into(), S::Map(Box::new(S::Text)));
        std_req.insert("data".into(), S::Data);
        std_req.insert("headers".into(), S::Map(Box::new(S::Text)));
        std_req.insert("user".into(), S::Opt(Box::new(S::Part("std.User".into()))));
        part_props.insert("std.Request".into(), std_req);
        let mut std_event: BTreeMap<String, S> = BTreeMap::new();
        std_event.insert("name".into(), S::Text);
        std_event.insert("data".into(), S::Data);
        part_props.insert("std.Event".into(), std_event);
        let mut std_user: BTreeMap<String, S> = BTreeMap::new();
        std_user.insert("id".into(), S::Text);
        std_user.insert("email".into(), S::Text);
        part_props.insert("std.User".into(), std_user);
        part_props.insert("std.Element".into(), BTreeMap::new());
        let mut std_log: BTreeMap<String, S> = BTreeMap::new();
        for f in ["debug", "info", "warn", "error"] {
            // Permissive: text message plus optional data payload.
            std_log.insert(f.into(), S::Unknown);
        }
        part_props.insert("std.log".into(), std_log);

        // Foreign signatures, visible by full name and per-space bare name.
        let mut foreigns: BTreeMap<String, S> = BTreeMap::new();
        for (full, info) in &program.foreigns {
            let fd = &program.files[info.file_idx].ast.foreigns[info.foreign_idx];
            let params = fd
                .params
                .iter()
                .map(|(_, sh)| resolve_shape_in(&bare, &info.space, sh))
                .collect::<Vec<_>>();
            let ret = resolve_shape_in(&bare, &info.space, &fd.ret);
            foreigns.insert(full.clone(), S::Fn(params, Box::new(ret)));
        }

        Tables {
            bare,
            fulls,
            part_props,
            data_shape_fields,
            foreigns,
        }
    }
}

/// Resolve an AST shape annotation to an inference shape, from `space`'s
/// point of view. Unresolvable part names (already E001/E002 elsewhere)
/// become Unknown rather than erroring twice.
fn resolve_shape_in(
    bare: &BTreeMap<String, BTreeMap<String, String>>,
    space: &str,
    sh: &SShape,
) -> S {
    match &sh.shape {
        Shape::Text => S::Text,
        Shape::Number => S::Number,
        Shape::Bool => S::Bool,
        Shape::Data => S::Data,
        Shape::List(i) => S::List(Box::new(resolve_shape_in(bare, space, i))),
        Shape::Map(v) => S::Map(Box::new(resolve_shape_in(bare, space, v))),
        Shape::Opt(i) => resolve_shape_in(bare, space, i).opt(),
        Shape::Fn(ps, r) => S::Fn(
            ps.iter()
                .map(|(_, p)| resolve_shape_in(bare, space, p))
                .collect(),
            Box::new(resolve_shape_in(bare, space, r)),
        ),
        Shape::Part(name) => {
            if name.len() > 1 {
                S::Part(ast::name_to_string(name))
            } else {
                match bare.get(space).and_then(|m| m.get(&name[0])) {
                    Some(full) => S::Part(full.clone()),
                    None => S::Unknown,
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Per-declaration checking context.
// ---------------------------------------------------------------------------

struct Cx<'a> {
    tables: &'a Tables,
    space: String,
    file: String,
    /// Full name of the part this declaration belongs to.
    part_full: String,
    locals: Vec<BTreeMap<String, S>>,
    diags: Vec<Diag>,
}

impl<'a> Cx<'a> {
    fn err(&mut self, span: Span, cause: String, note: String, edits: Vec<Edit>) {
        self.diags.push(
            Diag::new(E006_SHAPE, Level::Error, &self.file, span, cause).with_fix(note, edits),
        );
    }

    fn mismatch(&mut self, span: Span, expected: &S, actual: &S) {
        self.err(
            span,
            format!(
                "expected `{}`, found `{}`.",
                render(expected),
                render(actual)
            ),
            if matches!(actual, S::Opt(i) if fits(self.tables, i, expected)) {
                "Handle `none` first: use `??`, `!`, or `if x != none`.".to_string()
            } else {
                format!("Produce a `{}` here.", render(expected))
            },
            vec![],
        );
    }

    fn check_part_decl(&mut self, decl: &PartDecl) {
        for prop in &decl.props {
            let declared = prop
                .shape
                .as_ref()
                .map(|sh| resolve_shape_in(&self.tables.bare, &self.space, sh));
            if let Some(value) = &prop.value {
                // stack/pipe layer functions have arity rules the composer
                // owns (E019); check their bodies without an expectation.
                let is_chain = matches!(
                    prop.kind.as_ref().map(|k| k.kind),
                    Some(ast::MergeKind::Stack) | Some(ast::MergeKind::Pipe)
                );
                match (&declared, is_chain) {
                    (Some(exp), false) => {
                        let exp = exp.clone();
                        self.check_against(value, &exp);
                    }
                    _ => {
                        self.infer(value);
                    }
                }
                // `every` durations are validated at build time (§9.7).
                if prop.name == "every" {
                    if let Expr::Text(t) = &value.expr {
                        if !valid_duration(t) {
                            self.err(
                                value.span,
                                format!("`\"{}\"` is not a duration.", t),
                                "Write digits then a unit: `ms`, `s`, `m`, `h`, or `d` — e.g. `\"10m\"`.".to_string(),
                                vec![],
                            );
                        }
                    }
                }
            }
        }
    }

    // -- bidirectional checking ---------------------------------------------

    /// Check `e` against an expected shape, structurally where the literal
    /// form allows (map literals against data shapes and maps, lists
    /// against lists, function literals against function shapes), and by
    /// infer-then-fits everywhere else.
    fn check_against(&mut self, e: &SExpr, expected: &S) {
        match (&e.expr, expected) {
            (_, S::Unknown) => {
                self.infer(e);
            }
            (Expr::MapLit(items), S::Part(p))
                if self.tables.data_shape_fields.contains_key(p) =>
            {
                let fields = self.tables.data_shape_fields[p].clone();
                let mut seen: Vec<String> = Vec::new();
                let mut spread = false;
                for it in items {
                    match it {
                        MapItem::Entry(k, kspan, v) => {
                            match fields.get(k) {
                                Some((fs, _)) => {
                                    let fs = fs.clone();
                                    self.check_against(v, &fs);
                                }
                                None => {
                                    let nearest = fields
                                        .keys()
                                        .min_by_key(|f| lev(f, k))
                                        .filter(|f| lev(f, k) <= 2);
                                    let note = match nearest {
                                        Some(f) => format!("Did you mean `{}`?", f),
                                        None => format!(
                                            "`{}` declares: {}.",
                                            p,
                                            fields
                                                .keys()
                                                .map(|f| format!("`{}`", f))
                                                .collect::<Vec<_>>()
                                                .join(", ")
                                        ),
                                    };
                                    self.err(
                                        *kspan,
                                        format!("`{}` is not a field of `{}`.", k, p),
                                        note,
                                        vec![],
                                    );
                                }
                            }
                            seen.push(k.clone());
                        }
                        MapItem::Spread(x) => {
                            spread = true;
                            self.infer(x);
                        }
                    }
                }
                if !spread {
                    let missing: Vec<&String> = fields
                        .iter()
                        .filter(|(k, (_, has_default))| !has_default && !seen.contains(k))
                        .map(|(k, _)| k)
                        .collect();
                    if !missing.is_empty() {
                        self.err(
                            e.span,
                            format!(
                                "a `{}` needs {}.",
                                p,
                                missing
                                    .iter()
                                    .map(|f| format!("`{}`", f))
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            ),
                            "Add the missing fields to the literal.".to_string(),
                            vec![],
                        );
                    }
                }
            }
            (Expr::MapLit(items), S::Map(v)) => {
                for it in items {
                    match it {
                        MapItem::Entry(_, _, val) => self.check_against(val, v),
                        MapItem::Spread(x) => {
                            let xs = self.infer(x);
                            if !fits(self.tables, &xs, expected) && !xs.is_unknown() {
                                self.mismatch(x.span, expected, &xs);
                            }
                        }
                    }
                }
            }
            (Expr::List(items), S::List(elem)) => {
                for it in items {
                    match it {
                        ListItem::Item(x) => self.check_against(x, elem),
                        ListItem::Spread(x) => {
                            let xs = self.infer(x);
                            if !fits(self.tables, &xs, expected) && !xs.is_unknown() {
                                self.mismatch(x.span, expected, &xs);
                            }
                        }
                    }
                }
            }
            (Expr::FnLit(params, body), S::Fn(eps, ret)) if params.len() == eps.len() => {
                self.locals.push(BTreeMap::new());
                for p in params {
                    let s = resolve_shape_in(&self.tables.bare, &self.space, &p.shape);
                    self.locals.last_mut().unwrap().insert(p.name.clone(), s);
                }
                match body.as_ref() {
                    FnBody::Expr(x) => self.check_against(x, ret),
                    FnBody::Block(stmts) => {
                        let r = self.walk_block_collecting_returns(stmts);
                        if !fits(self.tables, &r, ret) && !r.is_unknown() {
                            self.mismatch(e.span, ret, &r);
                        }
                    }
                }
                self.locals.pop();
            }
            _ => {
                let actual = self.infer(e);
                if !fits(self.tables, &actual, expected) && !actual.is_unknown() {
                    self.mismatch(e.span, expected, &actual);
                }
            }
        }
    }

    // -- statements ----------------------------------------------------------

    /// Walk a function-body block; the result is the join of every
    /// `return` value (falling off the end contributes `none`).
    fn walk_block_collecting_returns(&mut self, stmts: &[Stmt]) -> S {
        let mut returns: Vec<S> = Vec::new();
        self.walk_stmts(stmts, &mut returns);
        let mut acc = match returns.pop() {
            Some(s) => s,
            None => S::NoneS,
        };
        for r in returns {
            acc = match join(self.tables, &acc, &r) {
                Some(j) => j,
                None => S::Unknown, // mixed returns already diagnosed at use
            };
        }
        acc
    }

    fn walk_stmts(&mut self, stmts: &[Stmt], returns: &mut Vec<S>) {
        self.locals.push(BTreeMap::new());
        for s in stmts {
            match s {
                Stmt::Let(name, _, e) => {
                    let sh = self.infer(e);
                    self.locals.last_mut().unwrap().insert(name.clone(), sh);
                }
                Stmt::Assign(name, span, e) => {
                    let target = self
                        .tables
                        .part_props
                        .get(&self.part_full)
                        .and_then(|m| m.get(name))
                        .cloned()
                        .unwrap_or(S::Unknown);
                    if target.is_unknown() {
                        self.infer(e);
                    } else {
                        self.check_against(e, &target);
                    }
                    let _ = span;
                }
                Stmt::If(cond, then, els) => {
                    self.check_condition(cond);
                    self.walk_stmts(then, returns);
                    if let Some(els) = els {
                        self.walk_stmts(els, returns);
                    }
                }
                Stmt::For(vars, iter, body) => {
                    let it = self.infer(iter);
                    self.locals.push(BTreeMap::new());
                    match (&it, vars.len()) {
                        (S::List(e), 1) => {
                            self.locals
                                .last_mut()
                                .unwrap()
                                .insert(vars[0].0.clone(), (**e).clone());
                        }
                        (S::Map(v), 2) => {
                            self.locals
                                .last_mut()
                                .unwrap()
                                .insert(vars[0].0.clone(), S::Text);
                            self.locals
                                .last_mut()
                                .unwrap()
                                .insert(vars[1].0.clone(), (**v).clone());
                        }
                        (S::List(_), 2) => {
                            self.err(
                                iter.span,
                                "a list iterates with one variable, not two.".to_string(),
                                "Use `for x in xs`; two variables are for maps.".to_string(),
                                vec![],
                            );
                            for (v, _) in vars {
                                self.locals.last_mut().unwrap().insert(v.clone(), S::Unknown);
                            }
                        }
                        (S::Map(_), 1) => {
                            self.err(
                                iter.span,
                                "a map iterates with two variables (key and value).".to_string(),
                                "Use `for k, v in m`.".to_string(),
                                vec![],
                            );
                            for (v, _) in vars {
                                self.locals.last_mut().unwrap().insert(v.clone(), S::Unknown);
                            }
                        }
                        (S::Unknown, _) | (S::Data, _) => {
                            for (v, _) in vars {
                                self.locals.last_mut().unwrap().insert(v.clone(), S::Unknown);
                            }
                        }
                        (other, _) => {
                            self.err(
                                iter.span,
                                format!("`{}` is not iterable.", render(other)),
                                "Iterate a list (`for x in xs`) or a map (`for k, v in m`)."
                                    .to_string(),
                                vec![],
                            );
                            for (v, _) in vars {
                                self.locals.last_mut().unwrap().insert(v.clone(), S::Unknown);
                            }
                        }
                    }
                    self.walk_stmts(body, returns);
                    self.locals.pop();
                }
                Stmt::Return(Some(e), _) => {
                    let s = self.infer(e);
                    returns.push(s);
                }
                Stmt::Return(None, _) => returns.push(S::NoneS),
                Stmt::Expr(e) => {
                    self.infer(e);
                }
            }
        }
        self.locals.pop();
    }

    /// Conditions must be `bool` (reference §6: no truthiness). An optional
    /// condition gets the mechanical `!= none` correction.
    fn check_condition(&mut self, cond: &SExpr) {
        let s = self.infer(cond);
        match s {
            S::Bool | S::Unknown => {}
            S::Opt(_) | S::NoneS => {
                let end = cond.span.end;
                self.err(
                    cond.span,
                    format!("a condition must be `bool`, found `{}`.", render(&s)),
                    "Test presence explicitly with `!= none`.".to_string(),
                    vec![Edit {
                        file: self.file.clone(),
                        start: end,
                        end,
                        text: " != none".to_string(),
                    }],
                );
            }
            other => {
                self.err(
                    cond.span,
                    format!("a condition must be `bool`, found `{}`.", render(&other)),
                    "There is no truthiness; compare explicitly (e.g. `x != 0`, `x != \"\"`)."
                        .to_string(),
                    vec![],
                );
            }
        }
    }

    // -- expression inference -------------------------------------------------

    fn local(&self, name: &str) -> Option<S> {
        self.locals.iter().rev().find_map(|f| f.get(name)).cloned()
    }

    fn infer(&mut self, e: &SExpr) -> S {
        match &e.expr {
            Expr::Text(_) => S::Text,
            Expr::Number(_) => S::Number,
            Expr::Bool(_) => S::Bool,
            Expr::NoneLit => S::NoneS,
            Expr::NameRef(segs) => self.infer_nameref(segs, e.span),
            Expr::List(items) => {
                let mut elem: Option<S> = None;
                for it in items {
                    let s = match it {
                        ListItem::Item(x) => self.infer(x),
                        ListItem::Spread(x) => match self.infer(x) {
                            S::List(i) => *i,
                            S::Unknown | S::Data => S::Unknown,
                            other => {
                                self.err(
                                    x.span,
                                    format!("`...` spreads a list here, found `{}`.", render(&other)),
                                    "Spread a list, or wrap the value in `[ ]`.".to_string(),
                                    vec![],
                                );
                                S::Unknown
                            }
                        },
                    };
                    elem = Some(match elem {
                        None => s,
                        Some(prev) => join(self.tables, &prev, &s).unwrap_or(S::Data),
                    });
                }
                S::List(Box::new(elem.unwrap_or(S::Unknown)))
            }
            Expr::MapLit(items) => {
                let mut val: Option<S> = None;
                for it in items {
                    let s = match it {
                        MapItem::Entry(_, _, v) => self.infer(v),
                        MapItem::Spread(x) => match self.infer(x) {
                            S::Map(v) => *v,
                            S::Unknown | S::Data | S::Part(_) => S::Unknown,
                            other => {
                                self.err(
                                    x.span,
                                    format!("`...` spreads a map here, found `{}`.", render(&other)),
                                    "Spread a map, or write `key: value` entries.".to_string(),
                                    vec![],
                                );
                                S::Unknown
                            }
                        },
                    };
                    val = Some(match val {
                        None => s,
                        Some(prev) => join(self.tables, &prev, &s).unwrap_or(S::Data),
                    });
                }
                S::Map(Box::new(val.unwrap_or(S::Unknown)))
            }
            Expr::Field(base, name, fspan) => {
                let b = self.infer(base);
                self.field_of(&b, name, *fspan)
            }
            Expr::Index(base, idx) => {
                let b = self.infer(base);
                let i = self.infer(idx);
                match b {
                    S::List(elem) => {
                        if !fits(self.tables, &i, &S::Number) && !i.is_unknown() {
                            self.mismatch(idx.span, &S::Number, &i);
                        }
                        elem.opt()
                    }
                    S::Map(v) => {
                        if !fits(self.tables, &i, &S::Text) && !i.is_unknown() {
                            self.mismatch(idx.span, &S::Text, &i);
                        }
                        v.opt()
                    }
                    S::Data => S::Data.opt(),
                    S::Unknown => S::Unknown,
                    S::Opt(_) => {
                        self.err(
                            base.span,
                            format!("indexing a `{}`; it may be `none`.", render(&b)),
                            "Handle `none` first: use `??`, `!`, or `if x != none`.".to_string(),
                            vec![],
                        );
                        S::Unknown
                    }
                    other => {
                        self.err(
                            base.span,
                            format!("`{}` cannot be indexed.", render(&other)),
                            "Index a list with a number or a map with a text key.".to_string(),
                            vec![],
                        );
                        S::Unknown
                    }
                }
            }
            Expr::Call(callee, args) => self.infer_call(callee, args, e.span),
            Expr::Unary(ast::UnOp::Not, x) => {
                let s = self.infer(x);
                if !fits(self.tables, &s, &S::Bool) && !s.is_unknown() {
                    self.mismatch(x.span, &S::Bool, &s);
                }
                S::Bool
            }
            Expr::Unary(ast::UnOp::Neg, x) => {
                let s = self.infer(x);
                if !fits(self.tables, &s, &S::Number) && !s.is_unknown() {
                    self.mismatch(x.span, &S::Number, &s);
                }
                S::Number
            }
            Expr::Assert(x) => match self.infer(x) {
                S::Opt(i) => *i,
                S::NoneS => {
                    self.err(
                        x.span,
                        "`!` on a value that is always `none`.".to_string(),
                        "This always faults at runtime; produce a real value instead.".to_string(),
                        vec![],
                    );
                    S::Unknown
                }
                other => other, // `!` on a never-none value is pointless but harmless
            },
            Expr::Binary(op, l, r) => self.infer_binary(*op, l, r, e.span),
            Expr::IfExpr(cond, then, els) => {
                self.check_condition(cond);
                let t = self.branch_value(then);
                let f = self.branch_value(els);
                match join(self.tables, &t, &f) {
                    Some(j) => j,
                    None => {
                        self.err(
                            e.span,
                            format!(
                                "the branches yield `{}` and `{}`; an `if` expression needs one shape.",
                                render(&t),
                                render(&f)
                            ),
                            "Make both branches yield the same shape.".to_string(),
                            vec![],
                        );
                        S::Unknown
                    }
                }
            }
            Expr::FnLit(params, body) => {
                self.locals.push(BTreeMap::new());
                let mut ps = Vec::new();
                for p in params {
                    let s = resolve_shape_in(&self.tables.bare, &self.space, &p.shape);
                    self.locals.last_mut().unwrap().insert(p.name.clone(), s.clone());
                    ps.push(s);
                }
                let ret = match body.as_ref() {
                    FnBody::Expr(x) => self.infer(x),
                    FnBody::Block(stmts) => self.walk_block_collecting_returns(stmts),
                };
                self.locals.pop();
                S::Fn(ps, Box::new(ret))
            }
        }
    }

    /// The value of an if-expression branch: its final expression statement
    /// (reference §6); other statements are walked for their own checks.
    fn branch_value(&mut self, stmts: &[Stmt]) -> S {
        let mut returns = Vec::new();
        if stmts.len() > 1 {
            self.walk_stmts(&stmts[..stmts.len() - 1], &mut returns);
        }
        match stmts.last() {
            Some(Stmt::Expr(e)) => self.infer(e),
            Some(other) => {
                self.walk_stmts(std::slice::from_ref(other), &mut returns);
                S::NoneS
            }
            None => S::NoneS,
        }
    }

    fn infer_binary(&mut self, op: ast::BinOp, l: &SExpr, r: &SExpr, span: Span) -> S {
        use ast::BinOp::*;
        let ls = self.infer(l);
        let rs = self.infer(r);
        match op {
            Or | And => {
                for (s, x) in [(&ls, l), (&rs, r)] {
                    if !fits(self.tables, s, &S::Bool) && !s.is_unknown() {
                        self.mismatch(x.span, &S::Bool, s);
                    }
                }
                S::Bool
            }
            EqEq | NotEq => {
                if join(self.tables, &ls, &rs).is_none() {
                    self.err(
                        span,
                        format!(
                            "`==` compares two values of one shape; found `{}` and `{}`.",
                            render(&ls),
                            render(&rs)
                        ),
                        "Convert one side so the shapes agree.".to_string(),
                        vec![],
                    );
                }
                S::Bool
            }
            Lt | LtEq | Gt | GtEq => {
                let ok = |s: &S| matches!(s, S::Number | S::Text | S::Unknown | S::Data);
                if !ok(&ls) {
                    self.err(
                        l.span,
                        format!("ordering compares numbers or texts, found `{}`.", render(&ls)),
                        "Compare numbers with numbers or texts with texts.".to_string(),
                        vec![],
                    );
                } else if !ok(&rs) {
                    self.err(
                        r.span,
                        format!("ordering compares numbers or texts, found `{}`.", render(&rs)),
                        "Compare numbers with numbers or texts with texts.".to_string(),
                        vec![],
                    );
                } else if join(self.tables, &ls, &rs).is_none() {
                    self.err(
                        span,
                        format!("cannot order `{}` against `{}`.", render(&ls), render(&rs)),
                        "Both sides must share one shape.".to_string(),
                        vec![],
                    );
                }
                S::Bool
            }
            Coalesce => match ls {
                S::Opt(i) => join(self.tables, &i, &rs).unwrap_or(S::Unknown),
                S::NoneS => rs,
                S::Unknown | S::Data => S::Unknown,
                other => {
                    self.err(
                        l.span,
                        format!("the left of `??` is never `none` (it is `{}`).", render(&other)),
                        "Remove the `??`, or make the left side optional.".to_string(),
                        vec![],
                    );
                    other
                }
            },
            Add => self.infer_add(&ls, &rs, l, r),
            Sub | Mul | Div | Rem => {
                for (s, x) in [(&ls, l), (&rs, r)] {
                    if !fits(self.tables, s, &S::Number) && !s.is_unknown() {
                        self.mismatch(x.span, &S::Number, s);
                    }
                }
                S::Number
            }
        }
    }

    /// `+`: number add, text join, list join. Mixing text and number gets
    /// the reference-promised `text(...)` wrapping fix (§6).
    fn infer_add(&mut self, ls: &S, rs: &S, l: &SExpr, r: &SExpr) -> S {
        use S::*;
        match (ls, rs) {
            (Unknown, _) | (_, Unknown) | (Data, _) | (_, Data) => Unknown,
            (Number, Number) => Number,
            (Text, Text) => Text,
            (List(a), List(b)) => match join(self.tables, a, b) {
                Some(j) => List(Box::new(j)),
                None => {
                    self.err(
                        r.span,
                        format!(
                            "`+` joins two lists of one shape; found `{}` and `{}`.",
                            render(ls),
                            render(rs)
                        ),
                        "Make the element shapes agree.".to_string(),
                        vec![],
                    );
                    Unknown
                }
            },
            (Text, Number) => {
                self.wrap_in_text(r);
                Text
            }
            (Number, Text) => {
                self.wrap_in_text(l);
                Text
            }
            _ => {
                self.err(
                    l.span,
                    format!(
                        "`+` adds numbers, joins texts, or joins lists; found `{}` and `{}`.",
                        render(ls),
                        render(rs)
                    ),
                    "Convert one side so the shapes agree.".to_string(),
                    vec![],
                );
                Unknown
            }
        }
    }

    fn wrap_in_text(&mut self, operand: &SExpr) {
        self.err(
            operand.span,
            "`+` mixes text and number.".to_string(),
            "Convert the number with `text(...)`.".to_string(),
            vec![
                Edit {
                    file: self.file.clone(),
                    start: operand.span.start,
                    end: operand.span.start,
                    text: "text(".to_string(),
                },
                Edit {
                    file: self.file.clone(),
                    start: operand.span.end,
                    end: operand.span.end,
                    text: ")".to_string(),
                },
            ],
        );
    }

    // -- names and fields -----------------------------------------------------

    /// Longest-prefix resolution mirroring the resolver, then trailing
    /// segments as checked field accesses.
    fn infer_nameref(&mut self, segs: &[String], span: Span) -> S {
        let (mut s, consumed) = self.resolve_prefix(segs);
        for name in &segs[consumed..] {
            s = self.field_of(&s, name, span);
        }
        s
    }

    fn resolve_prefix(&mut self, segs: &[String]) -> (S, usize) {
        // Longest first: full part names (and foreign full names).
        for k in (2..=segs.len()).rev() {
            let prefix = segs[..k].join(".");
            if self.tables.part_props.contains_key(&prefix) {
                return (S::Part(prefix), k);
            }
            if let Some(f) = self.tables.foreigns.get(&prefix) {
                return (f.clone(), k);
            }
        }
        let n = &segs[0];
        if let Some(s) = self.local(n) {
            return (s, 1);
        }
        if n == "log" {
            return (S::Part("std.log".into()), 1);
        }
        if let Some(s) = self
            .tables
            .part_props
            .get(&self.part_full)
            .and_then(|m| m.get(n))
        {
            return (s.clone(), 1);
        }
        if let Some(full) = self.tables.bare.get(&self.space).and_then(|m| m.get(n)) {
            return (S::Part(full.clone()), 1);
        }
        for (full, f) in &self.tables.foreigns {
            if full.rsplit('.').next() == Some(n.as_str()) {
                let space = &full[..full.len() - n.len() - 1];
                if space == self.space
                    || self.space_closure_contains(space)
                {
                    return (f.clone(), 1);
                }
            }
        }
        // std functions and anything the resolver already vetted: Unknown
        // keeps the checker silent where it has no signature knowledge.
        (S::Unknown, 1)
    }

    fn space_closure_contains(&self, target: &str) -> bool {
        self.tables
            .fulls
            .get(&self.space)
            .map(|f| f.iter().any(|full| full.starts_with(target)))
            .unwrap_or(false)
    }

    fn field_of(&mut self, base: &S, name: &str, span: Span) -> S {
        match base {
            S::Part(p) => match self.tables.part_props.get(p) {
                Some(props) if props.is_empty() => S::Unknown, // opaque (std.Element)
                Some(props) => match props.get(name) {
                    Some(s) => s.clone(),
                    None => {
                        let nearest = props
                            .keys()
                            .min_by_key(|f| lev(f, name))
                            .filter(|f| lev(f, name) <= 2);
                        let note = match nearest {
                            Some(f) => format!("Did you mean `{}`?", f),
                            None => format!(
                                "`{}` declares: {}.",
                                p,
                                props
                                    .keys()
                                    .map(|f| format!("`{}`", f))
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            ),
                        };
                        self.err(
                            span,
                            format!("`{}` is not a property of `{}`.", name, p),
                            note,
                            vec![],
                        );
                        S::Unknown
                    }
                },
                None => S::Unknown,
            },
            S::Data => S::Data,
            S::Unknown => S::Unknown,
            S::Opt(_) | S::NoneS => {
                self.err(
                    span,
                    format!("accessing `.{}` on a value that may be `none`.", name),
                    "Handle `none` first: use `??`, `!`, or `if x != none`.".to_string(),
                    vec![],
                );
                S::Unknown
            }
            other => {
                self.err(
                    span,
                    format!("`{}` has no fields.", render(other)),
                    "Only parts, data-shape values, and `data` have fields.".to_string(),
                    vec![],
                );
                S::Unknown
            }
        }
    }

    // -- calls ----------------------------------------------------------------

    fn infer_call(&mut self, callee: &SExpr, args: &[SExpr], span: Span) -> S {
        // std function calls by bare name get bespoke signatures.
        if let Expr::NameRef(segs) = &callee.expr {
            if segs.len() == 1
                && STD_FNS.contains(&segs[0].as_str())
                && self.local(&segs[0]).is_none()
                && !self
                    .tables
                    .part_props
                    .get(&self.part_full)
                    .map(|m| m.contains_key(&segs[0]))
                    .unwrap_or(false)
            {
                return self.check_std_call(&segs[0], args, span);
            }
            if segs.len() == 2 && segs[0] == "log" {
                // log.*(message, data?) — message is text, payload permissive.
                if let Some(first) = args.first() {
                    let s = self.infer(first);
                    if !fits(self.tables, &s, &S::Text) && !s.is_unknown() {
                        self.mismatch(first.span, &S::Text, &s);
                    }
                }
                for a in args.iter().skip(1) {
                    self.infer(a);
                }
                return S::NoneS;
            }
        }
        let cs = self.infer(callee);
        match cs {
            S::Fn(params, ret) => {
                if params.len() != args.len() {
                    self.err(
                        span,
                        format!(
                            "this call takes {} argument{}, found {}.",
                            params.len(),
                            if params.len() == 1 { "" } else { "s" },
                            args.len()
                        ),
                        "Match the declared parameters.".to_string(),
                        vec![],
                    );
                    for a in args {
                        self.infer(a);
                    }
                } else {
                    for (a, p) in args.iter().zip(&params) {
                        let p = p.clone();
                        self.check_against(a, &p);
                    }
                }
                *ret
            }
            S::Unknown | S::Data => {
                for a in args {
                    self.infer(a);
                }
                S::Unknown
            }
            other => {
                self.err(
                    callee.span,
                    format!("`{}` is not callable.", render(&other)),
                    "Only functions can be called.".to_string(),
                    vec![],
                );
                for a in args {
                    self.infer(a);
                }
                S::Unknown
            }
        }
    }

    /// Bespoke std signatures (reference §9.11). Polymorphic where the
    /// table is polymorphic; permissive (`Unknown`) for the runtime-wiring
    /// functions whose payloads the runtime owns.
    fn check_std_call(&mut self, name: &str, args: &[SExpr], span: Span) -> S {
        let shapes: Vec<S> = args.iter().map(|a| self.infer(a)).collect();
        let arity = |cx: &mut Cx, n: usize| -> bool {
            if args.len() != n {
                cx.err(
                    span,
                    format!(
                        "`{}` takes {} argument{}, found {}.",
                        name,
                        n,
                        if n == 1 { "" } else { "s" },
                        args.len()
                    ),
                    "Match the builtin's signature.".to_string(),
                    vec![],
                );
                false
            } else {
                true
            }
        };
        let want = |cx: &mut Cx, i: usize, exp: &S| {
            if let (Some(s), Some(a)) = (shapes.get(i), args.get(i)) {
                if !fits(cx.tables, s, exp) && !s.is_unknown() {
                    cx.mismatch(a.span, exp, s);
                }
            }
        };
        match name {
            "len" => {
                if arity(self, 1) {
                    if let Some(s) = shapes.first() {
                        if !matches!(
                            s,
                            S::Text | S::List(_) | S::Map(_) | S::Data | S::Unknown
                        ) {
                            self.err(
                                args[0].span,
                                format!("`len` measures text, lists, or maps; found `{}`.", render(s)),
                                "Pass a text, list, or map.".to_string(),
                                vec![],
                            );
                        }
                    }
                }
                S::Number
            }
            "range" => {
                if arity(self, 1) {
                    want(self, 0, &S::Number);
                }
                S::List(Box::new(S::Number))
            }
            "keys" => {
                if arity(self, 1) {
                    want(self, 0, &S::Map(Box::new(S::Unknown)));
                }
                S::List(Box::new(S::Text))
            }
            "put" => {
                if arity(self, 3) {
                    want(self, 0, &S::Map(Box::new(S::Unknown)));
                    want(self, 1, &S::Text);
                    if let Some(S::Map(v)) = shapes.first() {
                        let exp = (**v).clone();
                        if !exp.is_unknown() {
                            if let (Some(s), Some(a)) = (shapes.get(2), args.get(2)) {
                                if !fits(self.tables, s, &exp) && !s.is_unknown() {
                                    self.mismatch(a.span, &exp, s);
                                }
                            }
                        }
                    }
                }
                shapes.first().cloned().unwrap_or(S::Unknown)
            }
            "drop" => {
                if arity(self, 2) {
                    want(self, 0, &S::Map(Box::new(S::Unknown)));
                    want(self, 1, &S::Text);
                }
                shapes.first().cloned().unwrap_or(S::Unknown)
            }
            "slice" => {
                if arity(self, 3) {
                    if let Some(s) = shapes.first() {
                        if !matches!(s, S::Text | S::List(_) | S::Unknown | S::Data) {
                            self.err(
                                args[0].span,
                                format!("`slice` cuts text or lists; found `{}`.", render(s)),
                                "Pass a text or a list.".to_string(),
                                vec![],
                            );
                        }
                    }
                    want(self, 1, &S::Number);
                    want(self, 2, &S::Number);
                }
                shapes.first().cloned().unwrap_or(S::Unknown)
            }
            "find" => {
                if arity(self, 2) {
                    want(self, 0, &S::List(Box::new(S::Unknown)));
                }
                match shapes.first() {
                    Some(S::List(e)) => (**e).clone().opt(),
                    _ => S::Unknown,
                }
            }
            "map" => {
                if arity(self, 2) {
                    want(self, 0, &S::List(Box::new(S::Unknown)));
                }
                match shapes.get(1) {
                    Some(S::Fn(_, r)) => S::List(Box::new((**r).clone())),
                    _ => S::List(Box::new(S::Unknown)),
                }
            }
            "filter" | "sort" => {
                if arity(self, 2) {
                    want(self, 0, &S::List(Box::new(S::Unknown)));
                }
                shapes.first().cloned().unwrap_or(S::Unknown)
            }
            "join" => {
                if arity(self, 2) {
                    want(self, 0, &S::List(Box::new(S::Text)));
                    want(self, 1, &S::Text);
                }
                S::Text
            }
            "split" => {
                if arity(self, 2) {
                    want(self, 0, &S::Text);
                    want(self, 1, &S::Text);
                }
                S::List(Box::new(S::Text))
            }
            "contains" => {
                if arity(self, 2) {
                    if let Some(s) = shapes.first() {
                        if !matches!(s, S::Text | S::List(_) | S::Unknown | S::Data) {
                            self.err(
                                args[0].span,
                                format!(
                                    "`contains` searches text or lists; found `{}`.",
                                    render(s)
                                ),
                                "Pass a text or a list.".to_string(),
                                vec![],
                            );
                        }
                    }
                }
                S::Bool
            }
            "text" => {
                arity(self, 1);
                S::Text
            }
            "number" => {
                if arity(self, 1) {
                    want(self, 0, &S::Text);
                }
                S::Number.opt()
            }
            "json" => {
                if arity(self, 1) {
                    want(self, 0, &S::Text);
                }
                S::Data.opt()
            }
            "now" => {
                arity(self, 0);
                S::Number
            }
            "id" => {
                arity(self, 0);
                S::Text
            }
            // Runtime wiring: permissive on payloads, checked where obvious.
            "publish" | "subscribe" => {
                if args.len() >= 1 {
                    want(self, 0, &S::Text);
                }
                S::NoneS
            }
            "redirect" => {
                if !args.is_empty() {
                    want(self, 0, &S::Text);
                }
                S::Unknown
            }
            "fail" => {
                // fail(status, message) — §9.9; never returns, so Unknown
                // lets it unify with any surrounding shape (e.g. `x ?? fail(...)`).
                if !args.is_empty() {
                    want(self, 0, &S::Number);
                }
                if args.len() >= 2 {
                    want(self, 1, &S::Text);
                }
                S::Unknown
            }
            "el" | "signup" | "login" | "logout" | "spawn" => S::Unknown,
            _ => S::Unknown,
        }
    }
}

/// `"10m"`-style durations: digits then one of ms/s/m/h/d (§9.7).
fn valid_duration(t: &str) -> bool {
    let units = ["ms", "s", "m", "h", "d"];
    for u in units {
        if let Some(num) = t.strip_suffix(u) {
            if !num.is_empty() && num.chars().all(|c| c.is_ascii_digit()) {
                return true;
            }
        }
    }
    false
}

/// Levenshtein distance for nearest-field suggestions.
fn lev(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for i in 1..=a.len() {
        let mut row = vec![i];
        for j in 1..=b.len() {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            row.push((prev[j] + 1).min(row[j - 1] + 1).min(prev[j - 1] + cost));
        }
        prev = row;
    }
    prev[b.len()]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::check_sources;
    use crate::diag::Diag;

    /// Run the full pipeline on one file and return only checker diags.
    fn e006(src: &str) -> Vec<Diag> {
        let r = check_sources(vec![("t.ash".to_string(), src.to_string())]);
        let non_checker: Vec<_> = r.diags.iter().filter(|d| d.id != "E006").collect();
        assert!(
            non_checker.is_empty(),
            "fixture should only produce E006, got: {:?}",
            non_checker
        );
        r.diags
    }

    fn clean(src: &str) {
        let r = check_sources(vec![("t.ash".to_string(), src.to_string())]);
        assert!(r.diags.is_empty(), "expected clean, got: {:?}", r.diags);
    }

    #[test]
    fn truthy_number_condition_is_e006() {
        let d = e006(
            "space a\n\npart W {\n  go = (n: number) => {\n    if n {\n      log.info(\"x\")\n    }\n  }\n}\n",
        );
        assert_eq!(d.len(), 1);
        assert!(d[0].cause.contains("must be `bool`"));
        assert!(d[0].fix.as_ref().unwrap().note.contains("truthiness"));
    }

    #[test]
    fn optional_condition_gets_not_none_edit() {
        let d = e006(
            "space a\n\npart W {\n  go = (name: text?) => {\n    if name {\n      log.info(\"x\")\n    }\n  }\n}\n",
        );
        assert_eq!(d.len(), 1);
        let fix = d[0].fix.as_ref().unwrap();
        assert_eq!(fix.edits.len(), 1);
        assert_eq!(fix.edits[0].text, " != none");
    }

    #[test]
    fn text_plus_number_gets_wrap_fix() {
        let d = e006("space a\n\npart W {\n  go = (n: number) => \"n = \" + n\n}\n");
        assert_eq!(d.len(), 1);
        let fix = d[0].fix.as_ref().unwrap();
        assert_eq!(fix.edits.len(), 2);
        assert_eq!(fix.edits[0].text, "text(");
        assert_eq!(fix.edits[1].text, ")");
    }

    #[test]
    fn optional_index_in_plain_position_carries_handle_none_note() {
        // The F3 conversion (ADR-0008): misusing an optional lookup is a
        // compile-time correction naming `??` / `!`.
        let d = e006(
            "space a\n\npart W {\n  go = (m: {text: number}) => range(m[\"top\"])\n}\n",
        );
        assert_eq!(d.len(), 1);
        assert!(d[0].cause.contains("`number`") && d[0].cause.contains("`number?`"));
        assert!(d[0].fix.as_ref().unwrap().note.contains("Handle `none` first"));
    }

    #[test]
    fn data_shape_literal_missing_and_unknown_fields() {
        let d = e006(
            "space a\n\npart Message {\n  id: text\n  body: text\n}\n\npart W {\n  go = () => save({ id: \"1\", bod: \"hi\" })\n  save = (m: Message) => m\n}\n",
        );
        // Unknown key `bod` (nearest `body`) and missing `body`.
        assert_eq!(d.len(), 2, "{:?}", d);
        assert!(d.iter().any(|x| x.cause.contains("`bod` is not a field")
            && x.fix.as_ref().unwrap().note.contains("`body`")));
        assert!(d.iter().any(|x| x.cause.contains("needs `body`")));
    }

    #[test]
    fn foreign_call_arity_and_shape() {
        let d = e006(
            "space a\n\nforeign fetch: (url: text) -> data\n\npart W {\n  go = () => fetch(42)\n}\n",
        );
        assert_eq!(d.len(), 1);
        assert!(d[0].cause.contains("expected `text`, found `number`"));

        let d2 = e006(
            "space a\n\nforeign fetch: (url: text) -> data\n\npart W {\n  go = () => fetch(\"u\", \"extra\")\n}\n",
        );
        assert_eq!(d2.len(), 1);
        assert!(d2[0].cause.contains("takes 1 argument, found 2"));
    }

    #[test]
    fn if_expr_branch_mismatch() {
        let d = e006(
            "space a\n\npart W {\n  go = (b: bool) => {\n    let x = if b { \"t\" } else { 2 }\n    return x\n  }\n}\n",
        );
        assert_eq!(d.len(), 1);
        assert!(d[0].cause.contains("branches yield"));
    }

    #[test]
    fn unknown_part_property_with_nearest() {
        let d = e006(
            "space a\n\npart W {\n  port = 8080\n  handle pipe = (req: std.Request) => req.pth\n}\n",
        );
        assert_eq!(d.len(), 1);
        assert!(d[0].cause.contains("`pth` is not a property"));
        assert!(d[0].fix.as_ref().unwrap().note.contains("`path`"));
    }

    #[test]
    fn map_iteration_needs_two_vars() {
        let d = e006(
            "space a\n\npart W {\n  go = (m: {text: number}) => {\n    for x in m {\n      log.info(\"e\")\n    }\n  }\n}\n",
        );
        assert_eq!(d.len(), 1);
        assert!(d[0].cause.contains("two variables"));
    }

    #[test]
    fn bad_every_duration() {
        let d = e006(
            "space a\n\npart sweep {\n  every = \"10 minutes\"\n  run = () => { log.info(\"s\") }\n}\n",
        );
        assert_eq!(d.len(), 1);
        assert!(d[0].cause.contains("not a duration"));
    }

    #[test]
    fn assignment_shape_checked() {
        let d = e006(
            "space a\n\npart W {\n  state n: number = 0\n  go = () => {\n    n = \"hello\"\n  }\n}\n",
        );
        assert_eq!(d.len(), 1);
        assert!(d[0].cause.contains("expected `number`, found `text`"));
    }

    #[test]
    fn composed_program_checks_clean() {
        // The t_a3 24-composed-program fixture, single-file equivalent.
        clean(
            "space chat.data\n\npart Message {\n  id: text\n  body: text\n}\n\npart Store {\n  stored messages: {text: chat.data.Message} = {}\n  add = (m: chat.data.Message) => {\n    messages = put(messages, m.id, m)\n  }\n}\n\npart api {\n  route = \"/api/messages\"\n  handle pipe = (req: std.Request) => {\n    chat.data.Store.add({ id: id(), body: \"hello\" })\n    return chat.data.Store.messages\n  }\n}\n",
        );
    }

    #[test]
    fn coalesce_and_assert_remove_optionality() {
        clean(
            "space a\n\npart W {\n  f = (m: {text: number}) => range(m[\"a\"] ?? 0)\n  g = (m: {text: number}) => range(m[\"a\"]!)\n  h = (name: text?) => \"hi \" + (name ?? \"friend\")\n}\n",
        );
    }
}

#[cfg(test)]
mod route_tests {
    use crate::check_sources;

    fn diags_for(src: &str) -> Vec<&'static str> {
        let r = check_sources(vec![("t.ash".to_string(), src.to_string())]);
        r.diags.iter().map(|d| d.id).collect()
    }

    #[test]
    fn duplicate_routes_conflict() {
        let ids = diags_for(
            "space a\n\npart one {\n  route = \"/api/x\"\n  handle pipe = (req: std.Request) => req.path\n}\n\npart two {\n  route = \"/api/x\"\n  handle pipe = (req: std.Request) => req.path\n}\n",
        );
        assert_eq!(ids, vec!["E021"]);
    }

    #[test]
    fn capture_overlaps_static() {
        let ids = diags_for(
            "space a\n\npart item {\n  route = \"/api/x/{id}\"\n  handle pipe = (req: std.Request) => req.path\n}\n\npart new {\n  route = \"/api/x/new\"\n  handle pipe = (req: std.Request) => req.path\n}\n",
        );
        assert_eq!(ids, vec!["E021"]);
    }

    #[test]
    fn distinct_routes_do_not_conflict() {
        let ids = diags_for(
            "space a\n\npart one {\n  route = \"/api/x\"\n  handle pipe = (req: std.Request) => req.path\n}\n\npart two {\n  route = \"/api/x/{id}\"\n  handle pipe = (req: std.Request) => req.path\n}\n\npart three {\n  route = \"/api/y\"\n  handle pipe = (req: std.Request) => req.path\n}\n",
        );
        assert!(ids.is_empty(), "{:?}", ids);
    }
}
