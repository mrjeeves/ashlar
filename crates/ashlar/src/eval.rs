//! Evaluator (reference §6–§7, §9.3): executes composed programs.
//!
//! Values are immutable (§6); mutation exists only as reassignment of a
//! part's state-class properties, which live in the runtime's state store
//! keyed by full dotted name (`space.Part.prop`) — the same key the
//! `stored` persistence layer uses (§9.3, ADR-0007).
//!
//! Exactly two runtime faults exist (§6, D3): division by zero and `!` on
//! `none`. Everything else the evaluator could reject was rejected at
//! build time by the checker; internal inconsistencies surface as faults
//! with `internal:` causes rather than panics, so a server never dies on
//! a request.
//!
//! Threading: the whole runtime state sits behind one mutex taken per
//! request/task (correctness first; F1 governs build latency, not
//! request throughput). Function values capture their defining locals by
//! value — legal because locals are single-assignment and values are
//! immutable.

use crate::ast::{BinOp, Expr, FnBody, ListItem, MapItem, SExpr, Stmt, UnOp};
use crate::resolved::{ComposedPart, MergedValue, Program};
use std::collections::BTreeMap;
use std::fmt;
use std::rc::Rc;

/// A runtime value.
#[derive(Debug, Clone, PartialEq)]
pub enum V {
    Text(String),
    Number(f64),
    Bool(bool),
    None,
    List(Vec<V>),
    Map(BTreeMap<String, V>),
    /// A callable: parameters plus body, with captured locals.
    Fn(Rc<FnVal>),
    /// A part singleton, addressed by full name; fields resolve lazily.
    Part(String),
}

#[derive(Debug, PartialEq)]
pub struct FnVal {
    /// The part whose scope the function body resolves against.
    pub part: String,
    pub params: Vec<String>,
    pub body: FnBody,
    pub captured: BTreeMap<String, V>,
}

/// A runtime fault: one of the two §6 faults, or a request-level failure
/// raised by `fail(status, message)`.
#[derive(Debug, Clone, PartialEq)]
pub struct Fault {
    pub status: u16,
    pub message: String,
}

impl Fault {
    fn new(message: impl Into<String>) -> Fault {
        Fault {
            status: 500,
            message: message.into(),
        }
    }
}

impl fmt::Display for Fault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.status, self.message)
    }
}

pub type R = Result<V, Fault>;

/// The mutable world: state-class property values, keyed by full name.
#[derive(Debug, Default)]
pub struct StateStore {
    pub values: BTreeMap<String, V>,
    /// Full names of `stored` properties (the persisted subset).
    pub stored_keys: Vec<String>,
    /// Set when any stored value changed since the last flush.
    pub dirty: bool,
}

/// One evaluation context over a checked program.
pub struct Evaluator<'a> {
    pub program: &'a Program,
    pub composed: &'a BTreeMap<String, ComposedPart>,
    pub state: StateStore,
    /// Channel subscriptions: channel name -> handler functions.
    pub subs: BTreeMap<String, Vec<V>>,
    /// Log lines emitted (JSONL); the CLI drains this to stderr.
    pub log: Vec<String>,
}

impl<'a> Evaluator<'a> {
    pub fn new(program: &'a Program, composed: &'a BTreeMap<String, ComposedPart>) -> Self {
        let mut ev = Evaluator {
            program,
            composed,
            state: StateStore::default(),
            subs: BTreeMap::new(),
            log: Vec::new(),
        };
        ev.init_state();
        ev
    }

    /// Initialize every state-class property to its declared initial value.
    fn init_state(&mut self) {
        let fulls: Vec<String> = self.composed.keys().cloned().collect();
        for full in fulls {
            let props: Vec<(String, bool)> = self.composed[&full]
                .props
                .iter()
                .filter(|(_, p)| p.storage.is_some())
                .map(|(n, p)| {
                    (
                        n.clone(),
                        matches!(p.storage, Some(crate::ast::Storage::Stored)),
                    )
                })
                .collect();
            for (name, is_stored) in props {
                let key = format!("{}.{}", full, name);
                let init = self
                    .prop_value_expr(&full, &name)
                    .map(|e| self.eval_in_part(&full, &e))
                    .unwrap_or(Ok(V::None))
                    .unwrap_or(V::None);
                self.state.values.insert(key.clone(), init);
                if is_stored {
                    self.state.stored_keys.push(key);
                }
            }
        }
    }

    /// The source expression of a property's effective (replace-merged)
    /// value, cloned out of the AST.
    fn prop_value_expr(&self, part: &str, prop: &str) -> Option<SExpr> {
        let cp = self.composed.get(part)?;
        let p = cp.props.get(prop)?;
        match &p.value {
            MergedValue::Single(r) => self.program.files[r.file_idx].ast.parts[r.part_idx].props
                [r.prop_idx]
                .value
                .clone(),
            MergedValue::Literal(e) => Some(e.clone()),
            MergedValue::FieldOnly => None,
            MergedValue::Chain(_) => None,
        }
    }

    /// The ordered definition chain of a stack/pipe property.
    fn prop_chain(&self, part: &str, prop: &str) -> Vec<SExpr> {
        let Some(cp) = self.composed.get(part) else {
            return Vec::new();
        };
        let Some(p) = cp.props.get(prop) else {
            return Vec::new();
        };
        let refs: Vec<_> = match &p.value {
            MergedValue::Chain(refs) => refs.clone(),
            MergedValue::Single(r) => vec![r.clone()],
            _ => Vec::new(),
        };
        refs.iter()
            .filter_map(|r| {
                self.program.files[r.file_idx].ast.parts[r.part_idx].props[r.prop_idx]
                    .value
                    .clone()
            })
            .collect()
    }

