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
    /// A foreign function by full name, bound at first call (§9.10).
    ForeignFn(String),
}

#[derive(Debug, PartialEq)]
pub struct FnVal {
    /// The part whose scope the function body resolves against.
    pub part: String,
    /// The view instance the function was created in, if any (§9.4).
    pub instance: Option<String>,
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

/// A live view instance (§9.4): a view part used with `el` instantiates
/// per use — fields from the call site, `state` per instance.
#[derive(Debug, Clone, Default)]
pub struct Instance {
    pub part: String,
    pub fields: BTreeMap<String, V>,
    pub state: BTreeMap<String, V>,
    /// The page render this instance belongs to; when that page's socket
    /// closes, the instance unmounts (§9.5).
    pub page: Option<String>,
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
    /// Live view instances by id (§9.4).
    pub instances: BTreeMap<String, Instance>,
    /// Event handlers registered during renders: (instance, handler id) -> fn.
    pub handlers: BTreeMap<(String, String), V>,
    /// Instances whose state changed during the current event.
    pub dirty_instances: Vec<String>,
    /// Accounts: email -> (id, password hash). Persisted with `stored`.
    pub users: BTreeMap<String, (String, String)>,
    /// Sessions: token -> user id. Process-lifetime.
    pub sessions: BTreeMap<String, String>,
    /// The request's session context, set by the HTTP layer per dispatch.
    pub current_session: Option<String>,
    /// A session opened (Some(token)) or closed (Some empty) this request.
    pub pending_cookie: Option<String>,
    /// Functions queued by `spawn`, drained by the serve loop after the
    /// current request completes (§9.7).
    pub spawn_queue: Vec<V>,
    /// Read dependencies: state key -> instances whose last render read it.
    /// State keys are `space.Part.prop` (singletons) and
    /// `instance:<id>.<prop>` (per-instance).
    pub deps: BTreeMap<String, std::collections::BTreeSet<String>>,
    /// The instance currently rendering (reads record into `deps`).
    pub current_render: Option<String>,
    /// The page whose request/socket is being served; instances created
    /// now belong to it and unmount when its socket closes (§9.5).
    pub current_page: Option<String>,
    /// Project root for `foreign/<space>.so` resolution (§9.10).
    pub foreign_root: Option<std::path::PathBuf>,
    /// dlopen handles by space, opened lazily.
    foreign_libs: BTreeMap<String, usize>,
    counter: u64,
}

impl<'a> Evaluator<'a> {
    pub fn new(program: &'a Program, composed: &'a BTreeMap<String, ComposedPart>) -> Self {
        let mut ev = Evaluator {
            program,
            composed,
            state: StateStore::default(),
            subs: BTreeMap::new(),
            log: Vec::new(),
            instances: BTreeMap::new(),
            handlers: BTreeMap::new(),
            dirty_instances: Vec::new(),
            users: BTreeMap::new(),
            sessions: BTreeMap::new(),
            current_session: None,
            pending_cookie: None,
            spawn_queue: Vec::new(),
            deps: BTreeMap::new(),
            current_render: None,
            current_page: None,
            foreign_root: None,
            foreign_libs: BTreeMap::new(),
            counter: 0,
        };
        ev.init_state();
        ev
    }

    fn fresh_id(&mut self, prefix: &str) -> String {
        self.counter += 1;
        format!("{}{}", prefix, self.counter)
    }

    /// Open a fresh page context: subsequently created instances belong
    /// to it until the next `begin_page`/clear (§9.5).
    pub fn begin_page(&mut self) -> String {
        let p = self.fresh_id("p");
        self.current_page = Some(p.clone());
        p
    }

    /// A unique 8-byte salt: process clock plus the monotone counter,
    /// mixed through SHA-1. Uniqueness (not secrecy) is what a salt needs.
    fn fresh_salt(&mut self) -> Vec<u8> {
        self.counter += 1;
        let ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let seed = format!("{}:{}", ms, self.counter);
        crate::http::sha1(seed.as_bytes())[..8].to_vec()
    }

