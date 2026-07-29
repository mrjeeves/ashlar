//! The foreign boundary (§9.10, ADR-0017).
//!
//! A `foreign` declaration names a CAPABILITY — a name and a shape. How that
//! capability is reached is a deployment fact, resolved here and never
//! written in source (B5). Three transports carry the same JSON envelope:
//!
//! - `native` — `dlopen` a shared library, C ABI `char* f(const char*)`;
//! - `worker` — a long-lived co-process speaking JSON Lines on stdin/stdout;
//! - `http`   — POST the envelope to a URL (plaintext; TLS terminates at a
//!   proxy, ADR-0013).
//!
//! A space with no binding resolves by the DERIVATION rule — the native
//! library at `foreign/<space>` — which the binding file overrides, exactly
//! as `--port` overrides `port`. This module owns the only `unsafe` in the
//! workspace: the dlopen/dlsym pair below.

use crate::diag::{Diag, Level, E001_UNKNOWN_NAME};
use crate::eval::{from_json, to_json, V};
use crate::tokens::{Pos, Span};
use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};

// -- the binding file -------------------------------------------------------

/// How one space's capabilities are reached.
#[derive(Debug, Clone, PartialEq)]
pub enum Via {
    /// A shared library. `library` overrides the derived path; `symbols`
    /// maps an Ashlar name to a differently-spelled export.
    Native {
        library: Option<String>,
        symbols: BTreeMap<String, String>,
    },
    /// A co-process: argv, run from the project root.
    Worker { run: Vec<String> },
    /// An ordinary program, run once per call. `run` is the command; `args`
    /// maps an Ashlar name to the argv items that select it, defaulting to
    /// the name itself — the same relationship `symbols` has to an export.
    Command {
        run: Vec<String>,
        args: BTreeMap<String, Vec<String>>,
    },
    /// A service that speaks the same envelope over HTTP.
    Http { url: String },
}

impl Via {
    /// The derived default: a native library at `foreign/<space>`.
    fn derived() -> Via {
        Via::Native {
            library: None,
            symbols: BTreeMap::new(),
        }
    }

    /// The derived default for one space. One name derives to a co-process
    /// instead of a library, because the capability it names belongs to the
    /// machine rather than to the project (see [`derived_worker`]).
    pub fn derived_for(space: &str) -> Via {
        match derived_worker(space) {
            Some(run) => Via::Worker { run },
            None => Via::derived(),
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Via::Native { .. } => "native",
            Via::Worker { .. } => "worker",
            Via::Command { .. } => "command",
            Via::Http { .. } => "http",
        }
    }
}

/// The one space naming the mesh: who else is on the private network this
/// machine joined, and the sites they serve.
pub const MESH_SPACE: &str = "mesh";

/// The space whose derived default is a co-process rather than a native
/// library — and the command is this toolchain, in worker mode.
///
/// The derivation rule (ADR-0017) answers "where does this capability live"
/// with a path inside the project, which is right for a capability the project
/// supplies and wrong for one the machine already runs. The mesh node is the
/// second kind, installed once and shared by everything on the box, exactly
/// like the proxy that terminates TLS in front of the origin (ADR-0013).
///
/// What it must NOT do is make the mesh ship an Ashlar-shaped adapter. That is
/// the failure this whole ADR exists to end: a boundary that only works once
/// the foreign system has been re-authored for us is not a boundary. So the
/// adapter is ours — `ashlar mesh worker` speaks the one control socket that
/// node already exposes to its own clients (§9.10) — and the mechanism is the
/// ordinary worker transport, so a `foreign.json` entry still overrides it,
/// `check` still reports an unknown key, and the manifest still records
/// whichever won.
pub fn derived_worker(space: &str) -> Option<Vec<String>> {
    match space {
        MESH_SPACE => Some(vec![
            "ashlar".to_string(),
            "mesh".to_string(),
            "worker".to_string(),
        ]),
        _ => None,
    }
}

/// Whether an argv is this toolchain answering its own derived default. A
/// worker named `ashlar` is only findable when the toolchain is on PATH, and
/// it very often is not — a `cargo run` build, a binary in a release
/// directory. The CLI therefore publishes its own path as `ASHLAR_SELF`, and
/// spawning uses it for exactly this argv and nothing else, so the derived
/// default works wherever `ashlar` was started from without turning every
/// worker into a guess about what is running.
pub fn is_self_worker(run: &[String]) -> bool {
    run.len() == 3 && run[0] == "ashlar" && run[1] == "mesh" && run[2] == "worker"
}

/// Where the binding file lives: `ASHLAR_FOREIGN` if set, else
/// `<root>/foreign.json`. Absent is not an error — every space then takes
/// the derived default.
pub fn binding_path(root: &Path) -> PathBuf {
    match std::env::var("ASHLAR_FOREIGN") {
        Ok(p) if !p.is_empty() => PathBuf::from(p),
        _ => root.join("foreign.json"),
    }
}

/// The `run` array a `worker` and a `command` both need.
fn argv(fields: &BTreeMap<String, V>, space: &str) -> Result<Vec<String>, String> {
    let Some(V::List(items)) = fields.get("run") else {
        return Err(format!(
            "binding for `{}` needs `run`, a list like [\"sqlite3\", \"app.db\"].",
            space
        ));
    };
    let mut run = Vec::new();
    for it in items {
        let V::Text(a) = it else {
            return Err(format!(
                "binding for `{}`: every `run` entry must be a text.",
                space
            ));
        };
        run.push(a.clone());
    }
    if run.is_empty() {
        return Err(format!("binding for `{}`: `run` must name a command.", space));
    }
    Ok(run)
}