    /// Evaluate a property's value expression in its part's scope.
    pub fn eval_in_part(&mut self, part: &str, e: &SExpr) -> R {
        let mut env = Env {
            part: part.to_string(),
            frames: vec![BTreeMap::new()],
        };
        self.eval(&mut env, e)
    }

    /// Call a part's function property with arguments (replace semantics:
    /// the effective definition).
    pub fn call_prop(&mut self, part: &str, prop: &str, args: Vec<V>) -> R {
        let Some(e) = self.prop_value_expr(part, prop) else {
            return Err(Fault::new(format!(
                "internal: `{}.{}` has no callable value.",
                part, prop
            )));
        };
        let f = self.eval_in_part(part, &e)?;
        self.call(f, args)
    }

    /// Run a `stack` property (§4): every layer's function runs in order;
    /// a returned map merges one level onto the part's state properties.
    /// `reverse` runs derived-to-base.
    pub fn run_stack(&mut self, part: &str, prop: &str, reverse: bool) -> Result<(), Fault> {
        let mut chain = self.prop_chain(part, prop);
        if reverse {
            chain.reverse();
        }
        for e in chain {
            let f = self.eval_in_part(part, &e)?;
            let out = self.call(f, vec![])?;
            if let V::Map(m) = out {
                for (k, v) in m {
                    let key = format!("{}.{}", part, k);
                    if self.state.values.contains_key(&key) {
                        self.assign_state(&key, v);
                    }
                }
            }
        }
        Ok(())
    }

    /// Run a `pipe` property (§4): every layer's function runs in order,
    /// each receiving the previous return; yields the last return.
    pub fn run_pipe(&mut self, part: &str, prop: &str, reverse: bool, first: V) -> R {
        let mut chain = self.prop_chain(part, prop);
        if reverse {
            chain.reverse();
        }
        let mut acc = first;
        for e in chain {
            let f = self.eval_in_part(part, &e)?;
            acc = self.call(f, vec![acc])?;
        }
        Ok(acc)
    }

    /// A value property evaluated to a number, if it is one (`port`).
    pub fn prop_number(&mut self, part: &str, prop: &str) -> Option<f64> {
        let e = self.prop_value_expr(part, prop)?;
        match self.eval_in_part(part, &e) {
            Ok(V::Number(n)) => Some(n),
            _ => None,
        }
    }

    pub fn assign_state(&mut self, key: &str, v: V) {
        if self.state.stored_keys.iter().any(|k| k == key) {
            self.state.dirty = true;
        }
        self.state.values.insert(key.to_string(), v);
    }

    fn emit_log(&mut self, level: &str, msg: &str, payload: Option<&V>) {
        let mut line = String::from("{\"level\":");
        crate::diag::push_json_str(&mut line, level);
        line.push_str(",\"msg\":");
        crate::diag::push_json_str(&mut line, msg);
        if let Some(p) = payload {
            line.push_str(",\"data\":");
            line.push_str(&to_json(p));
        }
        line.push('}');
        self.log.push(line);
    }

    // -- expression evaluation ----------------------------------------------

    pub fn eval(&mut self, env: &mut Env, e: &SExpr) -> R {
        match &e.expr {
            Expr::Text(s) => Ok(V::Text(s.clone())),
            Expr::Number(n) => Ok(V::Number(*n)),
            Expr::Bool(b) => Ok(V::Bool(*b)),
            Expr::NoneLit => Ok(V::None),
            Expr::NameRef(segs) => self.eval_nameref(env, segs),
            Expr::List(items) => {
                let mut out = Vec::new();
                for it in items {
                    match it {
                        ListItem::Item(x) => out.push(self.eval(env, x)?),
                        ListItem::Spread(x) => match self.eval(env, x)? {
                            V::List(xs) => out.extend(xs),
                            other => {
                                return Err(Fault::new(format!(
                                    "internal: spread a non-list ({}).",
                                    kind_of(&other)
                                )))
                            }
                        },
                    }
                }
                Ok(V::List(out))
            }
            Expr::MapLit(items) => {
                let mut out = BTreeMap::new();
                for it in items {
                    match it {
                        MapItem::Entry(k, _, v) => {
                            let v = self.eval(env, v)?;
                            out.insert(k.clone(), v);
                        }
                        MapItem::Spread(x) => match self.eval(env, x)? {
                            V::Map(m) => out.extend(m),
                            other => {
                                return Err(Fault::new(format!(
                                    "internal: spread a non-map ({}).",
                                    kind_of(&other)
                                )))
                            }
                        },
                    }
                }
                Ok(V::Map(out))
            }
            Expr::Field(b, name, _) => {
                let base = self.eval(env, b)?;
                self.field(base, name)
            }
            Expr::Index(b, i) => {
                let base = self.eval(env, b)?;
                let idx = self.eval(env, i)?;
                match (base, idx) {
                    (V::List(xs), V::Number(n)) => {
                        let i = n as i64;
                        if i >= 0 && (i as usize) < xs.len() {
                            Ok(xs[i as usize].clone())
                        } else {
                            Ok(V::None)
                        }
                    }
                    (V::Map(m), V::Text(k)) => Ok(m.get(&k).cloned().unwrap_or(V::None)),
                    (b, i) => Err(Fault::new(format!(
                        "internal: cannot index {} with {}.",
                        kind_of(&b),
                        kind_of(&i)
                    ))),
                }
            }
            Expr::Call(callee, args) => {
                // std builtins and log.* dispatch by name before value eval.
                if let Expr::NameRef(segs) = &callee.expr {
                    if segs.len() == 2 && segs[0] == "log" && env.get("log").is_none() {
                        let mut vals = Vec::new();
                        for a in args {
                            vals.push(self.eval(env, a)?);
                        }
                        let msg = vals.first().map(to_text).unwrap_or_default();
                        self.emit_log(&segs[1], &msg, vals.get(1));
                        return Ok(V::None);
                    }
                    if segs.len() == 1
                        && crate::resolved::STD_FNS.contains(&segs[0].as_str())
                        && env.get(&segs[0]).is_none()
                        && !self.part_has_prop(&env.part, &segs[0])
                    {
                        let mut vals = Vec::new();
                        for a in args {
                            vals.push(self.eval(env, a)?);
                        }
                        return self.std_call(&segs[0], vals);
                    }
                }
                let f = self.eval(env, callee)?;
                let mut vals = Vec::new();
                for a in args {
                    vals.push(self.eval(env, a)?);
                }
                self.call(f, vals)
            }
            Expr::Unary(UnOp::Not, x) => match self.eval(env, x)? {
                V::Bool(b) => Ok(V::Bool(!b)),
                other => Err(Fault::new(format!(
                    "internal: `not` on {}.",
                    kind_of(&other)
                ))),
            },
            Expr::Unary(UnOp::Neg, x) => match self.eval(env, x)? {
                V::Number(n) => Ok(V::Number(-n)),
                other => Err(Fault::new(format!("internal: `-` on {}.", kind_of(&other)))),
            },
            Expr::Assert(x) => match self.eval(env, x)? {
                V::None => Err(Fault::new("`!` on `none`.".to_string())),
                v => Ok(v),
            },
            Expr::Binary(op, l, r) => self.eval_binary(env, *op, l, r),
            Expr::IfExpr(cond, then, els) => {
                let c = self.eval(env, cond)?;
                let branch = if truthy_bool(&c)? { then } else { els };
                match self.exec_block(env, branch)? {
                    Flow::Returned(v) => Ok(v),
                    Flow::Value(v) => Ok(v),
                    Flow::Normal => Ok(V::None),
                }
            }
            Expr::FnLit(params, body) => Ok(V::Fn(Rc::new(FnVal {
                part: env.part.clone(),
                params: params.iter().map(|p| p.name.clone()).collect(),
                body: (**body).clone(),
                captured: env.flatten(),
            }))),
        }
    }

