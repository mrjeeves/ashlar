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

use crate::eval::{from_json, to_json, V};
use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

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

    pub fn label(&self) -> &'static str {
        match self {
            Via::Native { .. } => "native",
            Via::Worker { .. } => "worker",
            Via::Http { .. } => "http",
        }
    }
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
                "binding for `{}` has no `via`; use \"native\", \"worker\", or \"http\".",
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
            "worker" => {
                let Some(V::List(items)) = fields.get("run") else {
                    return Err(format!(
                        "binding for `{}`: a worker needs `run`, a list like [\"python3\", \"foreign/x.py\"].",
                        space
                    ));
                };
                let mut run = Vec::new();
                for it in items {
                    let V::Text(a) = it else {
                        return Err(format!("binding for `{}`: every `run` entry must be a text.", space));
                    };
                    run.push(a.clone());
                }
                if run.is_empty() {
                    return Err(format!("binding for `{}`: `run` must name a command.", space));
                }
                Via::Worker { run }
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
                    "binding for `{}`: unknown `via` \"{}\"; use \"native\", \"worker\", or \"http\".",
                    space, other
                ))
            }
        };
        out.insert(space, bound);
    }
    Ok(out)
}

// -- the runtime boundary ---------------------------------------------------

/// Live foreign state: resolved bindings, open libraries, running workers.
pub struct Boundary {
    bindings: Option<BTreeMap<String, Via>>,
    libs: BTreeMap<String, usize>,
    workers: BTreeMap<String, Worker>,
}

struct Worker {
    child: Child,
    stdin: ChildStdin,
    out: BufReader<ChildStdout>,
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
            .unwrap_or_else(Via::derived))
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
            let Ok(c_path) = std::ffi::CString::new(path.to_string_lossy().as_bytes()) else {
                continue;
            };
            let h = unsafe { dlopen(c_path.as_ptr(), RTLD_NOW) };
            if !h.is_null() {
                self.libs.insert(space.to_string(), h as usize);
                return Ok(h as usize);
            }
            return Err(format!(
                "foreign library `{}` could not be loaded (it exists but dlopen refused it — wrong architecture, or a missing dependency of its own).",
                path.display()
            ));
        }
        Err(format!(
            "foreign space `{}` has no library. Looked for {}. Build the shim, or bind the space in `foreign.json`.",
            space,
            tried
                .iter()
                .map(|p| format!("`{}`", p.display()))
                .collect::<Vec<_>>()
                .join(", ")
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
            let w = spawn_worker(root, run)
                .map_err(|e| format!("foreign worker for `{}` could not start: {}", space, e))?;
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
                // starts a fresh one — lifecycle, not failover (ADR-0017).
                if let Some(mut w) = self.workers.remove(space) {
                    let _ = w.child.kill();
                    let _ = w.child.wait();
                }
                Err(format!("foreign worker for `{}` failed: {}", space, e))
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
    /// reap this worker.
    fn exchange(&mut self, request: &str) -> Result<String, String> {
        self.stdin
            .write_all(request.as_bytes())
            .and_then(|_| self.stdin.write_all(b"\n"))
            .and_then(|_| self.stdin.flush())
            .map_err(|e| format!("could not write to it ({})", e))?;
        let mut line = String::new();
        match self.out.read_line(&mut line) {
            Ok(0) => Err("it closed its output without answering".to_string()),
            Ok(_) => Ok(line),
            Err(e) => Err(format!("could not read its answer ({})", e)),
        }
    }
}

fn spawn_worker(root: &Path, run: &[String]) -> Result<Worker, std::io::Error> {
    let mut child = Command::new(&run[0])
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
    Ok(Worker {
        child,
        stdin,
        out: BufReader::new(stdout),
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

/// Spawn a worker, send one probe call, and require a well-formed JSON line
/// back within a few seconds. Runs the read on its own thread so a worker
/// that never answers fails the check instead of hanging it.
fn probe_worker(root: &Path, run: &[String]) -> Result<(), String> {
    let mut w = spawn_worker(root, run).map_err(|e| format!("could not start `{}`: {}", run.join(" "), e))?;
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
    let (tx, rx) = std::sync::mpsc::channel();
    let mut out = w.out;
    std::thread::spawn(move || {
        let mut line = String::new();
        let r = out.read_line(&mut line).map(|n| (n, line));
        let _ = tx.send(r);
    });
    let answered = rx.recv_timeout(std::time::Duration::from_secs(5));
    let _ = w.child.kill();
    let _ = w.child.wait();
    match answered {
        Ok(Ok((0, _))) => Err("the worker closed its output without answering.".to_string()),
        Ok(Ok((_, line))) => {
            if from_json(line.trim()).is_some() {
                Ok(())
            } else {
                Err(format!(
                    "the worker answered with something that is not JSON: {}",
                    line.trim()
                ))
            }
        }
        Ok(Err(e)) => Err(format!("could not read the worker's answer: {}", e)),
        Err(_) => Err("the worker did not answer within 5s (is its output line-buffered and flushed?).".to_string()),
    }
}

// -- raw dl bindings --------------------------------------------------------

/// Library extensions probed for the derived path, in order.
const LIB_EXTS: [&str; 3] = [".so", ".dylib", ".dll"];

// The only `unsafe` in the workspace, confined to this module (ADR-0017).
extern "C" {
    fn dlopen(filename: *const std::os::raw::c_char, flags: std::os::raw::c_int) -> *mut std::os::raw::c_void;
    fn dlsym(handle: *mut std::os::raw::c_void, symbol: *const std::os::raw::c_char) -> *mut std::os::raw::c_void;
}
const RTLD_NOW: std::os::raw::c_int = 2;

type ForeignAbi = unsafe extern "C" fn(*const std::os::raw::c_char) -> *mut std::os::raw::c_char;
type ForeignFree = unsafe extern "C" fn(*mut std::os::raw::c_char);

/// `dlsym` for one symbol, `None` when the library does not export it.
fn lookup(handle: usize, symbol: &str) -> Option<*mut std::os::raw::c_void> {
    let c_name = std::ffi::CString::new(symbol.as_bytes()).ok()?;
    let sym = unsafe { dlsym(handle as *mut std::os::raw::c_void, c_name.as_ptr()) };
    if sym.is_null() {
        None
    } else {
        Some(sym)
    }
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