    /// Create a view instance of `part` with the given fields; `state`
    /// properties initialize per instance (§9.4).
    pub fn new_instance(&mut self, part: &str, fields: BTreeMap<String, V>) -> Result<String, Fault> {
        let id = self.fresh_id("i");
        let mut state = BTreeMap::new();
        if let Some(cp) = self.composed.get(part) {
            let names: Vec<String> = cp
                .props
                .iter()
                .filter(|(_, p)| p.storage.is_some())
                .map(|(n, _)| n.clone())
                .collect();
            for name in names {
                let init = self
                    .prop_value_expr(part, &name)
                    .map(|e| self.eval_in_part(part, &e))
                    .unwrap_or(Ok(V::None))?;
                state.insert(name, init);
            }
        }
        self.instances.insert(
            id.clone(),
            Instance {
                part: part.to_string(),
                fields,
                state,
                page: self.current_page.clone(),
            },
        );
        // Mounting runs the instance's `start` stack (§9.4/§9.5):
        // subscriptions made there carry the instance and die with it.
        self.run_instance_stack(&id, "start", false)?;
        Ok(id)
    }

    /// Run a stack property in an INSTANCE's scope: every layer in
    /// composition order (reverse for teardown), returned maps merging
    /// onto the instance's own state (§9.4: state is per-instance).
    pub fn run_instance_stack(
        &mut self,
        id: &str,
        prop: &str,
        reverse: bool,
    ) -> Result<(), Fault> {
        let part = match self.instances.get(id) {
            Some(i) => i.part.clone(),
            None => return Ok(()),
        };
        let mut chain = self.prop_chain(&part, prop);
        if reverse {
            chain.reverse();
        }
        for e in chain {
            let mut env = Env {
                part: part.clone(),
                instance: Some(id.to_string()),
                frames: vec![BTreeMap::new()],
            };
            let f = self.eval(&mut env, &e)?;
            let out = self.call_with_instance(f, vec![], Some(id.to_string()))?;
            if let V::Map(m) = out {
                for (k, v) in m {
                    if let Some(inst) = self.instances.get_mut(id) {
                        if inst.state.contains_key(&k) {
                            inst.state.insert(k.clone(), v);
                            self.dirty_readers(&format!("instance:{}.{}", id, k));
                        }
                    }
                }
            }
        }
        Ok(())
    }

    /// Unmount every instance belonging to `page` (§9.5): run their `stop`
    /// stacks (teardown order), then remove the instances, their event
    /// handlers, their dependency edges, and every channel subscription
    /// their lifetime created.
    pub fn unmount_page(&mut self, page: &str) {
        let ids: Vec<String> = self
            .instances
            .iter()
            .filter(|(_, i)| i.page.as_deref() == Some(page))
            .map(|(id, _)| id.clone())
            .collect();
        for id in &ids {
            let _ = self.run_instance_stack(id, "stop", true);
        }
        for id in &ids {
            self.instances.remove(id);
            self.handlers.retain(|(inst, _), _| inst != id);
            let prefix = format!("instance:{}.", id);
            self.deps.retain(|k, _| !k.starts_with(&prefix));
            for readers in self.deps.values_mut() {
                readers.remove(id);
            }
            self.dirty_instances.retain(|d| d != id);
            for handlers in self.subs.values_mut() {
                handlers.retain(|h| match h {
                    V::Fn(f) => f.instance.as_deref() != Some(id.as_str()),
                    _ => true,
                });
            }
        }
    }

    /// Call a function property in an instance's scope.
    pub fn call_instance_prop(&mut self, instance: &str, prop: &str, args: Vec<V>) -> R {
        let part = match self.instances.get(instance) {
            Some(i) => i.part.clone(),
            None => return Err(Fault::new(format!("internal: no instance `{}`.", instance))),
        };
        let Some(e) = self.prop_value_expr(&part, prop) else {
            return Err(Fault::new(format!(
                "internal: `{}.{}` has no callable value.",
                part, prop
            )));
        };
        let mut env = Env {
            part: part.clone(),
            instance: Some(instance.to_string()),
            frames: vec![BTreeMap::new()],
        };
        let f = self.eval(&mut env, &e)?;
        self.call_with_instance(f, args, Some(instance.to_string()))
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
            instance: None,
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
        self.dirty_readers(key);
    }

    /// Record a state read during a render (the §9.4 dependency edge).
    fn record_read(&mut self, key: &str) {
        if let Some(inst) = &self.current_render {
            self.deps
                .entry(key.to_string())
                .or_default()
                .insert(inst.clone());
        }
    }