    fn eval_binary(&mut self, env: &mut Env, op: BinOp, l: &SExpr, r: &SExpr) -> R {
        use BinOp::*;
        match op {
            And => {
                return match self.eval(env, l)? {
                    V::Bool(false) => Ok(V::Bool(false)),
                    V::Bool(true) => self.eval(env, r),
                    other => Err(Fault::new(format!("internal: `and` on {}.", kind_of(&other)))),
                }
            }
            Or => {
                return match self.eval(env, l)? {
                    V::Bool(true) => Ok(V::Bool(true)),
                    V::Bool(false) => self.eval(env, r),
                    other => Err(Fault::new(format!("internal: `or` on {}.", kind_of(&other)))),
                }
            }
            Coalesce => {
                return match self.eval(env, l)? {
                    V::None => self.eval(env, r),
                    v => Ok(v),
                }
            }
            _ => {}
        }
        let lv = self.eval(env, l)?;
        let rv = self.eval(env, r)?;
        match (op, lv, rv) {
            (EqEq, a, b) => Ok(V::Bool(a == b)),
            (NotEq, a, b) => Ok(V::Bool(a != b)),
            (Lt, V::Number(a), V::Number(b)) => Ok(V::Bool(a < b)),
            (LtEq, V::Number(a), V::Number(b)) => Ok(V::Bool(a <= b)),
            (Gt, V::Number(a), V::Number(b)) => Ok(V::Bool(a > b)),
            (GtEq, V::Number(a), V::Number(b)) => Ok(V::Bool(a >= b)),
            (Lt, V::Text(a), V::Text(b)) => Ok(V::Bool(a < b)),
            (LtEq, V::Text(a), V::Text(b)) => Ok(V::Bool(a <= b)),
            (Gt, V::Text(a), V::Text(b)) => Ok(V::Bool(a > b)),
            (GtEq, V::Text(a), V::Text(b)) => Ok(V::Bool(a >= b)),
            (Add, V::Number(a), V::Number(b)) => Ok(V::Number(a + b)),
            (Add, V::Text(a), V::Text(b)) => Ok(V::Text(a + &b)),
            (Add, V::List(mut a), V::List(b)) => {
                a.extend(b);
                Ok(V::List(a))
            }
            (Sub, V::Number(a), V::Number(b)) => Ok(V::Number(a - b)),
            (Mul, V::Number(a), V::Number(b)) => Ok(V::Number(a * b)),
            (Div, V::Number(a), V::Number(b)) => {
                if b == 0.0 {
                    Err(Fault::new("division by zero.".to_string()))
                } else {
                    Ok(V::Number(a / b))
                }
            }
            (Rem, V::Number(a), V::Number(b)) => {
                if b == 0.0 {
                    Err(Fault::new("division by zero.".to_string()))
                } else {
                    Ok(V::Number(a % b))
                }
            }
            (op, a, b) => Err(Fault::new(format!(
                "internal: `{:?}` on {} and {}.",
                op,
                kind_of(&a),
                kind_of(&b)
            ))),
        }
    }

    fn eval_nameref(&mut self, env: &mut Env, segs: &[String]) -> R {
        // Longest-prefix: full part names first.
        for k in (2..=segs.len()).rev() {
            let prefix = segs[..k].join(".");
            if self.composed.contains_key(&prefix) {
                let mut v = V::Part(prefix);
                for name in &segs[k..] {
                    v = self.field(v, name)?;
                }
                return Ok(v);
            }
        }
        let n = &segs[0];
        let mut v = if let Some(local) = env.get(n) {
            local.clone()
        } else if self.part_has_prop(&env.part, n) {
            let part = env.part.clone();
            self.field(V::Part(part), n)?
        } else if let Some(full) = self.unique_bare_part(n) {
            V::Part(full)
        } else if n == "log" {
            V::Part("std.log".to_string())
        } else {
            return Err(Fault::new(format!("internal: `{}` not in scope.", n)));
        };
        for name in &segs[1..] {
            v = self.field(v, name)?;
        }
        Ok(v)
    }