/// Parse a binding file. A malformed one is loud: a program whose deployment
/// facts are unreadable must not silently fall back to the derived path.
pub fn parse_bindings(text: &str) -> Result<BTreeMap<String, Via>, String> {
    let Some(V::Map(spaces)) = from_json(text) else {
        return Err("the foreign binding file must be a JSON object of space -> binding.".to_string());
    };
    let mut out = BTreeMap::new();
    for (space, entry) in spaces {
        let V::Map(fields) = entry else {
            return Err(format!("binding for `{}` must be an object.", space));
        };
        let via = match fields.get("via") {
            Some(V::Text(v)) => v.clone(),
            Some(_) => return Err(format!("binding for `{}`: `via` must be a text.", space)),
            None => return Err(format!(
                "binding for `{}` has no `via`; use \"native\", \"worker\", \"command\", or \"http\".",
                space
            )),
        };
        let bound = match via.as_str() {
            "native" => {
                let library = match fields.get("library") {
                    Some(V::Text(p)) => Some(p.clone()),
                    Some(_) => return Err(format!("binding for `{}`: `library` must be a text.", space)),
                    None => None,
                };
                let mut symbols = BTreeMap::new();
                match fields.get("symbols") {
                    Some(V::Map(m)) => {
                        for (k, v) in m {
                            let V::Text(sym) = v else {
                                return Err(format!(
                                    "binding for `{}`: symbol `{}` must map to a text.",
                                    space, k
                                ));
                            };
                            symbols.insert(k.clone(), sym.clone());
                        }
                    }
                    Some(_) => return Err(format!("binding for `{}`: `symbols` must be an object.", space)),
                    None => {}
                }
                Via::Native { library, symbols }
            }
            "worker" => Via::Worker {
                run: argv(&fields, &space)?,
            },
            "command" => {
                let run = argv(&fields, &space)?;
                // Which argv items select this name. Absent means the name
                // itself, so `git` + `status` is the whole binding; present
                // and empty means the program takes the arguments alone,
                // which is how a tool that is one command (`sqlite3`) binds.
                let mut args: BTreeMap<String, Vec<String>> = BTreeMap::new();
                match fields.get("args") {
                    Some(V::Map(m)) => {
                        for (k, v) in m {
                            let V::List(items) = v else {
                                return Err(format!(
                                    "binding for `{}`: `args` for `{}` must be a list of text.",
                                    space, k
                                ));
                            };
                            let mut fixed = Vec::new();
                            for it in items {
                                let V::Text(a) = it else {
                                    return Err(format!(
                                        "binding for `{}`: `args` for `{}` must be a list of text.",
                                        space, k
                                    ));
                                };
                                fixed.push(a.clone());
                            }
                            args.insert(k.clone(), fixed);
                        }
                    }
                    Some(_) => {
                        return Err(format!("binding for `{}`: `args` must be an object.", space))
                    }
                    None => {}
                }
                Via::Command { run, args }
            }
            "http" => {
                let Some(V::Text(url)) = fields.get("url") else {
                    return Err(format!("binding for `{}`: an http binding needs `url`.", space));
                };
                if !url.starts_with("http://") {
                    return Err(format!(
                        "binding for `{}`: `url` must begin with `http://` — the runtime is an origin and does not speak TLS (ADR-0013); put a proxy in front or use a co-located service.",
                        space
                    ));
                }
                Via::Http { url: url.clone() }
            }
            other => {
                return Err(format!(
                    "binding for `{}`: unknown `via` \"{}\"; use \"native\", \"worker\", \"command\", or \"http\".",
                    space, other
                ))
            }
        };
        out.insert(space, bound);
    }
    Ok(out)
}

// -- the binding as a name-bearing fact -------------------------------------
//
// A binding is keyed by SPACE NAME, which makes `foreign.json` the one file
// outside `.ash` that carries a name the compiler reasons about. The three
// systems that govern names therefore all have to see it: the manifest records
// what resolved (§10), `check` reports a key that names no space (B3), and a
// space rename carries the key with it (E2). The scanner below is what they
// share — depth- and string-aware, so a nested `"native"` value can never be
// mistaken for a space key.

/// Char offsets of a top-level key's opening and closing quote.
fn key_offsets(text: &str, key: &str) -> Option<(usize, usize)> {
    let c: Vec<char> = text.chars().collect();
    let mut depth = 0usize;
    let mut i = 0usize;
    while i < c.len() {
        match c[i] {
            '{' | '[' => {
                depth += 1;
                i += 1;
            }
            '}' | ']' => {
                depth = depth.saturating_sub(1);
                i += 1;
            }
            '"' => {
                let open = i;
                i += 1;
                let mut content = String::new();
                while i < c.len() && c[i] != '"' {
                    if c[i] == '\\' && i + 1 < c.len() {
                        content.push(c[i]);
                        content.push(c[i + 1]);
                        i += 2;
                        continue;
                    }
                    content.push(c[i]);
                    i += 1;
                }
                let close = i;
                i += 1;
                // A key is a depth-1 string followed by `:`.
                if depth == 1 && content == key {
                    let mut j = i;
                    while j < c.len() && c[j].is_whitespace() {
                        j += 1;
                    }
                    if j < c.len() && c[j] == ':' {
                        return Some((open, close));
                    }
                }
            }
            _ => i += 1,
        }
    }
    None
}

/// Rewrite one top-level space key, leaving every other byte alone so that
/// reversing a rename restores the file exactly (E4).
pub fn rename_key(text: &str, old: &str, new: &str) -> Option<String> {
    let (open, close) = key_offsets(text, old)?;
    let c: Vec<char> = text.chars().collect();
    let mut out: String = c[..=open].iter().collect();
    out.push_str(new);
    out.extend(&c[close..]);
    Some(out)
}

/// The span of a top-level key, as a diagnostic location (1-based, chars).
fn key_span(text: &str, key: &str) -> Span {
    let Some((open, close)) = key_offsets(text, key) else {
        return Span {
            start: Pos { line: 1, col: 1 },
            end: Pos { line: 1, col: 1 },
        };
    };
    let (mut line, mut col) = (1u32, 1u32);
    let mut start = Pos { line: 1, col: 1 };
    for (i, ch) in text.chars().enumerate() {
        if i == open {
            start = Pos { line, col };
        }
        if i == close {
            return Span {
                start,
                end: Pos { line, col: col + 1 },
            };
        }
        if ch == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    Span { start, end: start }
}

/// The transport and the concrete thing it names, as the manifest records it.
/// Pure: it reads the binding file and opens nothing, so writing a manifest
/// never dlopens a library or spawns a co-process.
pub fn describe(root: &Path, space: &str) -> (String, String) {
    let via = Boundary::new()
        .via(root, space)
        .unwrap_or_else(|_| Via::derived_for(space));
    let detail = match &via {
        Via::Native { library, .. } => library
            .clone()
            .unwrap_or_else(|| format!("foreign/{}", space)),
        Via::Worker { run } | Via::Command { run, .. } => run.join(" "),
        Via::Http { url } => url.clone(),
    };
    (via.label().to_string(), detail)
}

/// The derived library paths a space rename must carry, one per probed
/// extension — the derivation rule names all three (ADR-0017).
pub fn derived_library_paths(space: &str) -> Vec<String> {
    LIB_EXTS
        .iter()
        .map(|ext| format!("foreign/{}{}", space, ext))
        .collect()
}

/// What a rename must SAY when it moves a space onto or off one of the two
/// derived-worker names (E3). Nothing moves on disk — the binding is the name
/// itself — so a rename that says nothing would silently swap which transport
/// a capability is reached by, which is the failure ADR-0017 recorded for the
/// binding file's keys. `None` when neither end is one of those names.
pub fn derived_worker_radius(old: &str, new: &str) -> Option<String> {
    match (derived_worker(old), derived_worker(new)) {
        (None, None) => None,
        (Some(run), None) => Some(format!(
            "`{}` derives to the co-process `{}`; `{}` derives to a native library at `foreign/{}`. \
             Bind `{}` in foreign.json to keep the co-process.",
            old,
            run.join(" "),
            new,
            new,
            new
        )),
        (None, Some(run)) => Some(format!(
            "`{}` derives to a native library at `foreign/{}`; `{}` derives to the co-process `{}`. \
             Bind `{}` in foreign.json to keep the library.",
            old,
            old,
            new,
            run.join(" "),
            new
        )),
        (Some(old_run), Some(new_run)) => Some(format!(
            "`{}` derives to the co-process `{}`; `{}` derives to `{}`.",
            old,
            old_run.join(" "),
            new,
            new_run.join(" ")
        )),
    }
}

/// Diagnose the binding file against the program's spaces. Two conditions,
/// both of which used to pass `check` in silence and surface only when a
/// request reached the boundary:
///
/// - a key naming no space in the program — a name that resolves to nothing
///   (B3, E001), which is exactly what a space rename leaves behind;
/// - a file that cannot be parsed, since deployment facts that are
///   unreadable must not quietly become the derived default.
///
/// Deliberately silent on a key whose space exists but declares no `foreign`:
/// that binding is inert, not wrong, and guessing at intent here would be a
/// false positive.
pub fn check_bindings(root: &Path, spaces: &[String]) -> Vec<Diag> {
    let path = binding_path(root);
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };
    let file = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "foreign.json".to_string());
    let whole = Span {
        start: Pos { line: 1, col: 1 },
        end: Pos { line: 1, col: 1 },
    };
    let bindings = match parse_bindings(&text) {
        Ok(b) => b,
        Err(e) => {
            return vec![Diag::new(
                E001_UNKNOWN_NAME,
                Level::Error,
                &file,
                whole,
                format!("the foreign binding file is unreadable: {}", e),
            )
            .with_fix(
                "Correct the binding file, or delete it to take the derived default (the native library at `foreign/<space>`).".to_string(),
                vec![],
            )]
        }
    };
    let mut out = Vec::new();
    for space in bindings.keys() {
        if spaces.iter().any(|s| s == space) {
            continue;
        }
        let near = nearest(space, spaces);
        let note = match &near {
            Some(n) => format!(
                "Rename the key to `{}`, or delete the binding if that space is gone.",
                n
            ),
            None => "Delete the binding, or add a file declaring that space.".to_string(),
        };
        out.push(
            Diag::new(
                E001_UNKNOWN_NAME,
                Level::Error,
                &file,
                key_span(&text, space),
                format!("foreign binding `{}` names no space in this program.", space),
            )
            .with_fix(note, vec![]),
        );
    }
    out
}