    /// Every instance whose last render read `key` re-renders (§9.4:
    /// "every view that read a changed state property").
    fn dirty_readers(&mut self, key: &str) {
        if let Some(readers) = self.deps.get(key) {
            for r in readers.clone() {
                if !self.dirty_instances.contains(&r) {
                    self.dirty_instances.push(r);
                }
            }
        }
    }

    /// Begin dependency tracking for one instance's render: its old edges
    /// drop so a render that stopped reading a key stops depending on it.
    pub fn begin_render(&mut self, id: &str) {
        for readers in self.deps.values_mut() {
            readers.remove(id);
        }
        self.current_render = Some(id.to_string());
    }

    pub fn end_render(&mut self) {
        self.current_render = None;
    }

    /// One structured entry (§9.9): timestamp, level, message, data, and
    /// the source location — the PART plus position, because in Ashlar a
    /// part's name is its address; files are not.
    fn emit_log(
        &mut self,
        level: &str,
        msg: &str,
        payload: Option<&V>,
        part: &str,
        span: crate::tokens::Span,
    ) {
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        let mut line = format!("{{\"ts\":{},\"level\":", ts);
        crate::diag::push_json_str(&mut line, level);
        line.push_str(",\"message\":");
        crate::diag::push_json_str(&mut line, msg);
        if let Some(p) = payload {
            line.push_str(",\"data\":");
            line.push_str(&to_json(p));
        }
        line.push_str(",\"loc\":{\"part\":");
        crate::diag::push_json_str(&mut line, part);
        line.push_str(&format!(
            ",\"line\":{},\"col\":{}}}",
            span.start.line, span.start.col
        ));
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
                // Chain properties run every layer when CALLED (§4): a
                // callee resolving to a `stack`/`pipe` prop dispatches to
                // the chain runners — a chain has no single value to eval.
                if let Expr::NameRef(segs) = &callee.expr {
                    if env.get(&segs[0]).is_none() {
                        if let Some((part, prop, kind, reverse)) = self.chain_target(env, segs) {
                            let mut vals = Vec::new();
                            for a in args {
                                vals.push(self.eval(env, a)?);
                            }
                            return match kind {
                                crate::ast::MergeKind::Pipe => {
                                    let first = vals.into_iter().next().unwrap_or(V::None);
                                    self.run_pipe(&part, &prop, reverse, first)
                                }
                                _ => {
                                    // A stack call returns the part (§4).
                                    // A bare call inside an instance runs
                                    // the instance's own stack.
                                    match (&env.instance, segs.len()) {
                                        (Some(id), 1) => {
                                            let id = id.clone();
                                            self.run_instance_stack(&id, &prop, reverse)?;
                                        }
                                        _ => self.run_stack(&part, &prop, reverse)?,
                                    }
                                    Ok(V::Part(part))
                                }
                            };
                        }
                    }
                }
                // std builtins and log.* dispatch by name before value eval.
                if let Expr::NameRef(segs) = &callee.expr {
                    if segs.len() == 2 && segs[0] == "log" && env.get("log").is_none() {
                        let mut vals = Vec::new();
                        for a in args {
                            vals.push(self.eval(env, a)?);
                        }
                        let msg = vals.first().map(to_text).unwrap_or_default();
                        let part = env.part.clone();
                        self.emit_log(&segs[1], &msg, vals.get(1), &part, callee.span);
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
                instance: env.instance.clone(),
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
        // Longest-prefix: foreign full names, then full part names.
        for k in (2..=segs.len()).rev() {
            let prefix = segs[..k].join(".");
            if self.program.foreigns.contains_key(&prefix) && k == segs.len() {
                return Ok(V::ForeignFn(prefix));
            }
            if self.composed.contains_key(&prefix) {
                let mut v = V::Part(prefix);
                for name in &segs[k..] {
                    v = self.field(v, name)?;
                }
                return Ok(v);
            }
        }
        let n = &segs[0];
        // Instance context first: fields and per-instance state (§9.4).
        let instance_hit = env.instance.as_ref().and_then(|id| {
            self.instances.get(id).and_then(|inst| {
                inst.state
                    .get(n)
                    .map(|v| (v.clone(), true))
                    .or_else(|| inst.fields.get(n).map(|v| (v.clone(), false)))
            })
        });
        let instance_hit = match instance_hit {
            Some((v, is_state)) => {
                if is_state {
                    if let Some(id) = env.instance.clone() {
                        self.record_read(&format!("instance:{}.{}", id, n));
                    }
                }
                Some(v)
            }
            None => None,
        };
        let mut v = if let Some(local) = env.get(n) {
            local.clone()
        } else if let Some(iv) = instance_hit {
            iv
        } else if self.part_has_prop(&env.part, n) {
            let part = env.part.clone();
            self.field(V::Part(part), n)?
        } else if let Some(full) = self.unique_bare_part(n) {
            V::Part(full)
        } else if let Some(full) = self.unique_bare_foreign(n) {
            V::ForeignFn(full)
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

    /// Resolve a call target to a chain (`stack`/`pipe`) property:
    /// `prop(...)` in the enclosing part, `Part.prop(...)` by unique bare
    /// name, or `full.name.Part.prop(...)` by longest part prefix.
    fn chain_target(
        &self,
        env: &Env,
        segs: &[String],
    ) -> Option<(String, String, crate::ast::MergeKind, bool)> {
        let chain_kind = |part: &str, prop: &str| -> Option<(crate::ast::MergeKind, bool)> {
            let (kind, rev) = self.composed.get(part)?.props.get(prop)?.kind?;
            matches!(kind, crate::ast::MergeKind::Stack | crate::ast::MergeKind::Pipe)
                .then_some((kind, rev))
        };
        if segs.len() == 1 {
            let (kind, rev) = chain_kind(&env.part, &segs[0])?;
            return Some((env.part.clone(), segs[0].clone(), kind, rev));
        }
        let (head, last) = segs.split_at(segs.len() - 1);
        let prop = &last[0];
        if head.len() == 1 {
            if let Some(full) = self.unique_bare_part(&head[0]) {
                if let Some((kind, rev)) = chain_kind(&full, prop) {
                    return Some((full, prop.clone(), kind, rev));
                }
            }
        }
        let prefix = head.join(".");
        if self.composed.contains_key(&prefix) {
            if let Some((kind, rev)) = chain_kind(&prefix, prop) {
                return Some((prefix, prop.clone(), kind, rev));
            }
        }
        None
    }

    fn unique_bare_foreign(&self, bare: &str) -> Option<String> {
        let mut hits = self
            .program
            .foreigns
            .keys()
            .filter(|f| f.rsplit('.').next() == Some(bare));
        let first = hits.next()?;
        if hits.next().is_some() {
            None
        } else {
            Some(first.clone())
        }
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
                if let Some(v) = self.state.values.get(&key).cloned() {
                    self.record_read(&key);
                    return Ok(v);
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

    /// Call a function value in its own captured instance context.
    pub fn call(&mut self, f: V, args: Vec<V>) -> R {
        let instance = match &f {
            V::Fn(fv) => fv.instance.clone(),
            _ => None,
        };
        self.call_with_instance(f, args, instance)
    }

    /// Call a function value, overriding the instance context (event
    /// dispatch runs a handler in the instance that rendered it).
    pub fn call_with_instance(&mut self, f: V, args: Vec<V>, instance: Option<String>) -> R {
        if let V::ForeignFn(full) = &f {
            let full = full.clone();
            return self.invoke_foreign(&full, args);
        }
        let V::Fn(fv) = f else {
            return Err(Fault::new(format!(
                "internal: {} is not callable.",
                kind_of(&f)
            )));
        };
        let mut env = Env {
            part: fv.part.clone(),
            instance: instance.or_else(|| fv.instance.clone()),
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
                // In an instance context a state property is per-instance;
                // the write marks the instance for re-render (§9.4).
                if let Some(id) = env.instance.clone() {
                    let is_instance_state = self
                        .instances
                        .get(&id)
                        .map(|i| i.state.contains_key(name))
                        .unwrap_or(false);
                    if is_instance_state {
                        if let Some(inst) = self.instances.get_mut(&id) {
                            inst.state.insert(name.clone(), v);
                        }
                        if !self.dirty_instances.contains(&id) {
                            self.dirty_instances.push(id.clone());
                        }
                        self.dirty_readers(&format!("instance:{}.{}", id, name));
                        return Ok(Flow::Normal);
                    }
                }
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
                // fail(message) -> 500, or fail(status, message) (§9.9).
                let first = arg(&mut args, 0);
                let (status, message) = match first {
                    V::Number(n) => (n as u16, to_text(&arg(&mut args, 1))),
                    other => (500, to_text(&other)),
                };
                Err(Fault { status, message })
            }
            "redirect" => {
                let mut m = BTreeMap::new();
                m.insert("__redirect".to_string(), arg(&mut args, 0));
                Ok(V::Map(m))
            }
            "el" => {
                match arg(&mut args, 0) {
                    // el(PartName, fields, children): instantiate (§9.4).
                    V::Part(part) => {
                        let fields = match arg(&mut args, 1) {
                            V::Map(m) => m,
                            _ => BTreeMap::new(),
                        };
                        let id = self.new_instance(&part, fields)?;
                        let mut m = BTreeMap::new();
                        m.insert("__view_instance".to_string(), V::Text(id));
                        Ok(V::Map(m))
                    }
                    tag => {
                        let mut m = BTreeMap::new();
                        m.insert("__el".to_string(), tag);
                        m.insert("attrs".to_string(), arg(&mut args, 1));
                        m.insert("children".to_string(), arg(&mut args, 2));
                        Ok(V::Map(m))
                    }
                }
            }
            "spawn" => {
                // Queued; the serve loop drains after the current request
                // completes (§9.7).
                self.spawn_queue.push(arg(&mut args, 0));
                Ok(V::None)
            }
            "signup" => {
                let (email, pw) = match (arg(&mut args, 0), arg(&mut args, 1)) {
                    (V::Text(e), V::Text(p)) => (e, p),
                    _ => return Err(Fault::new("internal: signup takes texts.".to_string())),
                };
                if self.users.contains_key(&email) {
                    return Err(Fault {
                        status: 409,
                        message: "an account with that email exists.".to_string(),
                    });
                }
                let id = self.fresh_id("u");
                let salt = self.fresh_salt();
                self.users
                    .insert(email.clone(), (id.clone(), hash_password_v2(&pw, &salt)));
                self.state.dirty = true; // accounts persist with stored state
                self.open_session(&id);
                Ok(user_value(&id, &email))
            }
            "login" => {
                let (email, pw) = match (arg(&mut args, 0), arg(&mut args, 1)) {
                    (V::Text(e), V::Text(p)) => (e, p),
                    _ => return Err(Fault::new("internal: login takes texts.".to_string())),
                };
                match self.users.get(&email).cloned() {
                    Some((id, hash)) => {
                        let (ok, needs_upgrade) = verify_password(&email, &pw, &hash);
                        if !ok {
                            return Err(Fault {
                                status: 401,
                                message: "bad credentials.".to_string(),
                            });
                        }
                        // Legacy v1 hashes upgrade transparently on the
                        // first successful login (§9.6).
                        if needs_upgrade {
                            let salt = self.fresh_salt();
                            self.users
                                .insert(email.clone(), (id.clone(), hash_password_v2(&pw, &salt)));
                            self.state.dirty = true;
                        }
                        self.open_session(&id);
                        Ok(user_value(&id, &email))
                    }
                    None => Err(Fault {
                        status: 401,
                        message: "bad credentials.".to_string(),
                    }),
                }
            }
            "logout" => {
                if let Some(tok) = self.current_session.take() {
                    self.sessions.remove(&tok);
                }
                self.pending_cookie = Some(String::new()); // clears the cookie
                Ok(V::None)
            }
            other => Err(Fault::new(format!("internal: unknown builtin `{}`.", other))),
        }
    }
}

// Raw dl bindings (glibc; libdl is part of libc on modern systems). The
// only unsafe in the codebase, confined to the §9.10 boundary.
extern "C" {
    fn dlopen(filename: *const std::os::raw::c_char, flags: std::os::raw::c_int) -> *mut std::os::raw::c_void;
    fn dlsym(handle: *mut std::os::raw::c_void, symbol: *const std::os::raw::c_char) -> *mut std::os::raw::c_void;
}
const RTLD_NOW: std::os::raw::c_int = 2;

type ForeignAbi = unsafe extern "C" fn(*const std::os::raw::c_char) -> *mut std::os::raw::c_char;

impl<'a> Evaluator<'a> {
    /// Call a foreign function (§9.10): the manifest-recorded binding is
    /// `foreign/<space>.so` under the project root; the C ABI is
    /// `char* name(const char* args_json)` — arguments as a JSON array,
    /// return as JSON, decoded and shape-checked at the call site. A
    /// mismatch is a runtime fault, exactly as the reference states.
    fn invoke_foreign(&mut self, full: &str, args: Vec<V>) -> R {
        let Some(info) = self.program.foreigns.get(full) else {
            return Err(Fault::new(format!("internal: unknown foreign `{}`.", full)));
        };
        let space = info.space.clone();
        let decl = &self.program.files[info.file_idx].ast.foreigns[info.foreign_idx];
        let name = decl.name.clone();
        let ret_shape = decl.ret.clone();
        if args.len() != decl.params.len() {
            return Err(Fault::new(format!(
                "foreign `{}` takes {} argument(s), got {}.",
                full,
                decl.params.len(),
                args.len()
            )));
        }

        let handle = match self.foreign_libs.get(&space) {
            Some(h) => *h,
            None => {
                let Some(root) = &self.foreign_root else {
                    return Err(Fault::new(format!(
                        "foreign `{}` is not bound (no project root for foreign libraries).",
                        full
                    )));
                };
                let path = root.join("foreign").join(format!("{}.so", space));
                let c_path = std::ffi::CString::new(path.to_string_lossy().as_bytes())
                    .map_err(|_| Fault::new("internal: bad library path.".to_string()))?;
                let h = unsafe { dlopen(c_path.as_ptr(), RTLD_NOW) };
                if h.is_null() {
                    return Err(Fault::new(format!(
                        "foreign library `foreign/{}.so` could not be loaded.",
                        space
                    )));
                }
                self.foreign_libs.insert(space.clone(), h as usize);
                h as usize
            }
        };

        let c_name = std::ffi::CString::new(name.as_bytes())
            .map_err(|_| Fault::new("internal: bad symbol name.".to_string()))?;
        let sym = unsafe { dlsym(handle as *mut std::os::raw::c_void, c_name.as_ptr()) };
        if sym.is_null() {
            return Err(Fault::new(format!(
                "foreign `{}` has no symbol `{}` in `foreign/{}.so`.",
                full, name, space
            )));
        }
        let f: ForeignAbi = unsafe { std::mem::transmute(sym) };

        let args_json = to_json(&V::List(args));
        let c_args = std::ffi::CString::new(args_json)
            .map_err(|_| Fault::new("internal: argument encoding.".to_string()))?;
        let out = unsafe { f(c_args.as_ptr()) };
        if out.is_null() {
            return Err(Fault::new(format!("foreign `{}` returned nothing.", full)));
        }
        let text = unsafe { std::ffi::CStr::from_ptr(out) }
            .to_string_lossy()
            .to_string();
        let value = from_json(&text).ok_or_else(|| {
            Fault::new(format!("foreign `{}` returned malformed JSON.", full))
        })?;
        if !value_fits_shape(&value, &ret_shape) {
            return Err(Fault::new(format!(
                "foreign `{}` returned a value that does not fit `{}`.",
                full,
                shape_name(&ret_shape)
            )));
        }
        Ok(value)
    }

    fn open_session(&mut self, user_id: &str) {
        let token = self.fresh_id("s");
        self.sessions.insert(token.clone(), user_id.to_string());
        self.current_session = Some(token.clone());
        self.pending_cookie = Some(token);
    }

    /// The user value for the current session, if any (`req.user`).
    pub fn session_user(&self) -> V {
        let Some(tok) = &self.current_session else { return V::None };
        let Some(uid) = self.sessions.get(tok) else { return V::None };
        for (email, (id, _)) in &self.users {
            if id == uid {
                return user_value(id, email);
            }
        }
        V::None
    }
}

fn user_value(id: &str, email: &str) -> V {
    let mut m = BTreeMap::new();
    m.insert("id".to_string(), V::Text(id.to_string()));
    m.insert("email".to_string(), V::Text(email.to_string()));
    V::Map(m)
}

/// Weak-by-construction v1 password hash (SHA-1 of email-salted input);
/// zero-dependency, documented in the roadmap as an upgrade point.
/// v1 (legacy): unsalted `sha1(email \0 pw)` hex. Still verified so
/// existing accounts keep working; upgraded to v2 on the next login.
fn hash_password_v1(email: &str, pw: &str) -> String {
    let digest = crate::http::sha1(format!("{}\u{0}{}", email, pw).as_bytes());
    digest.iter().map(|b| format!("{:02x}", b)).collect()
}

/// HMAC-SHA1 (RFC 2104) over the zero-dependency SHA-1.
fn hmac_sha1(key: &[u8], msg: &[u8]) -> [u8; 20] {
    let mut k = [0u8; 64];
    if key.len() > 64 {
        k[..20].copy_from_slice(&crate::http::sha1(key));
    } else {
        k[..key.len()].copy_from_slice(key);
    }
    let mut inner: Vec<u8> = k.iter().map(|b| b ^ 0x36).collect();
    inner.extend_from_slice(msg);
    let ih = crate::http::sha1(&inner);
    let mut outer: Vec<u8> = k.iter().map(|b| b ^ 0x5c).collect();
    outer.extend_from_slice(&ih);
    crate::http::sha1(&outer)
}

/// PBKDF2-HMAC-SHA1 (RFC 2898), single 20-byte block.
fn pbkdf2_sha1(pw: &[u8], salt: &[u8], iterations: u32) -> [u8; 20] {
    let mut block = salt.to_vec();
    block.extend_from_slice(&[0, 0, 0, 1]);
    let mut u = hmac_sha1(pw, &block);
    let mut out = u;
    for _ in 1..iterations {
        u = hmac_sha1(pw, &u);
        for (o, b) in out.iter_mut().zip(u.iter()) {
            *o ^= b;
        }
    }
    out
}

const PBKDF2_ITERATIONS: u32 = 10_000;

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

/// v2: `2$<salt hex>$<pbkdf2 hex>`, salted and iterated (§9.6).
fn hash_password_v2(pw: &str, salt: &[u8]) -> String {
    let h = pbkdf2_sha1(pw.as_bytes(), salt, PBKDF2_ITERATIONS);
    format!("2${}${}", hex(salt), hex(&h))
}

/// Verify against either format. Returns (ok, needs_upgrade).
fn verify_password(email: &str, pw: &str, stored: &str) -> (bool, bool) {
    if let Some(rest) = stored.strip_prefix("2$") {
        if let Some((salt_hex, hash_hex)) = rest.split_once('$') {
            let salt: Vec<u8> = (0..salt_hex.len())
                .step_by(2)
                .filter_map(|i| u8::from_str_radix(salt_hex.get(i..i + 2)?, 16).ok())
                .collect();
            let h = pbkdf2_sha1(pw.as_bytes(), &salt, PBKDF2_ITERATIONS);
            return (hex(&h) == hash_hex, false);
        }
        return (false, false);
    }
    (hash_password_v1(email, pw) == stored, true)
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
        V::ForeignFn(_) => "a foreign function",
    }
}

/// Runtime shape check for values crossing the foreign boundary (§9.10).
/// `data` admits every JSON-decodable value; part shapes check as maps.
fn value_fits_shape(v: &V, sh: &crate::ast::SShape) -> bool {
    use crate::ast::Shape;
    match (&sh.shape, v) {
        (Shape::Data, _) => !matches!(v, V::Fn(_) | V::Part(_) | V::ForeignFn(_)),
        (Shape::Text, V::Text(_)) => true,
        (Shape::Number, V::Number(_)) => true,
        (Shape::Bool, V::Bool(_)) => true,
        (Shape::Opt(_), V::None) => true,
        (Shape::Opt(i), v) => value_fits_shape(v, i),
        (Shape::List(i), V::List(xs)) => xs.iter().all(|x| value_fits_shape(x, i)),
        (Shape::Map(i), V::Map(m)) => m.values().all(|x| value_fits_shape(x, i)),
        (Shape::Part(_), V::Map(_)) => true, // field-level checking is the checker's future work
        (Shape::Fn(..), _) => false,         // functions cannot cross the boundary
        _ => false,
    }
}

fn shape_name(sh: &crate::ast::SShape) -> String {
    use crate::ast::Shape;
    match &sh.shape {
        Shape::Text => "text".into(),
        Shape::Number => "number".into(),
        Shape::Bool => "bool".into(),
        Shape::Data => "data".into(),
        Shape::List(i) => format!("[{}]", shape_name(i)),
        Shape::Map(i) => format!("{{text: {}}}", shape_name(i)),
        Shape::Opt(i) => format!("{}?", shape_name(i)),
        Shape::Part(n) => n.join("."),
        Shape::Fn(..) => "a function shape".into(),
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
        V::Fn(_) | V::ForeignFn(_) => "<function>".to_string(),
        V::Part(p) => p.clone(),
    }
}

/// Local scope: the enclosing part, the view instance (if any), and
/// lexical frames.
pub struct Env {
    part: String,
    instance: Option<String>,
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
        V::Fn(_) | V::ForeignFn(_) => "null".to_string(),
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
    fn calling_a_chain_property_runs_every_layer() {
        // §4: "Calling the property runs every layer's function in
        // composition order" — through the ORDINARY call syntax, layered
        // across spaces. The pipe threads both layers; the stack call
        // merges onto state and returns the part.
        let r = check_sources(vec![
            (
                "a.ash".to_string(),
                "space a\n\npart P {\n  state hits: number = 0\n  boot stack = () => {\n    return { hits: hits + 1 }\n  }\n  shape pipe = (t: text) => t + \"-base\"\n}\n\npart caller {\n  go = () => P.shape(\"x\")\n  kick = () => P.boot()\n  peek = () => P.hits\n}\n"
                    .to_string(),
            ),
            (
                "b.ash".to_string(),
                "space b\nuse a\n\npart a.P {\n  boot stack = () => {\n    return { hits: hits + 1 }\n  }\n  shape pipe = (t: text) => t + \"-layer\"\n}\n"
                    .to_string(),
            ),
        ]);
        assert!(r.diags.is_empty(), "fixture must be clean: {:?}", r.diags);
        let mut ev = Evaluator::new(&r.program, &r.composed);
        assert_eq!(
            ev.call_prop("a.caller", "go", vec![]).unwrap(),
            V::Text("x-base-layer".to_string()),
            "both pipe layers must run, base first"
        );
        assert_eq!(
            ev.call_prop("a.caller", "kick", vec![]).unwrap(),
            V::Part("a.P".to_string()),
            "a stack call returns the part"
        );
        assert_eq!(
            ev.call_prop("a.caller", "peek", vec![]).unwrap(),
            V::Number(2.0),
            "both stack layers must have merged onto state"
        );
    }

    #[test]
    fn log_entries_carry_ts_message_data_and_location() {
        // §9.9: structured entries with timestamp, level, message, data,
        // and the source location (part + position).
        let r = check_sources(vec![(
            "t.ash".to_string(),
            "space a\n\npart W {\n  go = () => {\n    log.warn(\"slow\", { ms: 12 })\n  }\n}\n"
                .to_string(),
        )]);
        assert!(r.diags.is_empty(), "{:?}", r.diags);
        let mut ev = Evaluator::new(&r.program, &r.composed);
        ev.call_prop("a.W", "go", vec![]).unwrap();
        let line = ev.log.first().expect("one log entry").clone();
        for needle in [
            "\"ts\":",
            "\"level\":\"warn\"",
            "\"message\":\"slow\"",
            "\"data\":{\"ms\":12}",
            "\"loc\":{\"part\":\"a.W\"",
        ] {
            assert!(line.contains(needle), "missing {} in {}", needle, line);
        }
    }

    #[test]
    fn fail_takes_one_or_two_arguments() {
        // §9.9: fail(message) is a 500; fail(status, message) is exact.
        let src = "space a\n\npart W {\n  one = () => fail(\"nope\")\n  two = () => fail(404, \"gone\")\n}\n";
        let f = eval_prop(src, "a.W", "one", vec![]).unwrap_err();
        assert_eq!((f.status, f.message.as_str()), (500, "nope"));
        let f = eval_prop(src, "a.W", "two", vec![]).unwrap_err();
        assert_eq!((f.status, f.message.as_str()), (404, "gone"));
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