    fn part_has_prop(&self, part: &str, prop: &str) -> bool {
        self.composed
            .get(part)
            .map(|cp| cp.props.contains_key(prop))
            .unwrap_or(false)
    }

    fn unique_bare_part(&self, bare: &str) -> Option<String> {
        let mut hits = self
            .composed
            .keys()
            .filter(|f| f.rsplit('.').next() == Some(bare));
        let first = hits.next()?;
        if hits.next().is_some() {
            None
        } else {
            Some(first.clone())
        }
    }

    /// Field access: part properties (state first, then values/functions),
    /// map keys, request-style data.
    fn field(&mut self, base: V, name: &str) -> R {
        match base {
            V::Part(full) => {
                let key = format!("{}.{}", full, name);
                if let Some(v) = self.state.values.get(&key) {
                    return Ok(v.clone());
                }
                if let Some(e) = self.prop_value_expr(&full, name) {
                    return self.eval_in_part(&full, &e);
                }
                Err(Fault::new(format!(
                    "internal: `{}` has no property `{}`.",
                    full, name
                )))
            }
            V::Map(m) => Ok(m.get(name).cloned().unwrap_or(V::None)),
            V::None => Err(Fault::new(format!(
                "internal: `.{}` on `none`.",
                name
            ))),
            other => Err(Fault::new(format!(
                "internal: `.{}` on {}.",
                name,
                kind_of(&other)
            ))),
        }
    }

    /// Call a function value.
    pub fn call(&mut self, f: V, args: Vec<V>) -> R {
        let V::Fn(fv) = f else {
            return Err(Fault::new(format!(
                "internal: {} is not callable.",
                kind_of(&f)
            )));
        };
        let mut env = Env {
            part: fv.part.clone(),
            frames: vec![fv.captured.clone(), BTreeMap::new()],
        };
        for (p, a) in fv.params.iter().zip(args) {
            env.set(p, a);
        }
        match &fv.body {
            FnBody::Expr(e) => self.eval(&mut env, &e.clone()),
            FnBody::Block(stmts) => match self.exec_block(&mut env, &stmts.clone())? {
                Flow::Returned(v) | Flow::Value(v) => Ok(v),
                Flow::Normal => Ok(V::None),
            },
        }
    }

    // -- statements ----------------------------------------------------------

    fn exec_block(&mut self, env: &mut Env, stmts: &[Stmt]) -> Result<Flow, Fault> {
        env.frames.push(BTreeMap::new());
        let mut last: Flow = Flow::Normal;
        for s in stmts {
            match self.exec_stmt(env, s)? {
                Flow::Returned(v) => {
                    env.frames.pop();
                    return Ok(Flow::Returned(v));
                }
                f => last = f,
            }
        }
        env.frames.pop();
        Ok(last)
    }

    fn exec_stmt(&mut self, env: &mut Env, s: &Stmt) -> Result<Flow, Fault> {
        match s {
            Stmt::Let(name, _, e) => {
                let v = self.eval(env, e)?;
                env.set(name, v);
                Ok(Flow::Normal)
            }
            Stmt::Assign(name, _, e) => {
                let v = self.eval(env, e)?;
                let key = format!("{}.{}", env.part, name);
                self.assign_state(&key, v);
                Ok(Flow::Normal)
            }
            Stmt::If(cond, then, els) => {
                let c = self.eval(env, cond)?;
                if truthy_bool(&c)? {
                    self.exec_block(env, then)
                } else if let Some(els) = els {
                    self.exec_block(env, els)
                } else {
                    Ok(Flow::Normal)
                }
            }
            Stmt::For(vars, iter, body) => {
                let it = self.eval(env, iter)?;
                match (it, vars.len()) {
                    (V::List(xs), 1) => {
                        for x in xs {
                            env.frames.push(BTreeMap::new());
                            env.set(&vars[0].0, x);
                            let f = self.exec_block(env, body)?;
                            env.frames.pop();
                            if let Flow::Returned(v) = f {
                                return Ok(Flow::Returned(v));
                            }
                        }
                        Ok(Flow::Normal)
                    }
                    (V::Map(m), 2) => {
                        for (k, v) in m {
                            env.frames.push(BTreeMap::new());
                            env.set(&vars[0].0, V::Text(k));
                            env.set(&vars[1].0, v);
                            let f = self.exec_block(env, body)?;
                            env.frames.pop();
                            if let Flow::Returned(v) = f {
                                return Ok(Flow::Returned(v));
                            }
                        }
                        Ok(Flow::Normal)
                    }
                    (other, _) => Err(Fault::new(format!(
                        "internal: `for` over {}.",
                        kind_of(&other)
                    ))),
                }
            }
            Stmt::Return(Some(e), _) => {
                let v = self.eval(env, e)?;
                Ok(Flow::Returned(v))
            }
            Stmt::Return(None, _) => Ok(Flow::Returned(V::None)),
            Stmt::Expr(e) => {
                let v = self.eval(env, e)?;
                Ok(Flow::Value(v))
            }
        }
    }

    // -- std builtins at runtime (§9.11) --------------------------------------