/// The closest space name by edit distance, when one is close enough to name
/// in a correction rather than guess at.
fn nearest(needle: &str, hay: &[String]) -> Option<String> {
    let mut best: Option<(usize, &String)> = None;
    for h in hay {
        let d = distance(needle, h);
        if d <= (needle.len() / 2).max(2) && best.map(|(bd, _)| d < bd).unwrap_or(true) {
            best = Some((d, h));
        }
    }
    best.map(|(_, h)| h.clone())
}

/// Levenshtein distance, two rows.
fn distance(a: &str, b: &str) -> usize {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur = vec![0usize; b.len() + 1];
    for i in 1..=a.len() {
        cur[0] = i;
        for j in 1..=b.len() {
            let sub = prev[j - 1] + usize::from(a[i - 1] != b[j - 1]);
            cur[j] = sub.min(prev[j] + 1).min(cur[j - 1] + 1);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b.len()]
}

// -- the runtime boundary ---------------------------------------------------

/// Live foreign state: resolved bindings, open libraries, running workers.
pub struct Boundary {
    bindings: Option<BTreeMap<String, Via>>,
    libs: BTreeMap<String, usize>,
    workers: BTreeMap<String, Worker>,
    changed: Changed,
}

/// Collections a worker said changed while nobody was asking: `(space, shape)`
/// pairs, drained by the server loop.
///
/// This is the boundary's ONE unsolicited path. A worker answers calls, and
/// may also volunteer `{"changed": "<Shape>"}` at any moment; the runtime
/// marks that collection and every view that read it re-renders (§9.10). A
/// co-process that watches something — a mesh roster, a table, a directory —
/// therefore pushes, and a page follows it in the time the server loop takes
/// to come round, rather than in the time some schedule was guessed at.
pub type Changed = std::sync::Arc<std::sync::Mutex<Vec<(String, String)>>>;

struct Worker {
    child: Child,
    stdin: ChildStdin,
    /// Answers, in order, from the reader thread. The thread exists so a
    /// pushed line is seen when it is SENT rather than when the next call
    /// happens to read the pipe — which is the whole difference between a
    /// roster that is live and one that is three seconds stale.
    answers: std::sync::mpsc::Receiver<Result<String, String>>,
}

impl Default for Boundary {
    fn default() -> Self {
        Boundary::new()
    }
}

impl Boundary {
    pub fn new() -> Boundary {
        Boundary {
            bindings: None,
            libs: BTreeMap::new(),
            workers: BTreeMap::new(),
            changed: Changed::default(),
        }
    }

    /// Collections pushed since this was last asked. The server loop drains
    /// it every turn and dirties each one's readers.
    pub fn take_changed(&self) -> Vec<(String, String)> {
        match self.changed.lock() {
            Ok(mut q) => std::mem::take(&mut *q),
            Err(_) => Vec::new(),
        }
    }

    /// Load the binding file once. Absent is fine (everything derives);
    /// unreadable or malformed is an error the caller reports.
    fn bindings(&mut self, root: &Path) -> Result<&BTreeMap<String, Via>, String> {
        if self.bindings.is_none() {
            let path = binding_path(root);
            let loaded = match std::fs::read_to_string(&path) {
                Ok(text) => parse_bindings(&text)
                    .map_err(|e| format!("{}: {}", path.display(), e))?,
                Err(_) => BTreeMap::new(),
            };
            self.bindings = Some(loaded);
        }
        Ok(self.bindings.as_ref().expect("just loaded"))
    }

    /// How this space is reached (the derived default when unbound).
    pub fn via(&mut self, root: &Path, space: &str) -> Result<Via, String> {
        Ok(self
            .bindings(root)?
            .get(space)
            .cloned()
            .unwrap_or_else(|| Via::derived_for(space)))
    }

    /// Call `space.name` with `args`, returning the decoded result. The
    /// caller shape-checks it; every transport speaks the same envelope.
    pub fn call(
        &mut self,
        root: &Path,
        space: &str,
        name: &str,
        args: Vec<V>,
    ) -> Result<V, String> {
        match self.via(root, space)? {
            Via::Native { library, symbols } => {
                let symbol = symbols.get(name).cloned().unwrap_or_else(|| name.to_string());
                self.call_native(root, space, &library, &symbol, args)
            }
            Via::Worker { run } => self.call_worker(root, space, &run, name, args),
            Via::Command { run, args: which } => call_command(root, &run, &which, name, args),
            Via::Http { url } => call_http(&url, name, args),
        }
    }

    // -- native -------------------------------------------------------------

    /// Resolve and open this space's library, caching the handle. An explicit
    /// `library` wins; otherwise the derived path is probed across platform
    /// extensions.
    fn native_handle(
        &mut self,
        root: &Path,
        space: &str,
        library: &Option<String>,
    ) -> Result<usize, String> {
        if let Some(h) = self.libs.get(space) {
            return Ok(*h);
        }
        let mut tried: Vec<PathBuf> = Vec::new();
        match library {
            Some(p) => {
                let path = Path::new(p);
                tried.push(if path.is_absolute() { path.to_path_buf() } else { root.join(path) });
            }
            None => {
                for ext in LIB_EXTS {
                    tried.push(root.join("foreign").join(format!("{}{}", space, ext)));
                }
            }
        }
        for path in &tried {
            if !path.is_file() {
                continue;
            }
            match open_library(path) {
                Ok(h) => {
                    self.libs.insert(space.to_string(), h);
                    return Ok(h);
                }
                Err(e) => return Err(e),
            }
        }
        let source = root.join("foreign").join(format!("{}.rs", space));
        Err(native_failure(
            space,
            &tried,
            if source.is_file() { Some(&source) } else { None },
        ))
    }

    fn call_native(
        &mut self,
        root: &Path,
        space: &str,
        library: &Option<String>,
        symbol: &str,
        args: Vec<V>,
    ) -> Result<V, String> {
        let handle = self.native_handle(root, space, library)?;
        let sym = lookup(handle, symbol).ok_or_else(|| {
            format!(
                "foreign library for `{}` exports no symbol `{}`.",
                space, symbol
            )
        })?;
        let f: ForeignAbi = unsafe { std::mem::transmute(sym) };

        let args_json = to_json(&V::List(args));
        let c_args = std::ffi::CString::new(args_json)
            .map_err(|_| "internal: argument encoding.".to_string())?;
        let out = unsafe { f(c_args.as_ptr()) };
        if out.is_null() {
            return Err(format!("foreign `{}.{}` returned nothing.", space, symbol));
        }
        let text = unsafe { std::ffi::CStr::from_ptr(out) }
            .to_string_lossy()
            .to_string();
        // Ownership (ADR-0017): a library may export `ashlar_free` to take its
        // buffer back. Without it the buffer is the library's business, and
        // the runtime cannot free what it did not allocate.
        if let Some(freer) = lookup(handle, "ashlar_free") {
            let free_fn: ForeignFree = unsafe { std::mem::transmute(freer) };
            unsafe { free_fn(out) };
        }
        decode(&text, space, symbol)
    }

    // -- worker -------------------------------------------------------------

    fn call_worker(
        &mut self,
        root: &Path,
        space: &str,
        run: &[String],
        name: &str,
        args: Vec<V>,
    ) -> Result<V, String> {
        if !self.workers.contains_key(space) {
            let w = spawn_worker(root, run, space, self.changed.clone()).map_err(|e| {
                format!(
                    "foreign worker for `{}` could not start `{}`: {}. Name the program this \
                     machine actually has in `foreign.json` — how a capability is reached is a \
                     deployment fact, not source (§9.10).",
                    space, run[0], e
                )
            })?;
            self.workers.insert(space.to_string(), w);
        }
        let request = request_line(name, args);
        let result = {
            let w = self.workers.get_mut(space).expect("just inserted");
            w.exchange(&request)
        };
        match result {
            Ok(line) => decode(&line, space, name),
            Err(e) => {
                // The worker died or broke protocol. Reap it so the NEXT call
                // starts a fresh one — lifecycle, not failover (ADR-0017) —
                // and report what happened TO it. "It closed its output
                // without answering" is true and useless: the operator needs
                // to know WHICH program ended and how. The common cause is a
                // program that is not the one the binding meant — a Windows
                // `python3` shim that prints to stderr and exits, say — and
                // its exit status is the only thing that says so.
                let ended = match self.workers.remove(space) {
                    Some(mut w) => match settle(&mut w.child) {
                        Some(status) => format!(" — `{}` {}", run.join(" "), status),
                        None => format!(" — `{}` closed it while still running", run.join(" ")),
                    },
                    None => String::new(),
                };
                Err(format!(
                    "foreign worker for `{}` failed: {}{}. Whatever it printed is in this \
                     server's stderr; `ashlar foreign check` proves the binding before a \
                     request finds out.",
                    space, e, ended
                ))
            }
        }
    }

    /// Stop every running worker (server shutdown).
    pub fn shutdown(&mut self) {
        for (_, mut w) in std::mem::take(&mut self.workers) {
            let _ = w.child.kill();
            let _ = w.child.wait();
        }
    }
}

impl Drop for Boundary {
    fn drop(&mut self) {
        self.shutdown();
    }
}

impl Worker {
    /// One request, one response line. Any I/O failure is the caller's cue to
    /// reap this worker. Pushed lines never arrive here — the reader thread
    /// has already put them where the server loop will find them.
    fn exchange(&mut self, request: &str) -> Result<String, String> {
        self.stdin
            .write_all(request.as_bytes())
            .and_then(|_| self.stdin.write_all(b"\n"))
            .and_then(|_| self.stdin.flush())
            .map_err(|e| format!("could not write to it ({})", e))?;
        match self.answers.recv() {
            Ok(answer) => answer,
            Err(_) => Err("it closed its output without answering".to_string()),
        }
    }
}

/// The collection a pushed line names, or `None` for an ordinary answer.
///
/// A worker that never pushes is unaffected: `{"ok":…}` and `{"error":…}` are
/// answers, and so is anything else, which is what keeps the check in
/// `probe_worker` honest about a co-process that speaks nonsense.
pub fn pushed_collection(line: &str) -> Option<String> {
    match from_json(line.trim()) {
        Some(V::Map(m)) if m.len() == 1 => match m.get("changed") {
            Some(V::Text(shape)) if !shape.trim().is_empty() => Some(shape.trim().to_string()),
            _ => None,
        },
        _ => None,
    }
}

/// Why a `native` binding could not be reached, said so it can be acted on.
///
/// An EMPTY `tried` means there is nowhere to look: `LIB_EXTS` is empty off
/// unix, because `native` needs a POSIX dynamic loader and that platform has
/// none. This used to fall through to the "no library" sentence with an empty
/// list — "Looked for ." — and then advise building a shim, on the one
/// platform where no shim could ever be loaded. A correction that cannot be
/// carried out is not a correction.
///
/// `source` is the shim's own source where it is sitting next to the library
/// that is missing: "build the shim" is a shrug until it says WHICH one.
pub fn native_failure(space: &str, tried: &[PathBuf], source: Option<&Path>) -> String {
    if tried.is_empty() {
        return format!(
            "foreign space `{}` is bound to the `native` transport, which needs a POSIX dynamic \
             loader; this platform has none, so there is no library path that would work. Bind \
             it to `worker`, `command`, or `http` in `foreign.json` — every transport carries \
             the same envelope, so nothing in the program itself changes.",
            space
        );
    }
    let where_ = tried
        .iter()
        .map(|p| format!("`{}`", p.display()))
        .collect::<Vec<_>>()
        .join(", ");
    match source {
        Some(src) => format!(
            "foreign space `{}` has no library. Looked for {}. `{}` is there but not built: \
             compile it to a `cdylib` at `{}` (it may need link flags of its own), or bind the \
             space to a `worker` or `command` transport in `foreign.json`.",
            space,
            where_,
            src.display(),
            tried[0].display()
        ),
        None => format!(
            "foreign space `{}` has no library. Looked for {}. Build the shim, or bind the \
             space in `foreign.json`.",
            space, where_
        ),
    }
}

/// What became of a worker whose output just ended. Bounded: a co-process that
/// closed stdout and kept running must not hang the request that noticed, so
/// this gives up after a tenth of a second and says so rather than waiting.
/// Only ever called on the failure path.
fn settle(child: &mut std::process::Child) -> Option<std::process::ExitStatus> {
    for _ in 0..10 {
        match child.try_wait() {
            Ok(Some(status)) => return Some(status),
            Ok(None) => std::thread::sleep(std::time::Duration::from_millis(10)),
            Err(_) => return None,
        }
    }
    let _ = child.kill();
    let _ = child.wait();
    None
}

fn spawn_worker(
    root: &Path,
    run: &[String],
    space: &str,
    changed: Changed,
) -> Result<Worker, std::io::Error> {
    // The derived mesh binding names this toolchain, which is not always on
    // PATH under the name `ashlar`, so the CLI records where it lives and the
    // fallback uses that. It is NOT `current_exe`: this library is embedded in
    // other programs (its own test harness, for one), and spawning whatever
    // binary happens to be running as a mesh worker answers with that
    // program's output — which arrives here as "returned malformed JSON",
    // three layers from the cause.
    let program: PathBuf = match (is_self_worker(run), std::env::var("ASHLAR_SELF")) {
        (true, Ok(path)) if !path.is_empty() => PathBuf::from(path),
        _ => PathBuf::from(&run[0]),
    };
    let mut child = Command::new(program)
        .args(&run[1..])
        .current_dir(root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        // stderr is inherited on purpose: a worker's own logging belongs in
        // the server's stderr where an operator already looks.
        .stderr(Stdio::inherit())
        .spawn()?;
    let stdin = child.stdin.take().expect("piped");
    let stdout = child.stdout.take().expect("piped");
    // One thread per worker, reading its output for as long as it lives. A
    // pushed line is routed to the changed queue the moment it arrives; an
    // answer goes to whoever is waiting. Reading only inside `exchange` would
    // leave a push sitting in the pipe until the next call, which is a poll
    // wearing a push's clothes.
    let (tx, answers) = std::sync::mpsc::channel();
    let space = space.to_string();
    std::thread::spawn(move || {
        let mut out = BufReader::new(stdout);
        loop {
            let mut line = String::new();
            match out.read_line(&mut line) {
                Ok(0) => break,
                Ok(_) => {
                    if let Some(shape) = pushed_collection(&line) {
                        if let Ok(mut q) = changed.lock() {
                            q.push((space.clone(), shape));
                        }
                    } else if tx.send(Ok(line)).is_err() {
                        break;
                    }
                }
                Err(e) => {
                    let _ = tx.send(Err(format!("could not read its answer ({})", e)));
                    break;
                }
            }
        }
    });
    Ok(Worker {
        child,
        stdin,
        answers,
    })
}

/// The request envelope every non-native transport receives.
fn request_line(name: &str, args: Vec<V>) -> String {
    let mut m = BTreeMap::new();
    m.insert("call".to_string(), V::Text(name.to_string()));
    m.insert("args".to_string(), V::List(args));
    to_json(&V::Map(m))
}

// -- http -------------------------------------------------------------------

/// Split `http://host[:port]/path` into (host, port, path).
pub fn split_url(url: &str) -> Result<(String, u16, String), String> {
    let rest = url
        .strip_prefix("http://")
        .ok_or_else(|| format!("`{}` is not an http:// url.", url))?;
    let (authority, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };
    if authority.is_empty() {
        return Err(format!("`{}` names no host.", url));
    }
    let (host, port) = match authority.rsplit_once(':') {
        Some((h, p)) => {
            let n: u16 = p
                .parse()
                .map_err(|_| format!("`{}` has a bad port `{}`.", url, p))?;
            (h.to_string(), n)
        }
        None => (authority.to_string(), 80),
    };
    Ok((host, port, path.to_string()))
}

fn call_http(url: &str, name: &str, args: Vec<V>) -> Result<V, String> {
    let (host, port, path) = split_url(url)?;
    let body = request_line(name, args);
    let mut s = std::net::TcpStream::connect((host.as_str(), port))
        .map_err(|e| format!("could not reach `{}`: {}", url, e))?;
    let req = format!(
        "POST {} HTTP/1.1\r\nhost: {}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
        path,
        host,
        body.len(),
        body
    );
    s.write_all(req.as_bytes())
        .map_err(|e| format!("could not send to `{}`: {}", url, e))?;
    let mut raw = Vec::new();
    s.read_to_end(&mut raw)
        .map_err(|e| format!("could not read from `{}`: {}", url, e))?;
    let text = String::from_utf8_lossy(&raw).to_string();
    let status: u16 = text
        .split_whitespace()
        .nth(1)
        .and_then(|c| c.parse().ok())
        .unwrap_or(0);
    let payload = match text.find("\r\n\r\n") {
        Some(i) => text[i + 4..].to_string(),
        None => String::new(),
    };
    if !(200..300).contains(&status) {
        return Err(format!(
            "foreign service at `{}` answered {} — {}",
            url,
            status,
            payload.trim()
        ));
    }
    decode(&payload, url, name)
}

// -- the shared result envelope ---------------------------------------------

/// Decode a transport's answer (ADR-0017): an object whose only key is
/// `error` (a text) is a fault; one whose only key is `ok` yields that value;
/// anything else IS the value, so the simple case stays ceremony-free.
pub fn decode(text: &str, where_: &str, name: &str) -> Result<V, String> {
    let v = from_json(text.trim()).ok_or_else(|| {
        format!(
            "foreign `{}.{}` returned malformed JSON: {}",
            where_,
            name,
            text.trim()
        )
    })?;
    if let V::Map(m) = &v {
        if m.len() == 1 {
            if let Some(V::Text(msg)) = m.get("error") {
                return Err(msg.clone());
            }
            if let Some(inner) = m.get("ok") {
                return Ok(inner.clone());
            }
        }
    }
    Ok(v)
}

// -- reachability (`ashlar foreign check`) ----------------------------------

/// What `check` proved about one space.
pub struct Reach {
    pub space: String,
    pub via: String,
    pub detail: String,
    pub problems: Vec<String>,
}

/// Verify every declared name of one space is actually reachable, turning a
/// runtime fault into a build-time correction (ADR-0017).
pub fn check_space(root: &Path, space: &str, names: &[String]) -> Reach {
    let mut b = Boundary::new();
    let via = match b.via(root, space) {
        Ok(v) => v,
        Err(e) => {
            return Reach {
                space: space.to_string(),
                via: "unreadable".to_string(),
                detail: String::new(),
                problems: vec![e],
            }
        }
    };
    let label = via.label().to_string();
    let mut problems = Vec::new();
    let detail;
    match via {
        Via::Native { library, symbols } => {
            match b.native_handle(root, space, &library) {
                Ok(handle) => {
                    detail = library.clone().unwrap_or_else(|| format!("foreign/{}", space));
                    for n in names {
                        let symbol = symbols.get(n).cloned().unwrap_or_else(|| n.clone());
                        if lookup(handle, &symbol).is_none() {
                            problems.push(format!(
                                "`{}` needs symbol `{}`, which the library does not export. Export it, or map it in `foreign.json` under `symbols`.",
                                n, symbol
                            ));
                        }
                    }
                }
                Err(e) => {
                    detail = String::new();
                    problems.push(e);
                }
            }
        }
        Via::Worker { run } => {
            detail = run.join(" ");
            // Speaking the protocol is the proof. A worker that answers an
            // unknown call with an `error` envelope still answered.
            match probe_worker(root, &run) {
                Ok(()) => {}
                Err(e) => problems.push(e),
            }
        }
        Via::Command { run, .. } => {
            detail = run.join(" ");
            // Is the program there? Asked by looking, not by running it: a
            // command's arguments are the program's own, so there is no
            // side-effect-free invocation to probe with — `sqlite3 --version`
            // is safe and `rm --version` is a guess about someone else's CLI.
            if let Err(e) = findable(root, &run[0]) {
                problems.push(e);
            }
        }
        Via::Http { url } => {
            detail = url.clone();
            // Connect only: a POST could have side effects, and reachability
            // is what `check` is entitled to prove.
            match split_url(&url).and_then(|(h, p, _)| {
                std::net::TcpStream::connect((h.as_str(), p))
                    .map(|_| ())
                    .map_err(|e| format!("could not reach `{}`: {}", url, e))
            }) {
                Ok(()) => {}
                Err(e) => problems.push(e),
            }
        }
    }
    Reach {
        space: space.to_string(),
        via: label,
        detail,
        problems,
    }
}

/// Whether a program can be found to run: a path is a file that exists, a
/// bare name is somewhere on `PATH`. The same question a shell asks, and the
/// most `check` can prove without running somebody's command for them.
pub fn findable(root: &Path, program: &str) -> Result<(), String> {
    let named_path = program.contains('/') || program.contains('\\');
    if named_path {
        let direct = root.join(program);
        if Path::new(program).is_file() || direct.is_file() {
            return Ok(());
        }
        return Err(format!("`{}` is not a file on this machine.", program));
    }
    let path = std::env::var_os("PATH").unwrap_or_default();
    for dir in std::env::split_paths(&path) {
        if dir.join(program).is_file() {
            return Ok(());
        }
        // Windows spells the same program with an extension.
        for ext in ["exe", "bat", "cmd"] {
            if dir.join(format!("{}.{}", program, ext)).is_file() {
                return Ok(());
            }
        }
    }
    Err(format!(
        "`{}` is not on PATH. Install it, name its full path in foreign.json, \
         or bind this space to something else.",
        program
    ))
}

/// Run an ordinary program and take its output as the answer.
///
/// This is the transport for what is already on the machine: `sqlite3`,
/// `git`, `ffmpeg`, a shell script, a Python file. No ABI, no envelope, no
/// adapter to write — the reason it exists is that the `native` transport
/// asks for a C-ABI shim before you can run a `select`, and a capability that
/// costs 165 lines of marshalling to reach is one an author will not reach for.
///
/// argv is `run`, then the items that select this name (the name itself
/// unless `args` says otherwise), then the arguments as text. Output is the
/// answer: JSON if it parses as JSON, else the text as it stands. A non-zero
/// exit is a fault carrying what the program said on stderr, which is where
/// programs put the reason.
fn call_command(
    root: &Path,
    run: &[String],
    which: &BTreeMap<String, Vec<String>>,
    name: &str,
    args: Vec<V>,
) -> Result<V, String> {
    let mut argv: Vec<String> = run.to_vec();
    match which.get(name) {
        Some(fixed) => argv.extend(fixed.iter().cloned()),
        None => argv.push(name.to_string()),
    }
    argv.extend(args.iter().map(as_argument));
    let out = Command::new(&argv[0])
        .args(&argv[1..])
        .current_dir(root)
        .output()
        .map_err(|e| {
            format!(
                "could not run `{}`: {}. Name the program this machine actually has in \
                 `foreign.json` — how a capability is reached is a deployment fact (§9.10).",
                argv[0], e
            )
        })?;
    if !out.status.success() {
        let said = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(if said.is_empty() {
            format!("`{}` exited with {}", argv.join(" "), out.status)
        } else {
            said
        });
    }
    let said = String::from_utf8_lossy(&out.stdout).trim().to_string();
    Ok(from_json(&said).unwrap_or(V::Text(said)))
}

/// One argument as a program expects it: a text is itself, a number or bool
/// is what it prints as, and a list or map is JSON — because a command line
/// carries text, and the only lossless text for a structure is its JSON.
fn as_argument(value: &V) -> String {
    match value {
        V::Text(t) => t.clone(),
        V::None => String::new(),
        other => to_json(other),
    }
}

/// Spawn a worker, send one probe call, and require a well-formed JSON line
/// back within a few seconds. Runs the read on its own thread so a worker
/// that never answers fails the check instead of hanging it.
fn probe_worker(root: &Path, run: &[String]) -> Result<(), String> {
    let mut w = spawn_worker(root, run, "probe", Changed::default())
        .map_err(|e| format!("could not start `{}`: {}", run.join(" "), e))?;
    let request = request_line("__ping", Vec::new());
    let write = w
        .stdin
        .write_all(request.as_bytes())
        .and_then(|_| w.stdin.write_all(b"\n"))
        .and_then(|_| w.stdin.flush());
    if let Err(e) = write {
        let _ = w.child.kill();
        let _ = w.child.wait();
        return Err(format!("could not write to the worker: {}", e));
    }
    // The reader thread is already draining it, so this waits on the answer
    // rather than on the pipe — a worker that never answers fails the check
    // instead of hanging it.
    let answered = w.answers.recv_timeout(std::time::Duration::from_secs(5));
    let _ = w.child.kill();
    let _ = w.child.wait();
    match answered {
        Ok(Err(e)) => Err(format!("the worker could not be read: {}", e)),
        Ok(Ok(line)) => {
            if from_json(line.trim()).is_some() {
                Ok(())
            } else {
                Err(format!(
                    "the worker answered with something that is not JSON: {}",
                    line.trim()
                ))
            }
        }
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
            Err("the worker closed its output without answering.".to_string())
        }
        Err(_) => Err("the worker did not answer within 5s (is its output line-buffered and flushed?).".to_string()),
    }
}

// -- the dynamic loader, which is the one platform-specific thing here ------
//
// `native` needs a POSIX dynamic loader. `worker` and `http` need nothing but
// std, so they run wherever Rust runs — which is why the two functions below
// are the ONLY platform gate in the workspace, and why a platform without
// `dlopen` still gets a complete foreign boundary through the other two
// transports. Everything else in the runtime is portable.

/// Library extensions probed for the derived path, in order. Only meaningful
/// where a POSIX loader exists; see `open_library`.
#[cfg(unix)]
const LIB_EXTS: [&str; 2] = [".so", ".dylib"];
#[cfg(not(unix))]
const LIB_EXTS: [&str; 0] = [];

/// Open a shared library, returning an opaque handle. The whole `unsafe`
/// surface of this workspace is here and in `lookup` (ADR-0017).
#[cfg(unix)]
fn open_library(path: &Path) -> Result<usize, String> {
    let c_path = std::ffi::CString::new(path.to_string_lossy().as_bytes())
        .map_err(|_| format!("library path `{}` contains a NUL byte.", path.display()))?;
    let h = unsafe { dlopen(c_path.as_ptr(), RTLD_NOW) };
    if h.is_null() {
        return Err(format!(
            "foreign library `{}` could not be loaded (it exists but dlopen refused it — wrong architecture, or a missing dependency of its own).",
            path.display()
        ));
    }
    Ok(h as usize)
}

/// Without a POSIX loader there is no `native` transport — and saying so with
/// the correction attached is the whole behavior, not a stub: `worker` and
/// `http` carry the same envelope and need no loader at all.
#[cfg(not(unix))]
fn open_library(path: &Path) -> Result<usize, String> {
    Err(format!(
        "the `native` transport needs a POSIX dynamic loader, which this platform does not provide, so `{}` cannot be opened. Bind the space to a `worker` (a co-process in any language) or `http` transport in `foreign.json` — both carry the same envelope.",
        path.display()
    ))
}

#[cfg(unix)]
extern "C" {
    fn dlopen(filename: *const std::os::raw::c_char, flags: std::os::raw::c_int) -> *mut std::os::raw::c_void;
    fn dlsym(handle: *mut std::os::raw::c_void, symbol: *const std::os::raw::c_char) -> *mut std::os::raw::c_void;
}
#[cfg(unix)]
const RTLD_NOW: std::os::raw::c_int = 2;

type ForeignAbi = unsafe extern "C" fn(*const std::os::raw::c_char) -> *mut std::os::raw::c_char;
type ForeignFree = unsafe extern "C" fn(*mut std::os::raw::c_char);

/// `dlsym` for one symbol, `None` when the library does not export it.
#[cfg(unix)]
fn lookup(handle: usize, symbol: &str) -> Option<*mut std::os::raw::c_void> {
    let c_name = std::ffi::CString::new(symbol.as_bytes()).ok()?;
    let sym = unsafe { dlsym(handle as *mut std::os::raw::c_void, c_name.as_ptr()) };
    if sym.is_null() {
        None
    } else {
        Some(sym)
    }
}

/// No loader, so no symbol can ever resolve. `native_handle` refuses first, so
/// this is unreachable in practice; it exists so the module compiles as one
/// piece on every platform rather than only where `dlopen` lives.
#[cfg(not(unix))]
fn lookup(_handle: usize, _symbol: &str) -> Option<*mut std::os::raw::c_void> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bindings_parse_every_transport() {
        let b = parse_bindings(
            r#"{
              "a": {"via":"native","library":"/x/lib.so","symbols":{"open":"a_open_v2"}},
              "b": {"via":"worker","run":["python3","foreign/b.py"]},
              "c": {"via":"http","url":"http://127.0.0.1:9000/rpc"}
            }"#,
        )
        .expect("parses");
        assert_eq!(b.len(), 3);
        match &b["a"] {
            Via::Native { library, symbols } => {
                assert_eq!(library.as_deref(), Some("/x/lib.so"));
                assert_eq!(symbols["open"], "a_open_v2");
            }
            other => panic!("{:?}", other),
        }
        assert_eq!(b["b"], Via::Worker { run: vec!["python3".into(), "foreign/b.py".into()] });
        assert_eq!(b["c"], Via::Http { url: "http://127.0.0.1:9000/rpc".into() });
    }

    #[test]
    fn a_malformed_binding_is_loud() {
        // Unknown transport, missing fields, and TLS all name the fix.
        assert!(parse_bindings(r#"{"a":{"via":"carrier-pigeon"}}"#)
            .unwrap_err()
            .contains("unknown `via`"));
        assert!(parse_bindings(r#"{"a":{"via":"worker"}}"#).unwrap_err().contains("needs `run`"));
        assert!(parse_bindings(r#"{"a":{"via":"http","url":"https://x/y"}}"#)
            .unwrap_err()
            .contains("does not speak TLS"));
        assert!(parse_bindings("not json").is_err());
    }

    #[test]
    fn an_unbound_space_takes_the_derived_default() {
        let mut b = Boundary::new();
        let root = std::env::temp_dir();
        assert_eq!(b.via(&root, "anything").unwrap(), Via::derived());
    }

    #[test]
    fn the_mesh_space_derives_to_a_co_process() {
        // covers: B5, G4
        // And the co-process is US. The mesh does not ship an Ashlar-shaped
        // adapter and must not have to: this toolchain speaks the socket it
        // already exposes.
        let mut b = Boundary::new();
        let root = std::env::temp_dir();
        let ours = Via::Worker {
            run: vec![
                "ashlar".to_string(),
                "mesh".to_string(),
                "worker".to_string(),
            ],
        };
        assert_eq!(b.via(&root, MESH_SPACE).unwrap(), ours);
        // A neighbouring name is not it: the rule is one name, not a prefix,
        // so `mesh.anything` stays an ordinary space.
        assert_eq!(b.via(&root, "mesh.sites").unwrap(), Via::derived());
        assert_eq!(b.via(&root, "meshx").unwrap(), Via::derived());
    }

    #[test]
    fn a_binding_file_still_overrides_the_mesh_derivation() {
        // The derived worker is a default, not a law: deployment names the
        // transport, exactly as it does for every other space (ADR-0017).
        let bindings =
            parse_bindings(r#"{"mesh":{"via":"http","url":"http://127.0.0.1:9000/rpc"}}"#)
                .unwrap();
        assert_eq!(
            bindings.get(MESH_SPACE),
            Some(&Via::Http {
                url: "http://127.0.0.1:9000/rpc".to_string()
            })
        );
    }

    #[test]
    fn renaming_onto_or_off_a_mesh_name_is_reported() {
        // covers: E3
        assert_eq!(derived_worker_radius("tools", "chat.data"), None);
        let off = derived_worker_radius(MESH_SPACE, "tools").expect("leaving the mesh name reports");
        assert!(off.contains("ashlar mesh worker") && off.contains("foreign/tools"), "{}", off);
        let onto = derived_worker_radius("tools", MESH_SPACE).expect("entering it reports");
        assert!(onto.contains("ashlar mesh worker"), "{}", onto);
    }

    #[test]
    fn only_our_own_derived_argv_falls_back_to_this_executable() {
        assert!(is_self_worker(&[
            "ashlar".to_string(),
            "mesh".to_string(),
            "worker".to_string()
        ]));
        // Anything a deployment wrote is spawned exactly as written.
        assert!(!is_self_worker(&["ashlar".to_string(), "mesh".to_string()]));
        assert!(!is_self_worker(&[
            "python3".to_string(),
            "mesh".to_string(),
            "worker".to_string()
        ]));
        assert!(!is_self_worker(&[]));
    }

    #[test]
    fn the_envelope_decodes_three_ways() {
        // A bare value is the result; `error` faults; `ok` is the escape hatch.
        assert_eq!(decode("[1,2]", "s", "n").unwrap(), V::List(vec![V::Number(1.0), V::Number(2.0)]));
        assert_eq!(decode(r#"{"error":"nope"}"#, "s", "n").unwrap_err(), "nope");
        assert_eq!(decode(r#"{"ok":{"error":"literal"}}"#, "s", "n").unwrap(), {
            let mut m = BTreeMap::new();
            m.insert("error".to_string(), V::Text("literal".to_string()));
            V::Map(m)
        });
        // A two-key object is data, not an envelope.
        assert!(matches!(decode(r#"{"error":"x","also":1}"#, "s", "n"), Ok(V::Map(_))));
        assert!(decode("{oops", "s", "n").unwrap_err().contains("malformed JSON"));
    }

    #[test]
    fn urls_split_into_host_port_path() {
        assert_eq!(split_url("http://h/p").unwrap(), ("h".into(), 80, "/p".into()));
        assert_eq!(split_url("http://h:9000/rpc").unwrap(), ("h".into(), 9000, "/rpc".into()));
        assert_eq!(split_url("http://h").unwrap(), ("h".into(), 80, "/".into()));
        assert!(split_url("https://h/p").is_err());
    }
}

#[cfg(test)]
mod key_tests {
    use super::*;

    #[test]
    fn rename_key_rewrites_only_the_key_and_reverses_exactly() {
        let text = "{\n  \"tools\": { \"via\": \"native\", \"library\": \"tools\" }\n}\n";
        let renamed = rename_key(text, "tools", "kit").expect("key present");
        assert_eq!(
            renamed,
            "{\n  \"kit\": { \"via\": \"native\", \"library\": \"tools\" }\n}\n",
            "only the key changes: a `library` value that happens to match must be left alone"
        );
        // E4: reversing restores the file byte-for-byte.
        assert_eq!(rename_key(&renamed, "kit", "tools").unwrap(), text);
    }

    #[test]
    fn rename_key_ignores_nested_keys_and_absent_names() {
        // `symbols` maps Ashlar names to exports at depth 2 — never a space.
        let text = "{ \"geo\": { \"via\": \"native\", \"symbols\": { \"lookup\": \"geo_v2\" } } }";
        assert!(rename_key(text, "lookup", "find").is_none());
        assert!(rename_key(text, "nope", "x").is_none());
        assert_eq!(
            rename_key(text, "geo", "atlas").unwrap(),
            "{ \"atlas\": { \"via\": \"native\", \"symbols\": { \"lookup\": \"geo_v2\" } } }"
        );
    }

    #[test]
    fn key_span_points_at_the_key() {
        let text = "{\n  \"tools\": { \"via\": \"http\", \"url\": \"http://x\" }\n}\n";
        let span = key_span(text, "tools");
        assert_eq!((span.start.line, span.start.col), (2, 3));
        assert_eq!(span.end.line, 2);
    }

    #[test]
    fn a_command_binding_reaches_a_program_with_no_adapter() {
        // The point of this transport: what is already on the machine should
        // cost nothing to reach. `native` asks for a C-ABI shim before you can
        // run a `select` — 165 lines of marshalling in the one example that
        // uses it — and an author does not reach for that.
        let bound = parse_bindings(
            r#"{ "db": { "via": "command", "run": ["sqlite3", "-json", "app.db"],
                         "args": { "query": [] } },
                 "vcs": { "via": "command", "run": ["git"] } }"#,
        )
        .unwrap();
        assert_eq!(
            bound.get("db"),
            Some(&Via::Command {
                run: vec!["sqlite3".into(), "-json".into(), "app.db".into()],
                args: [("query".to_string(), vec![])].into_iter().collect(),
            })
        );
        // No `args` entry means the name IS the subcommand, which is how
        // every tool shaped like `git status` binds with nothing written.
        assert_eq!(
            bound.get("vcs"),
            Some(&Via::Command {
                run: vec!["git".into()],
                args: BTreeMap::new(),
            })
        );
        assert_eq!(bound["db"].label(), "command");
        assert!(parse_bindings(r#"{ "db": { "via": "command" } }"#)
            .unwrap_err()
            .contains("needs `run`"));
        assert!(
            parse_bindings(r#"{ "db": { "via": "command", "run": ["x"], "args": { "q": "s" } } }"#)
                .unwrap_err()
                .contains("list of text")
        );
    }

    #[test]
    fn an_argument_crosses_as_the_text_a_command_line_carries() {
        assert_eq!(as_argument(&V::Text("a b".into())), "a b");
        assert_eq!(as_argument(&V::Number(3.0)), "3");
        assert_eq!(as_argument(&V::Bool(true)), "true");
        assert_eq!(as_argument(&V::None), "");
        // A structure has one lossless text, and it is its JSON.
        assert_eq!(
            as_argument(&V::List(vec![V::Number(1.0), V::Text("x".into())])),
            "[1,\"x\"]"
        );
    }

    #[test]
    fn a_command_that_is_not_there_is_named_before_a_request_finds_out() {
        let root = std::env::temp_dir();
        assert!(findable(&root, "sh").is_ok() || cfg!(windows));
        let missing = findable(&root, "definitely-not-a-real-program-xyz").unwrap_err();
        assert!(missing.contains("is not on PATH"), "{}", missing);
        assert!(missing.contains("foreign.json"), "the fix is named: {}", missing);
        let bad_path = findable(&root, "./nope/nothing").unwrap_err();
        assert!(bad_path.contains("is not a file"), "{}", bad_path);
    }

    #[test]
    fn a_push_is_told_from_an_answer_by_shape_alone() {
        // The unsolicited line is the whole of the push protocol, so what
        // counts as one has to be unambiguous: exactly `{"changed": "<Shape>"}`
        // and nothing else. Anything wider would swallow a worker's answer
        // that happened to carry a `changed` field of its own.
        assert_eq!(
            pushed_collection(r#"{"changed":"mesh.Peer"}"#),
            Some("mesh.Peer".to_string())
        );
        assert_eq!(
            pushed_collection("  {\"changed\": \" Row \"}  \n"),
            Some("Row".to_string())
        );
        for answer in [
            r#"{"ok":{"changed":"mesh.Peer"}}"#,
            r#"{"changed":"mesh.Peer","ok":1}"#,
            r#"{"changed":""}"#,
            r#"{"changed":7}"#,
            r#"{"ok":[1,2]}"#,
            r#"{"error":"no"}"#,
            "not json at all",
        ] {
            assert_eq!(pushed_collection(answer), None, "{}", answer);
        }
    }

    #[test]
    fn derived_library_paths_cover_every_probed_extension() {
        // The probe list is the platform's, so the assertion is too: a
        // `native` binding is only reachable where a POSIX loader exists, and
        // on a platform without one there is no derived library to rename.
        #[cfg(unix)]
        assert_eq!(
            derived_library_paths("tools"),
            vec!["foreign/tools.so", "foreign/tools.dylib"]
        );
        #[cfg(not(unix))]
        assert!(derived_library_paths("tools").is_empty());
    }
}