    fn std_call(&mut self, name: &str, mut args: Vec<V>) -> R {
        let arg = |args: &mut Vec<V>, i: usize| -> V {
            args.get(i).cloned().unwrap_or(V::None)
        };
        match name {
            "len" => match arg(&mut args, 0) {
                V::Text(s) => Ok(V::Number(s.chars().count() as f64)),
                V::List(xs) => Ok(V::Number(xs.len() as f64)),
                V::Map(m) => Ok(V::Number(m.len() as f64)),
                other => Err(Fault::new(format!("internal: `len` of {}.", kind_of(&other)))),
            },
            "range" => match arg(&mut args, 0) {
                V::Number(n) => Ok(V::List(
                    (0..(n.max(0.0) as i64)).map(|i| V::Number(i as f64)).collect(),
                )),
                other => Err(Fault::new(format!("internal: `range` of {}.", kind_of(&other)))),
            },
            "keys" => match arg(&mut args, 0) {
                V::Map(m) => Ok(V::List(m.keys().cloned().map(V::Text).collect())),
                other => Err(Fault::new(format!("internal: `keys` of {}.", kind_of(&other)))),
            },
            "put" => match (arg(&mut args, 0), arg(&mut args, 1)) {
                (V::Map(mut m), V::Text(k)) => {
                    m.insert(k, arg(&mut args, 2));
                    Ok(V::Map(m))
                }
                (a, b) => Err(Fault::new(format!(
                    "internal: `put` on {} with {}.",
                    kind_of(&a),
                    kind_of(&b)
                ))),
            },
            "drop" => match (arg(&mut args, 0), arg(&mut args, 1)) {
                (V::Map(mut m), V::Text(k)) => {
                    m.remove(&k);
                    Ok(V::Map(m))
                }
                (a, b) => Err(Fault::new(format!(
                    "internal: `drop` on {} with {}.",
                    kind_of(&a),
                    kind_of(&b)
                ))),
            },
            "slice" => {
                let (from, to) = match (arg(&mut args, 1), arg(&mut args, 2)) {
                    (V::Number(f), V::Number(t)) => (f.max(0.0) as usize, t.max(0.0) as usize),
                    _ => return Err(Fault::new("internal: `slice` bounds.".to_string())),
                };
                match arg(&mut args, 0) {
                    V::Text(s) => {
                        let chars: Vec<char> = s.chars().collect();
                        let to = to.min(chars.len());
                        let from = from.min(to);
                        Ok(V::Text(chars[from..to].iter().collect()))
                    }
                    V::List(xs) => {
                        let to = to.min(xs.len());
                        let from = from.min(to);
                        Ok(V::List(xs[from..to].to_vec()))
                    }
                    other => Err(Fault::new(format!("internal: `slice` of {}.", kind_of(&other)))),
                }
            }
            "find" => {
                let f = arg(&mut args, 1);
                match arg(&mut args, 0) {
                    V::List(xs) => {
                        for x in xs {
                            if self.call(f.clone(), vec![x.clone()])? == V::Bool(true) {
                                return Ok(x);
                            }
                        }
                        Ok(V::None)
                    }
                    other => Err(Fault::new(format!("internal: `find` in {}.", kind_of(&other)))),
                }
            }
            "map" => {
                let f = arg(&mut args, 1);
                match arg(&mut args, 0) {
                    V::List(xs) => {
                        let mut out = Vec::new();
                        for x in xs {
                            out.push(self.call(f.clone(), vec![x])?);
                        }
                        Ok(V::List(out))
                    }
                    other => Err(Fault::new(format!("internal: `map` over {}.", kind_of(&other)))),
                }
            }
            "filter" => {
                let f = arg(&mut args, 1);
                match arg(&mut args, 0) {
                    V::List(xs) => {
                        let mut out = Vec::new();
                        for x in xs {
                            if self.call(f.clone(), vec![x.clone()])? == V::Bool(true) {
                                out.push(x);
                            }
                        }
                        Ok(V::List(out))
                    }
                    other => Err(Fault::new(format!(
                        "internal: `filter` over {}.",
                        kind_of(&other)
                    ))),
                }
            }
            "sort" => {
                let f = arg(&mut args, 1);
                match arg(&mut args, 0) {
                    V::List(xs) => {
                        let mut keyed: Vec<(V, V)> = Vec::new();
                        for x in xs {
                            let k = self.call(f.clone(), vec![x.clone()])?;
                            keyed.push((k, x));
                        }
                        keyed.sort_by(|(a, _), (b, _)| cmp_values(a, b));
                        Ok(V::List(keyed.into_iter().map(|(_, x)| x).collect()))
                    }
                    other => Err(Fault::new(format!("internal: `sort` of {}.", kind_of(&other)))),
                }
            }
            "join" => match (arg(&mut args, 0), arg(&mut args, 1)) {
                (V::List(xs), V::Text(sep)) => {
                    let parts: Vec<String> = xs.iter().map(to_text).collect();
                    Ok(V::Text(parts.join(&sep)))
                }
                (a, b) => Err(Fault::new(format!(
                    "internal: `join` on {} with {}.",
                    kind_of(&a),
                    kind_of(&b)
                ))),
            },
            "split" => match (arg(&mut args, 0), arg(&mut args, 1)) {
                (V::Text(s), V::Text(sep)) => Ok(V::List(
                    s.split(&sep).map(|p| V::Text(p.to_string())).collect(),
                )),
                (a, b) => Err(Fault::new(format!(
                    "internal: `split` on {} with {}.",
                    kind_of(&a),
                    kind_of(&b)
                ))),
            },
            "contains" => match (arg(&mut args, 0), arg(&mut args, 1)) {
                (V::Text(s), V::Text(n)) => Ok(V::Bool(s.contains(&n))),
                (V::List(xs), y) => Ok(V::Bool(xs.contains(&y))),
                (a, b) => Err(Fault::new(format!(
                    "internal: `contains` on {} with {}.",
                    kind_of(&a),
                    kind_of(&b)
                ))),
            },
            "text" => Ok(V::Text(to_text(&arg(&mut args, 0)))),
            "number" => match arg(&mut args, 0) {
                V::Text(s) => Ok(s.trim().parse::<f64>().map(V::Number).unwrap_or(V::None)),
                other => Err(Fault::new(format!(
                    "internal: `number` of {}.",
                    kind_of(&other)
                ))),
            },
            "json" => match arg(&mut args, 0) {
                V::Text(s) => Ok(from_json(&s).unwrap_or(V::None)),
                other => Err(Fault::new(format!("internal: `json` of {}.", kind_of(&other)))),
            },
            "now" => Ok(V::Number(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_millis() as f64)
                    .unwrap_or(0.0),
            )),
            "id" => {
                use std::sync::atomic::{AtomicU64, Ordering};
                static COUNTER: AtomicU64 = AtomicU64::new(0);
                let n = COUNTER.fetch_add(1, Ordering::Relaxed);
                let t = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_nanos())
                    .unwrap_or(0);
                Ok(V::Text(format!("id-{:x}-{:x}", t, n)))
            }
            "publish" => {
                let channel = match arg(&mut args, 0) {
                    V::Text(c) => c,
                    other => {
                        return Err(Fault::new(format!(
                            "internal: `publish` to {}.",
                            kind_of(&other)
                        )))
                    }
                };
                let payload = arg(&mut args, 1);
                let handlers = self.subs.get(&channel).cloned().unwrap_or_default();
                for h in handlers {
                    self.call(h, vec![payload.clone()])?;
                }
                Ok(V::None)
            }
            "subscribe" => {
                let channel = match arg(&mut args, 0) {
                    V::Text(c) => c,
                    other => {
                        return Err(Fault::new(format!(
                            "internal: `subscribe` to {}.",
                            kind_of(&other)
                        )))
                    }
                };
                self.subs.entry(channel).or_default().push(arg(&mut args, 1));
                Ok(V::None)
            }
            "fail" => {
                let status = match arg(&mut args, 0) {
                    V::Number(n) => n as u16,
                    _ => 500,
                };
                let message = to_text(&arg(&mut args, 1));
                Err(Fault { status, message })
            }
            "redirect" => {
                let mut m = BTreeMap::new();
                m.insert("__redirect".to_string(), arg(&mut args, 0));
                Ok(V::Map(m))
            }
            "el" => {
                let mut m = BTreeMap::new();
                m.insert("__el".to_string(), arg(&mut args, 0));
                m.insert("attrs".to_string(), arg(&mut args, 1));
                m.insert("children".to_string(), arg(&mut args, 2));
                Ok(V::Map(m))
            }
            "spawn" => {
                // v1: background tasks run to completion inline; the
                // scheduling improvement is a runtime concern, not a
                // semantic one (the task may not observe request state).
                let f = arg(&mut args, 0);
                self.call(f, vec![])?;
                Ok(V::None)
            }
            "signup" | "login" | "logout" => {
                // Session wiring lives in the HTTP layer; the evaluator
                // exposes the auth intent for it to act on.
                let mut m = BTreeMap::new();
                m.insert(format!("__{}", name), V::List(args));
                Ok(V::Map(m))
            }
            other => Err(Fault::new(format!("internal: unknown builtin `{}`.", other))),
        }
    }
}

enum Flow {
    Normal,
    /// A bare expression statement's value (if-expression branches).
    Value(V),
    Returned(V),
}

fn truthy_bool(v: &V) -> Result<bool, Fault> {
    match v {
        V::Bool(b) => Ok(*b),
        other => Err(Fault::new(format!(
            "internal: condition was {}.",
            kind_of(other)
        ))),
    }
}

fn kind_of(v: &V) -> &'static str {
    match v {
        V::Text(_) => "text",
        V::Number(_) => "number",
        V::Bool(_) => "bool",
        V::None => "none",
        V::List(_) => "a list",
        V::Map(_) => "a map",
        V::Fn(_) => "a function",
        V::Part(_) => "a part",
    }
}

fn cmp_values(a: &V, b: &V) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    match (a, b) {
        (V::Number(x), V::Number(y)) => x.partial_cmp(y).unwrap_or(Ordering::Equal),
        (V::Text(x), V::Text(y)) => x.cmp(y),
        _ => Ordering::Equal,
    }
}

/// `text(x)`: texts pass through; everything else renders as JSON, except
/// `none` which is the word.
pub fn to_text(v: &V) -> String {
    match v {
        V::Text(s) => s.clone(),
        V::None => "none".to_string(),
        V::Number(_) | V::Bool(_) | V::List(_) | V::Map(_) => to_json(v),
        V::Fn(_) => "<function>".to_string(),
        V::Part(p) => p.clone(),
    }
}

/// Local scope: the enclosing part plus lexical frames.
pub struct Env {
    part: String,
    frames: Vec<BTreeMap<String, V>>,
}

impl Env {
    fn get(&self, name: &str) -> Option<&V> {
        self.frames.iter().rev().find_map(|f| f.get(name))
    }
    fn set(&mut self, name: &str, v: V) {
        self.frames.last_mut().unwrap().insert(name.to_string(), v);
    }
    fn flatten(&self) -> BTreeMap<String, V> {
        let mut out = BTreeMap::new();
        for f in &self.frames {
            for (k, v) in f {
                out.insert(k.clone(), v.clone());
            }
        }
        out
    }
}

/// Render a value as JSON (§9.2 response rendering; also `log` payloads).
pub fn to_json(v: &V) -> String {
    match v {
        V::Text(s) => {
            let mut out = String::new();
            crate::diag::push_json_str(&mut out, s);
            out
        }
        V::Number(n) => {
            if n.fract() == 0.0 && n.abs() < 9.0e15 {
                format!("{}", *n as i64)
            } else {
                format!("{}", n)
            }
        }
        V::Bool(b) => format!("{}", b),
        V::None => "null".to_string(),
        V::List(items) => {
            let inner: Vec<String> = items.iter().map(to_json).collect();
            format!("[{}]", inner.join(","))
        }
        V::Map(m) => {
            let inner: Vec<String> = m
                .iter()
                .map(|(k, v)| {
                    let mut key = String::new();
                    crate::diag::push_json_str(&mut key, k);
                    format!("{}:{}", key, to_json(v))
                })
                .collect();
            format!("{{{}}}", inner.join(","))
        }
        V::Fn(_) => "null".to_string(),
        V::Part(p) => {
            let mut out = String::new();
            crate::diag::push_json_str(&mut out, p);
            out
        }
    }
}

/// Parse JSON into a value (`json(t)` builtin and request bodies).
pub fn from_json(s: &str) -> Option<V> {
    let mut chars: Vec<char> = s.chars().collect();
    chars.push('\0');
    let mut pos = 0usize;
    let v = json_value(&chars, &mut pos)?;
    json_ws(&chars, &mut pos);
    if chars[pos] == '\0' {
        Some(v)
    } else {
        None
    }
}

fn json_ws(c: &[char], p: &mut usize) {
    while matches!(c[*p], ' ' | '\t' | '\n' | '\r') {
        *p += 1;
    }
}

fn json_value(c: &[char], p: &mut usize) -> Option<V> {
    json_ws(c, p);
    match c[*p] {
        '"' => json_string(c, p).map(V::Text),
        '{' => {
            *p += 1;
            let mut m = BTreeMap::new();
            json_ws(c, p);
            if c[*p] == '}' {
                *p += 1;
                return Some(V::Map(m));
            }
            loop {
                json_ws(c, p);
                let k = json_string(c, p)?;
                json_ws(c, p);
                if c[*p] != ':' {
                    return None;
                }
                *p += 1;
                let v = json_value(c, p)?;
                m.insert(k, v);
                json_ws(c, p);
                match c[*p] {
                    ',' => *p += 1,
                    '}' => {
                        *p += 1;
                        return Some(V::Map(m));
                    }
                    _ => return None,
                }
            }
        }
        '[' => {
            *p += 1;
            let mut items = Vec::new();
            json_ws(c, p);
            if c[*p] == ']' {
                *p += 1;
                return Some(V::List(items));
            }
            loop {
                let v = json_value(c, p)?;
                items.push(v);
                json_ws(c, p);
                match c[*p] {
                    ',' => *p += 1,
                    ']' => {
                        *p += 1;
                        return Some(V::List(items));
                    }
                    _ => return None,
                }
            }
        }
        't' => {
            if c[*p..].starts_with(&['t', 'r', 'u', 'e']) {
                *p += 4;
                Some(V::Bool(true))
            } else {
                None
            }
        }
        'f' => {
            if c[*p..].starts_with(&['f', 'a', 'l', 's', 'e']) {
                *p += 5;
                Some(V::Bool(false))
            } else {
                None
            }
        }
        'n' => {
            if c[*p..].starts_with(&['n', 'u', 'l', 'l']) {
                *p += 4;
                Some(V::None)
            } else {
                None
            }
        }
        '-' | '0'..='9' => {
            let start = *p;
            if c[*p] == '-' {
                *p += 1;
            }
            while c[*p].is_ascii_digit() {
                *p += 1;
            }
            if c[*p] == '.' {
                *p += 1;
                while c[*p].is_ascii_digit() {
                    *p += 1;
                }
            }
            if matches!(c[*p], 'e' | 'E') {
                *p += 1;
                if matches!(c[*p], '+' | '-') {
                    *p += 1;
                }
                while c[*p].is_ascii_digit() {
                    *p += 1;
                }
            }
            let text: String = c[start..*p].iter().collect();
            text.parse::<f64>().ok().map(V::Number)
        }
        _ => None,
    }
}

fn json_string(c: &[char], p: &mut usize) -> Option<String> {
    if c[*p] != '"' {
        return None;
    }
    *p += 1;
    let mut out = String::new();
    loop {
        match c[*p] {
            '\0' => return None,
            '"' => {
                *p += 1;
                return Some(out);
            }
            '\\' => {
                *p += 1;
                match c[*p] {
                    '"' => out.push('"'),
                    '\\' => out.push('\\'),
                    '/' => out.push('/'),
                    'n' => out.push('\n'),
                    't' => out.push('\t'),
                    'r' => out.push('\r'),
                    'b' => out.push('\u{8}'),
                    'f' => out.push('\u{c}'),
                    'u' => {
                        let hex: String = c[*p + 1..*p + 5].iter().collect();
                        let code = u32::from_str_radix(&hex, 16).ok()?;
                        out.push(char::from_u32(code).unwrap_or('\u{fffd}'));
                        *p += 4;
                    }
                    _ => return None,
                }
                *p += 1;
            }
            ch => {
                out.push(ch);
                *p += 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::check_sources;

    fn eval_prop(src: &str, part: &str, prop: &str, args: Vec<V>) -> R {
        let r = check_sources(vec![("t.ash".to_string(), src.to_string())]);
        assert!(r.diags.is_empty(), "fixture must be clean: {:?}", r.diags);
        let mut ev = Evaluator::new(&r.program, &r.composed);
        ev.call_prop(part, prop, args)
    }

    #[test]
    fn arithmetic_text_and_lists() {
        let src = "space a\n\npart W {\n  f = (n: number) => \"n = \" + text(n * 2 + 1)\n  g = (xs: [text]) => [...xs, \"end\"]\n}\n";
        assert_eq!(
            eval_prop(src, "a.W", "f", vec![V::Number(20.0)]).unwrap(),
            V::Text("n = 41".to_string())
        );
        assert_eq!(
            eval_prop(src, "a.W", "g", vec![V::List(vec![V::Text("x".into())])]).unwrap(),
            V::List(vec![V::Text("x".into()), V::Text("end".into())])
        );
    }

    #[test]
    fn the_two_runtime_faults() {
        let src = "space a\n\npart W {\n  d = (n: number) => 1 / n\n  b = (xs: [text]) => xs[0]!\n}\n";
        let f = eval_prop(src, "a.W", "d", vec![V::Number(0.0)]).unwrap_err();
        assert!(f.message.contains("division by zero"));
        let f = eval_prop(src, "a.W", "b", vec![V::List(vec![])]).unwrap_err();
        assert!(f.message.contains("`!` on `none`"));
        // And the non-fault paths.
        assert_eq!(
            eval_prop(src, "a.W", "d", vec![V::Number(2.0)]).unwrap(),
            V::Number(0.5)
        );
    }

    #[test]
    fn state_assignment_and_reads() {
        let src = "space a\n\npart Counter {\n  state n: number = 0\n  bump = () => { n = n + 1 }\n}\n";
        let r = check_sources(vec![("t.ash".to_string(), src.to_string())]);
        assert!(r.diags.is_empty());
        let mut ev = Evaluator::new(&r.program, &r.composed);
        assert_eq!(ev.state.values["a.Counter.n"], V::Number(0.0));
        ev.call_prop("a.Counter", "bump", vec![]).unwrap();
        ev.call_prop("a.Counter", "bump", vec![]).unwrap();
        assert_eq!(ev.state.values["a.Counter.n"], V::Number(2.0));
    }

    #[test]
    fn stack_merges_and_pipe_threads() {
        let src = "space srv\n\npart Server {\n  port = 8080\n  state ready: bool = false\n  start stack = () => {\n    return { ready: true }\n  }\n  handle pipe = (req: {text: text}) => put(req, \"a\", \"1\")\n}\n\n// second layer via a second space\n";
        let src2 = "space srv.more\nuse srv\n\npart srv.Server {\n  start stack = () => none\n  handle pipe = (req: {text: text}) => put(req, \"b\", \"2\")\n}\n";
        let r = check_sources(vec![
            ("a.ash".to_string(), src.replace("\n// second layer via a second space\n", "")),
            ("b.ash".to_string(), src2.to_string()),
        ]);
        assert!(r.diags.is_empty(), "{:?}", r.diags);
        let mut ev = Evaluator::new(&r.program, &r.composed);
        ev.run_stack("srv.Server", "start", false).unwrap();
        assert_eq!(ev.state.values["srv.Server.ready"], V::Bool(true));
        let out = ev
            .run_pipe("srv.Server", "handle", false, V::Map(BTreeMap::new()))
            .unwrap();
        let V::Map(m) = out else { panic!() };
        assert_eq!(m.get("a"), Some(&V::Text("1".into())));
        assert_eq!(m.get("b"), Some(&V::Text("2".into())));
    }

    #[test]
    fn pubsub_and_closures() {
        let src = "space n\n\npart A {\n  state seen: number = 0\n  go = () => {\n    subscribe(\"c\", (m: data) => bump())\n    publish(\"c\", { x: 1 })\n    publish(\"c\", { x: 2 })\n  }\n  bump = () => { seen = seen + 1 }\n}\n";
        let r = check_sources(vec![("t.ash".to_string(), src.to_string())]);
        assert!(r.diags.is_empty(), "{:?}", r.diags);
        let mut ev = Evaluator::new(&r.program, &r.composed);
        ev.call_prop("n.A", "go", vec![]).unwrap();
        assert_eq!(ev.state.values["n.A.seen"], V::Number(2.0));
    }

    #[test]
    fn std_collection_builtins() {
        let src = "space u\n\npart W {\n  f = (m: {text: number}) => join(map(keys(m), (k: text) => k + \"=\" + text(m[k]!)), \",\")\n  g = (xs: [number]) => filter(xs, (x: number) => x > 1)\n}\n";
        let mut m = BTreeMap::new();
        m.insert("a".to_string(), V::Number(1.0));
        m.insert("b".to_string(), V::Number(2.0));
        assert_eq!(
            eval_prop(src, "u.W", "f", vec![V::Map(m)]).unwrap(),
            V::Text("a=1,b=2".to_string())
        );
        assert_eq!(
            eval_prop(
                src,
                "u.W",
                "g",
                vec![V::List(vec![V::Number(1.0), V::Number(2.0), V::Number(3.0)])]
            )
            .unwrap(),
            V::List(vec![V::Number(2.0), V::Number(3.0)])
        );
    }

    #[test]
    fn if_expr_for_loops_and_json_roundtrip() {
        let src = "space u\n\npart W {\n  f = (read: bool) => if read { \"seen\" } else { \"new\" }\n  g = (m: {text: number}) => {\n    let out = json(\"[1, {\\\"a\\\": true}, null]\")\n    return out\n  }\n}\n";
        assert_eq!(
            eval_prop(src, "u.W", "f", vec![V::Bool(true)]).unwrap(),
            V::Text("seen".into())
        );
        let out = eval_prop(src, "u.W", "g", vec![V::Map(BTreeMap::new())]).unwrap();
        let V::List(items) = out else { panic!() };
        assert_eq!(items[0], V::Number(1.0));
        assert_eq!(items[2], V::None);
        let V::Map(m) = &items[1] else { panic!() };
        assert_eq!(m.get("a"), Some(&V::Bool(true)));
    }

    #[test]
    fn fail_carries_status() {
        let src = "space u\n\npart W {\n  f = (m: {text: text}) => m[\"k\"] ?? fail(404, \"missing\")\n}\n";
        let err = eval_prop(src, "u.W", "f", vec![V::Map(BTreeMap::new())]).unwrap_err();
        assert_eq!(err.status, 404);
        assert_eq!(err.message, "missing");
    }
}
