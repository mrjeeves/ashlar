//! `ashlar mesh worker` — the adapter behind the two mesh spaces (§9.10).
//!
//! The mesh is a capability, and the daemons that provide it already have
//! clients: a control socket each, carrying JSON, driven by their own CLIs and
//! GUIs. This module speaks those two sockets. **Nothing outside this
//! repository changes to make that work**, which is the whole point — ADR-0017
//! exists because a boundary that requires the foreign system to be re-authored
//! for us is not a boundary, and shipping an Ashlar-shaped adapter into someone
//! else's daemon would have been exactly that mistake with a different face.
//!
//! Two sockets, because the mesh is two capabilities:
//!
//! - **the roster** (`mesh`) — the mesh daemon's own socket, one JSON object
//!   per line, keyed by `op`. Identity, joined networks, peers.
//! - **the sites** (`mesh.sites`) — the node's socket, length-prefixed frames
//!   (`[u32 BE len][tag][JSON]`) carrying `{cmd, args}`. Publishing a local
//!   port to the mesh's members and reaching theirs needs a proxy, which the
//!   node has and the mesh daemon alone does not.
//!
//! Both are reached as a sidecar: reuse whatever is already running on this
//! machine, and bring it up if nothing is. That is how a program built on this
//! stack is already expected to behave, and it is why an Ashlar site needs no
//! setup beyond the mesh being installed.
//!
//! Unix only. The sockets are named pipes on Windows, and a zero-dependency
//! runtime has no client for those (§9.10 already says `native` needs a POSIX
//! loader); the failure is one sentence at the boundary rather than a silent
//! empty roster.

use crate::eval::{from_json, to_json, V};
use std::collections::BTreeMap;
use std::io::{BufRead, Write};
use std::path::PathBuf;

/// The mesh an Ashlar site lands on when nothing names another. `lib/mesh`
/// defaults its `network` setting to the same word, so a program that says
/// nothing and a machine told nothing meet on one area rather than two.
pub const DEFAULT_NETWORK: &str = "ashlar";

/// Run the worker: read one call per line, answer one per line, until stdin
/// closes. A failure crosses as `{"error": …}` and the runtime raises it at
/// the Ashlar call site with the message intact — worth more than a dead
/// co-process, so this exits zero for a failed call.
pub fn run() -> i32 {
    let stdin = std::io::stdin();
    let mut out = std::io::stdout();
    let mut session = Session::default();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { return 1 };
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let answer = match parse_call(line) {
            Ok((name, args)) => match session.dispatch(&name, &args) {
                Ok(value) => envelope("ok", value),
                Err(e) => envelope("error", V::Text(e)),
            },
            Err(e) => envelope("error", V::Text(e)),
        };
        if writeln!(out, "{}", answer).and_then(|_| out.flush()).is_err() {
            return 0; // the runtime hung up; nothing left to answer
        }
    }
    0
}

fn envelope(key: &str, value: V) -> String {
    let mut m = BTreeMap::new();
    m.insert(key.to_string(), value);
    to_json(&V::Map(m))
}

/// Split one request envelope into a call name and its arguments.
pub fn parse_call(line: &str) -> Result<(String, Vec<V>), String> {
    let Some(V::Map(m)) = from_json(line) else {
        return Err("a call must be a JSON object".to_string());
    };
    let Some(V::Text(name)) = m.get("call") else {
        return Err("a call envelope needs a `call` name".to_string());
    };
    let args = match m.get("args") {
        Some(V::List(items)) => items.clone(),
        None => Vec::new(),
        Some(_) => return Err("`args` must be a list".to_string()),
    };
    Ok((name.clone(), args))
}

/// What one worker remembers: the mesh this program asked for, and the last
/// roster it described. Everything that matters lives in the daemon — a worker
/// that died and respawned rejoins the same mesh and sees the same peers.
#[derive(Default)]
pub struct Session {
    network: Option<String>,
    revision: f64,
    fingerprint: Option<String>,
}

impl Session {
    fn dispatch(&mut self, name: &str, args: &[V]) -> Result<V, String> {
        match name {
            // -- the roster (`mesh`) ---------------------------------------
            "here" => self.here(),
            "peers" => Ok(V::List(self.peers()?)),
            "enter" => {
                let network = network_or_default(&text_arg(args, 0)?);
                let label = text_arg(args, 1)?;
                self.enter(&network, &label)
            }
            "revision" | "reread" => {
                self.refresh()?;
                Ok(V::Number(self.revision))
            }
            // -- the sites (`mesh.sites`) ----------------------------------
            "expose" => {
                let port = port_arg(args, 0)?;
                let label = text_arg(args, 1)?;
                let network = network_or_default(&text_arg(args, 2).unwrap_or_default());
                self.expose(port, &label, &network)
            }
            "unexpose" => {
                let port = port_arg(args, 0)?;
                let exposed = without_exposed(&self.exposed()?, port);
                self.set_exposed(exposed)?;
                Ok(V::Bool(true))
            }
            "published" => self.published(),
            "nearby" => self.nearby(),
            other => Err(format!(
                "no such call: `{}`. The mesh answers here, peers, enter, \
                 revision, reread; its sites answer expose, unexpose, \
                 published, nearby.",
                other
            )),
        }
    }

    /// The mesh in force: what `enter` was told, else the default area.
    fn network(&self) -> String {
        self.network
            .clone()
            .unwrap_or_else(|| DEFAULT_NETWORK.to_string())
    }

    // -- the roster ---------------------------------------------------------

    fn here(&mut self) -> Result<V, String> {
        let identity = daemon("identity_show", BTreeMap::new())?;
        let peers = self.peers().unwrap_or_default();
        Ok(map(&[
            ("id", V::Text(field(&identity, "device_id"))),
            ("label", V::Text(field(&identity, "label"))),
            ("network", V::Text(self.network())),
            ("peers", V::Number(peers.len() as f64)),
        ]))
    }

    fn peers(&self) -> Result<Vec<V>, String> {
        let mut args = BTreeMap::new();
        args.insert("network".to_string(), V::Text(self.network()));
        let answer = daemon("peers_list", args)?;
        let Some(V::List(list)) = at(&answer, "peers") else {
            return Ok(Vec::new());
        };
        Ok(list.iter().map(peer_row).collect())
    }

    fn enter(&mut self, network: &str, label: &str) -> Result<V, String> {
        if !label.trim().is_empty() {
            // A label that will not stick is not a reason to refuse the mesh:
            // the roster works by id, and the name is cosmetic.
            let mut args = BTreeMap::new();
            args.insert("label".to_string(), V::Text(label.trim().to_string()));
            let _ = daemon("identity_set_label", args);
        }
        let joined = daemon("networks_list", BTreeMap::new())
            .map(|v| already_joined(&v, network))
            .unwrap_or(false);
        if !joined {
            daemon("network_add", network_config(network, label))?;
        }
        self.network = Some(network.to_string());
        self.fingerprint = None;
        self.here()
    }

    /// Move the revision if — and only if — the roster is not what it was.
    /// `revision` is asked on a timer and marks nothing; `reread` is the call
    /// that marks the collection changed, so pages re-render when the roster
    /// moved rather than on every tick (§9.10).
    fn refresh(&mut self) -> Result<(), String> {
        let peers = self.peers()?;
        let print = fingerprint(&peers);
        if self.fingerprint.as_deref() != Some(print.as_str()) {
            self.fingerprint = Some(print);
            self.revision += 1.0;
        }
        Ok(())
    }

    // -- the sites ----------------------------------------------------------

    fn exposed(&self) -> Result<BTreeMap<String, String>, String> {
        Ok(exposed_map(&node("site_exposed", V::Map(BTreeMap::new()))?))
    }

    fn set_exposed(&self, exposed: BTreeMap<String, String>) -> Result<(), String> {
        let mut inner = BTreeMap::new();
        for (k, v) in exposed {
            inner.insert(k, V::Text(v));
        }
        let mut args = BTreeMap::new();
        args.insert("exposed".to_string(), V::Map(inner));
        node("site_set_exposed", V::Map(args)).map(|_| ())
    }

    fn expose(&mut self, port: u16, label: &str, network: &str) -> Result<V, String> {
        // The node keeps its own list of networks; joining there is what puts
        // the site's proxy on the same mesh the roster is read from.
        let joined = node("mesh_networks", V::Map(BTreeMap::new()))
            .map(|v| already_joined(&v, network))
            .unwrap_or(false);
        if !joined {
            let mut args = BTreeMap::new();
            args.insert(
                "config".to_string(),
                V::Map(network_config(network, label)),
            );
            node("mesh_network_add", V::Map(args))?;
        }
        // Add one port to the owner's exposed selection and leave the rest
        // alone. The node's proxy refuses every port outside that selection,
        // so this is the whole of publishing — and it cannot reach a service
        // its owner never exposed.
        let exposed = with_exposed(&self.exposed()?, port, label);
        self.set_exposed(exposed)?;
        self.network = Some(network.to_string());
        let identity = node("mesh_identity", V::Map(BTreeMap::new())).unwrap_or(V::None);
        let id = match &identity {
            V::Map(_) => field(&identity, "device_id"),
            _ => String::new(),
        };
        Ok(map(&[
            (
                "node",
                V::Text(if id.is_empty() { "this node".to_string() } else { id }),
            ),
            ("network", V::Text(network.to_string())),
            ("label", V::Text(label.to_string())),
        ]))
    }

    /// What this machine publishes, read back from the node's own selection
    /// rather than from anything this process remembered.
    fn published(&self) -> Result<V, String> {
        let exposed = self.exposed()?;
        let me = node("mesh_identity", V::Map(BTreeMap::new()))
            .map(|v| field(&v, "device_id"))
            .unwrap_or_default();
        let me = if me.is_empty() { "this node".to_string() } else { me };
        let mut out = Vec::new();
        for (id, label) in &exposed {
            let Some(port) = port_of(id) else { continue };
            let shown = if label.is_empty() { id.clone() } else { label.clone() };
            out.push(site(&me, &shown, &local_url(port)));
        }
        Ok(V::List(out))
    }

    /// The peers' sites, each with an address this machine can open. A peer's
    /// advert says what it serves; mapping it binds a local port the node
    /// proxies over the mesh, so the link a page renders is ordinary loopback.
    fn nearby(&self) -> Result<V, String> {
        let snapshot = node("session_snapshot", V::Map(BTreeMap::new()))?;
        let mappings = node("site_mappings", V::Map(BTreeMap::new())).unwrap_or(V::List(vec![]));
        let mut out = Vec::new();
        for (peer, port, label) in peer_sites(&snapshot) {
            let local = match existing_mapping(&mappings, &peer, port) {
                Some(p) => Some(p),
                None => {
                    let mut args = BTreeMap::new();
                    args.insert("node".to_string(), V::Text(peer.clone()));
                    args.insert("port".to_string(), V::Number(port as f64));
                    node("site_map", V::Map(args))
                        .ok()
                        .and_then(|v| match at(&v, "localPort") {
                            Some(V::Number(n)) => Some(n as u16),
                            _ => None,
                        })
                }
            };
            // A site that will not map is still a site the peer is running.
            // "There but unreachable from here" is a different fact from "not
            // there", and dropping it would report the second.
            let url = local.map(local_url).unwrap_or_default();
            out.push(site(&peer, &label, &url));
        }
        Ok(V::List(out))
    }
}

// ---------------------------------------------------------------------------
// Pure decisions — everything above the sockets
// ---------------------------------------------------------------------------

/// The mesh a call named, or the default when it named none.
pub fn network_or_default(named: &str) -> String {
    let named = named.trim();
    if named.is_empty() {
        DEFAULT_NETWORK.to_string()
    } else {
        named.to_string()
    }
}

/// The config that joins a mesh: open and auto-approving, because a roster
/// nobody can join is not a roster — everyone running the program should see
/// everyone else without a human approving each arrival.
///
/// That makes the mesh id itself the secret. The shared default is a public
/// square by design; a program that wants privacy names its own mesh (a
/// setting, §9.12) and should name an unguessable one.
pub fn network_config(network: &str, label: &str) -> BTreeMap<String, V> {
    let mut config = BTreeMap::new();
    config.insert("id".to_string(), V::Text(network.to_string()));
    config.insert("network_id".to_string(), V::Text(network.to_string()));
    config.insert("label".to_string(), V::Text(label.trim().to_string()));
    config.insert("kind".to_string(), V::Text("open".to_string()));
    config.insert("auto_approve".to_string(), V::Bool(true));
    let mut args = BTreeMap::new();
    args.insert("config".to_string(), V::Map(config));
    args
}

/// One roster row, in the shape `mesh.Peer` declares. A peer is `here` when
/// app traffic can actually flow to it — active, or shelved by the topology
/// with its heartbeat still up. Every other state is known-but-not-reachable,
/// and a live dot on a peer nothing can reach is worse than no dot.
pub fn peer_row(peer: &V) -> V {
    let status = field(peer, "status").to_ascii_lowercase();
    let id = field(peer, "device_id");
    let label = {
        let l = field(peer, "label");
        if l.trim().is_empty() {
            if id.is_empty() {
                "unknown".to_string()
            } else {
                id.clone()
            }
        } else {
            l.trim().to_string()
        }
    };
    map(&[
        ("id", V::Text(id)),
        ("label", V::Text(label)),
        ("here", V::Bool(status == "active" || status == "shelved")),
    ])
}

/// A stable summary of a roster, so a poll can tell "changed" from "asked".
pub fn fingerprint(peers: &[V]) -> String {
    let mut rows: Vec<String> = peers
        .iter()
        .map(|p| {
            format!(
                "{}:{}",
                field(p, "id"),
                matches!(at(p, "here"), Some(V::Bool(true)))
            )
        })
        .collect();
    rows.sort();
    rows.join(",")
}

/// Whether a networks answer already carries this mesh, keyed either way the
/// two daemons key it.
pub fn already_joined(networks: &V, network: &str) -> bool {
    let list = match at(networks, "networks") {
        Some(V::List(l)) => l,
        _ => match networks {
            V::List(l) => l.clone(),
            _ => return false,
        },
    };
    list.iter()
        .any(|n| field(n, "network_id") == network || field(n, "id") == network)
}

/// What a node calls the listening service on `port` (`tcp:8080`), and back.
pub fn service_id(port: u16) -> String {
    format!("tcp:{}", port)
}

pub fn port_of(id: &str) -> Option<u16> {
    id.strip_prefix("tcp:").and_then(|p| p.parse().ok())
}

/// The address this machine reaches a port at. Ashlar source may not write a
/// location down (B5), so every URL a page renders arrives from out here.
pub fn local_url(port: u16) -> String {
    format!("http://127.0.0.1:{}", port)
}

pub fn with_exposed(
    current: &BTreeMap<String, String>,
    port: u16,
    label: &str,
) -> BTreeMap<String, String> {
    let mut next = current.clone();
    next.insert(service_id(port), label.to_string());
    next
}

/// Removing what was never there is not an error: `unexpose` runs on the way
/// out of a run that may never have published.
pub fn without_exposed(
    current: &BTreeMap<String, String>,
    port: u16,
) -> BTreeMap<String, String> {
    let mut next = current.clone();
    next.remove(&service_id(port));
    next
}

pub fn exposed_map(value: &V) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    if let V::Map(m) = value {
        for (k, v) in m {
            out.insert(
                k.clone(),
                match v {
                    V::Text(t) => t.clone(),
                    _ => String::new(),
                },
            );
        }
    }
    out
}

/// One site in the shape `mesh.sites.Site` declares.
pub fn site(peer: &str, label: &str, url: &str) -> V {
    map(&[
        ("peer", V::Text(peer.to_string())),
        ("label", V::Text(label.to_string())),
        ("url", V::Text(url.to_string())),
    ])
}

/// Every site the peers in a snapshot advertise. A profile's `sites` are what
/// that peer chose to expose, so this reads an advert and never a scan of
/// somebody else's machine.
pub fn peer_sites(snapshot: &V) -> Vec<(String, u16, String)> {
    let mut out = Vec::new();
    let Some(V::List(peers)) = at(snapshot, "peers") else {
        return out;
    };
    for peer in &peers {
        let node = field(peer, "node");
        if node.is_empty() {
            continue;
        }
        let peer_label = field(peer, "label");
        let Some(V::List(sites)) = at(peer, "sites") else {
            continue;
        };
        for advert in &sites {
            let Some(V::Number(port)) = at(advert, "port") else {
                continue;
            };
            let label = {
                let l = field(advert, "label");
                if l.is_empty() {
                    format!("{} :{}", peer_label, port as u16)
                } else {
                    l
                }
            };
            out.push((node.clone(), port as u16, label));
        }
    }
    out
}

/// The local port a node's site is already mapped to. Asking first keeps
/// `nearby` from re-binding a port on every render.
pub fn existing_mapping(mappings: &V, node: &str, port: u16) -> Option<u16> {
    let V::List(rows) = mappings else { return None };
    rows.iter()
        .find(|m| {
            field(m, "node") == node
                && matches!(at(m, "port"), Some(V::Number(p)) if p as u16 == port)
        })
        .and_then(|m| match at(m, "localPort") {
            Some(V::Number(p)) => Some(p as u16),
            _ => None,
        })
}

/// One key out of a map value. `V` is a plain union with no accessor, and
/// every answer from a socket arrives as one, so this is the only way in.
pub fn at(value: &V, key: &str) -> Option<V> {
    match value {
        V::Map(m) => m.get(key).cloned(),
        _ => None,
    }
}

fn map(pairs: &[(&str, V)]) -> V {
    let mut m = BTreeMap::new();
    for (k, v) in pairs {
        m.insert(k.to_string(), v.clone());
    }
    V::Map(m)
}

fn field(value: &V, key: &str) -> String {
    match at(value, key) {
        Some(V::Text(t)) => t,
        Some(V::Number(n)) => to_json(&V::Number(n)),
        _ => String::new(),
    }
}

fn text_arg(args: &[V], index: usize) -> Result<String, String> {
    match args.get(index) {
        Some(V::Text(t)) => Ok(t.clone()),
        Some(_) => Err(format!("argument {} must be a text", index + 1)),
        None => Err(format!("this call wants at least {} argument(s)", index + 1)),
    }
}

fn port_arg(args: &[V], index: usize) -> Result<u16, String> {
    match args.get(index) {
        Some(V::Number(n)) if *n > 0.0 && *n <= u16::MAX as f64 => Ok(*n as u16),
        Some(_) => Err(format!("argument {} must be a port number", index + 1)),
        None => Err(format!("this call wants at least {} argument(s)", index + 1)),
    }
}

// ---------------------------------------------------------------------------
// The two sockets
// ---------------------------------------------------------------------------

/// Where each daemon keeps its socket.
///
/// `MYOWNMESH_HOME` moves a whole stack, which is how a second install runs
/// beside a first without either finding the other's socket. Both daemons read
/// it and they do NOT read it the same way: the mesh daemon treats it as its
/// data directory outright, while the node joins `.myownmesh` onto it. With
/// the variable unset — the ordinary case — both land in `~/.myownmesh`.
///
/// Each socket is therefore derived exactly as its owner derives it. Guessing
/// one rule for both would work on every developer's machine and fail on the
/// side-by-side install the variable exists for, which is the worst shape a
/// bug can have.
fn home(joins_dot_dir: bool) -> Option<PathBuf> {
    match std::env::var("MYOWNMESH_HOME") {
        Ok(h) if !h.trim().is_empty() => {
            let base = PathBuf::from(h.trim());
            Some(if joins_dot_dir { base.join(".myownmesh") } else { base })
        }
        _ => std::env::var("HOME")
            .ok()
            .filter(|h| !h.is_empty())
            .map(|h| PathBuf::from(h).join(".myownmesh")),
    }
}

/// The mesh daemon's directory (`MYOWNMESH_HOME` is the directory itself).
pub fn daemon_home() -> Option<PathBuf> {
    home(false)
}

/// The node's directory (`MYOWNMESH_HOME` is its parent).
pub fn node_home() -> Option<PathBuf> {
    home(true)
}

/// One request to the mesh daemon: one JSON line out, one line back. Its wire
/// is `{"op": …}` plus the op's own fields, answering `{ok, data, error}` —
/// the socket `myownmesh ctl` and its GUI already drive.
pub fn daemon(op: &str, args: BTreeMap<String, V>) -> Result<V, String> {
    let mut request = args;
    request.insert("op".to_string(), V::Text(op.to_string()));
    let line = to_json(&V::Map(request));
    let answer = unix::line_round_trip(
        &socket_path(daemon_home(), "daemon.sock")?,
        &line,
        "myownmesh",
        &["myownmesh", "serve"],
    )?;
    let Some(value) = from_json(&answer) else {
        return Err(format!("the mesh daemon's answer to `{}` was not JSON", op));
    };
    if matches!(at(&value, "ok"), Some(V::Bool(true))) {
        return Ok(at(&value, "data").unwrap_or(V::None));
    }
    Err(match at(&value, "error") {
        Some(V::Text(e)) => e,
        _ => format!("the mesh daemon refused `{}` without saying why", op),
    })
}

/// One request to the node: a length-prefixed JSON frame out, one frame back.
/// Its wire is `[u32 BE len][tag][{"cmd","args"}]`, answering
/// `{ok, result, error}` — the socket the node's own GUI drives.
pub fn node(cmd: &str, args: V) -> Result<V, String> {
    let mut body = BTreeMap::new();
    body.insert("cmd".to_string(), V::Text(cmd.to_string()));
    body.insert("args".to_string(), args);
    let payload = to_json(&V::Map(body));
    let answer = unix::frame_round_trip(
        &socket_path(node_home(), "allmystuff-node.sock")?,
        &payload,
        "allmystuff-serve",
        &["allmystuff-serve"],
    )?;
    let Some(value) = from_json(&answer) else {
        return Err(format!("the node's answer to `{}` was not JSON", cmd));
    };
    if matches!(at(&value, "ok"), Some(V::Bool(true))) {
        return Ok(at(&value, "result").unwrap_or(V::None));
    }
    Err(match at(&value, "error") {
        Some(V::Text(e)) => e,
        _ => format!("the node refused `{}` without saying why", cmd),
    })
}

fn socket_path(home: Option<PathBuf>, name: &str) -> Result<PathBuf, String> {
    home.map(|h| h.join(name))
        .ok_or_else(|| "no home directory, so no mesh socket to find".to_string())
}

/// The framing both round trips share, and the sidecar bring-up in front of
/// them. Unix only — see the module header.
#[cfg(unix)]
mod unix {
    use std::io::{BufRead, BufReader, Read, Write};
    use std::os::unix::net::UnixStream;
    use std::path::Path;

    /// Connect, or bring the sidecar up and connect. Reusing what is already
    /// running is the first move: a machine with the app open already has both
    /// daemons, and a second copy would fight the first over one identity.
    fn connect(socket: &Path, binary: &str, run: &[&str]) -> Result<UnixStream, String> {
        if let Ok(s) = UnixStream::connect(socket) {
            return Ok(s);
        }
        let spawned = std::process::Command::new(run[0])
            .args(&run[1..])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
        if spawned.is_err() {
            return Err(format!(
                "no mesh here: `{}` is not running and not on PATH. Install it, \
                 or bind this space in foreign.json to something that is.",
                binary
            ));
        }
        // A daemon takes a moment to bind. Wait in short steps rather than one
        // long sleep, so the common case — it is up almost at once — is fast.
        for _ in 0..40 {
            std::thread::sleep(std::time::Duration::from_millis(50));
            if let Ok(s) = UnixStream::connect(socket) {
                return Ok(s);
            }
        }
        Err(format!(
            "`{}` was started but never answered its socket at {}.",
            binary,
            socket.display()
        ))
    }

    /// One line out, one line back (the mesh daemon).
    pub fn line_round_trip(
        socket: &Path,
        line: &str,
        binary: &str,
        run: &[&str],
    ) -> Result<String, String> {
        let mut stream = connect(socket, binary, run)?;
        stream
            .write_all(line.as_bytes())
            .and_then(|_| stream.write_all(b"\n"))
            .and_then(|_| stream.flush())
            .map_err(|e| format!("could not write to {}: {}", binary, e))?;
        let mut answer = String::new();
        BufReader::new(&stream)
            .read_line(&mut answer)
            .map_err(|e| format!("could not read from {}: {}", binary, e))?;
        if answer.trim().is_empty() {
            return Err(format!("{} closed without answering", binary));
        }
        Ok(answer)
    }

    /// One frame out, one frame back (the node). `[u32 BE len][tag][payload]`,
    /// where the length counts the tag byte and tag 0 is JSON.
    pub fn frame_round_trip(
        socket: &Path,
        payload: &str,
        binary: &str,
        run: &[&str],
    ) -> Result<String, String> {
        let mut stream = connect(socket, binary, run)?;
        let bytes = payload.as_bytes();
        let len = (bytes.len() as u32) + 1;
        stream
            .write_all(&len.to_be_bytes())
            .and_then(|_| stream.write_all(&[0u8]))
            .and_then(|_| stream.write_all(bytes))
            .and_then(|_| stream.flush())
            .map_err(|e| format!("could not write to {}: {}", binary, e))?;

        let mut head = [0u8; 4];
        stream
            .read_exact(&mut head)
            .map_err(|e| format!("could not read from {}: {}", binary, e))?;
        let len = u32::from_be_bytes(head) as usize;
        if len == 0 {
            return Err(format!("{} sent an empty frame", binary));
        }
        // A frame ceiling before allocating: a length is untrusted input even
        // from a local socket.
        if len > 64 * 1024 * 1024 {
            return Err(format!("{} sent a frame past the 64MB ceiling", binary));
        }
        let mut body = vec![0u8; len];
        stream
            .read_exact(&mut body)
            .map_err(|e| format!("{}'s answer was truncated: {}", binary, e))?;
        if body[0] != 0 {
            return Err(format!("{} answered with a non-JSON frame", binary));
        }
        String::from_utf8(body[1..].to_vec())
            .map_err(|_| format!("{}'s answer was not UTF-8", binary))
    }
}

#[cfg(not(unix))]
mod unix {
    use std::path::Path;

    const WHY: &str = "the mesh sockets are named pipes on this platform, and \
                       this runtime has no client for them; bind the space in \
                       foreign.json to a worker that does.";

    pub fn line_round_trip(_: &Path, _: &str, _: &str, _: &[&str]) -> Result<String, String> {
        Err(WHY.to_string())
    }

    pub fn frame_round_trip(_: &Path, _: &str, _: &str, _: &[&str]) -> Result<String, String> {
        Err(WHY.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(s: &str) -> V {
        V::Text(s.to_string())
    }

    #[test]
    fn a_call_envelope_is_split_or_refused() {
        let (name, args) = parse_call(r#"{"call":"enter","args":["home","ada"]}"#).unwrap();
        assert_eq!(name, "enter");
        assert_eq!(args, vec![text("home"), text("ada")]);
        let (name, args) = parse_call(r#"{"call":"here"}"#).unwrap();
        assert_eq!(name, "here");
        assert!(args.is_empty());
        assert!(parse_call("[]").unwrap_err().contains("JSON object"));
        assert!(parse_call("{}").unwrap_err().contains("`call` name"));
        assert!(parse_call(r#"{"call":"x","args":1}"#).unwrap_err().contains("`args`"));
    }

    #[test]
    fn a_peer_is_here_only_when_traffic_can_flow() {
        let row = |status: &str| {
            peer_row(&map(&[
                ("device_id", text("n1")),
                ("label", text("ada")),
                ("status", text(status)),
            ]))
        };
        for live in ["active", "Active", "shelved"] {
            assert_eq!(at(&row(live), "here"), Some(V::Bool(true)), "{}", live);
        }
        for dead in ["sighted", "handshaking", "pending_approval", "reconnecting", "offline"] {
            assert_eq!(at(&row(dead), "here"), Some(V::Bool(false)), "{}", dead);
        }
    }

    #[test]
    fn an_unlabelled_peer_falls_back_to_its_id() {
        let row = peer_row(&map(&[("device_id", text("abc")), ("label", text(" "))]));
        assert_eq!(at(&row, "label"), Some(text("abc")));
        let row = peer_row(&map(&[("device_id", text("abc")), ("label", text(" ada "))]));
        assert_eq!(at(&row, "label"), Some(text("ada")));
        let row = peer_row(&map(&[]));
        assert_eq!(at(&row, "label"), Some(text("unknown")));
    }

    #[test]
    fn a_fingerprint_moves_on_change_and_not_on_asking() {
        let a = vec![peer_row(&map(&[("device_id", text("n1")), ("status", text("active"))]))];
        let b = vec![peer_row(&map(&[("device_id", text("n1")), ("status", text("active"))]))];
        assert_eq!(fingerprint(&a), fingerprint(&b), "asking twice is not a change");
        let c = vec![peer_row(&map(&[("device_id", text("n1")), ("status", text("offline"))]))];
        assert_ne!(fingerprint(&a), fingerprint(&c), "presence moving IS a change");
        let d = vec![
            peer_row(&map(&[("device_id", text("n1")), ("status", text("active"))])),
            peer_row(&map(&[("device_id", text("n2")), ("status", text("active"))])),
        ];
        assert_ne!(fingerprint(&a), fingerprint(&d), "a peer arriving IS a change");
    }

    #[test]
    fn exposing_leaves_every_other_selection_alone() {
        // The node's exposed map is its owner's choice about the whole
        // machine. Publishing adds one port; replacing the map would silently
        // unpublish whatever else was there.
        let mut current = BTreeMap::new();
        current.insert("tcp:3000".to_string(), "dev server".to_string());
        let after = with_exposed(&current, 8080, "site.app");
        assert_eq!(after.get("tcp:3000").map(String::as_str), Some("dev server"));
        assert_eq!(after.get("tcp:8080").map(String::as_str), Some("site.app"));
        assert_eq!(without_exposed(&after, 8080), current);
        assert_eq!(
            without_exposed(&current, 9999),
            current,
            "withdrawing what was never published is not an error"
        );
    }

    #[test]
    fn a_service_id_and_its_port_are_inverses() {
        assert_eq!(service_id(8080), "tcp:8080");
        assert_eq!(port_of("tcp:8080"), Some(8080));
        assert_eq!(port_of("udp:53"), None);
        assert_eq!(port_of("tcp:nope"), None);
    }

    #[test]
    fn peer_sites_read_the_advert_not_a_scan() {
        let snapshot = map(&[(
            "peers",
            V::List(vec![
                map(&[
                    ("node", text("n1")),
                    ("label", text("ada")),
                    (
                        "sites",
                        V::List(vec![
                            map(&[("label", text("pad")), ("port", V::Number(8080.0))]),
                            map(&[("label", text("")), ("port", V::Number(9000.0))]),
                        ]),
                    ),
                ]),
                map(&[("label", text("no id")), ("sites", V::List(vec![]))]),
            ]),
        )]);
        let sites = peer_sites(&snapshot);
        assert_eq!(sites.len(), 2, "a peer with no node id is skipped: {:?}", sites);
        assert_eq!(sites[0], ("n1".to_string(), 8080, "pad".to_string()));
        assert_eq!(sites[1], ("n1".to_string(), 9000, "ada :9000".to_string()));
    }

    #[test]
    fn a_mapping_is_reused_rather_than_rebound() {
        let mappings = V::List(vec![map(&[
            ("node", text("n1")),
            ("port", V::Number(8080.0)),
            ("localPort", V::Number(47001.0)),
        ])]);
        assert_eq!(existing_mapping(&mappings, "n1", 8080), Some(47001));
        assert_eq!(existing_mapping(&mappings, "n1", 9000), None);
        assert_eq!(existing_mapping(&mappings, "n2", 8080), None);
        assert_eq!(existing_mapping(&V::List(vec![]), "n1", 8080), None);
    }

    #[test]
    fn a_joined_network_is_recognised_either_way_it_is_keyed() {
        let networks = map(&[(
            "networks",
            V::List(vec![map(&[("id", text("home")), ("network_id", text("abc"))])]),
        )]);
        assert!(already_joined(&networks, "abc"));
        assert!(already_joined(&networks, "home"));
        assert!(!already_joined(&networks, "elsewhere"));
        assert!(!already_joined(&V::None, "abc"));
    }

    #[test]
    fn the_default_mesh_matches_what_the_library_ships_with() {
        assert_eq!(DEFAULT_NETWORK, "ashlar");
        assert_eq!(network_or_default(""), "ashlar");
        assert_eq!(network_or_default("  "), "ashlar");
        assert_eq!(network_or_default(" enclave "), "enclave");
    }

    #[test]
    fn a_join_is_open_so_the_mesh_id_is_the_secret() {
        let args = network_config("enclave", " demo ");
        let Some(V::Map(config)) = args.get("config") else {
            panic!("the config is nested under `config`");
        };
        assert_eq!(config.get("network_id"), Some(&text("enclave")));
        assert_eq!(config.get("kind"), Some(&text("open")));
        assert_eq!(config.get("auto_approve"), Some(&V::Bool(true)));
        assert_eq!(config.get("label"), Some(&text("demo")));
    }

    #[test]
    fn an_unknown_call_names_both_halves() {
        let mut s = Session::default();
        let e = s.dispatch("summarize", &[]).unwrap_err();
        assert!(e.contains("no such call"), "{}", e);
        assert!(e.contains("here, peers, enter"), "{}", e);
        assert!(e.contains("expose, unexpose"), "{}", e);
    }

    #[test]
    fn arguments_are_checked_before_a_socket_is_touched() {
        // A bad argument must not reach the daemon: the message an Ashlar call
        // site sees should be about the argument, not about a socket.
        let mut s = Session::default();
        assert!(s.dispatch("expose", &[text("8080")]).unwrap_err().contains("port number"));
        assert!(s.dispatch("expose", &[V::Number(0.0)]).unwrap_err().contains("port number"));
        assert!(s
            .dispatch("expose", &[V::Number(70000.0)])
            .unwrap_err()
            .contains("port number"));
        assert!(s.dispatch("unexpose", &[]).unwrap_err().contains("at least 1"));
        assert!(s.dispatch("enter", &[text("x")]).unwrap_err().contains("at least 2"));
    }

    #[test]
    fn each_socket_is_derived_the_way_its_owner_derives_it() {
        // The two daemons read MYOWNMESH_HOME differently, and a client that
        // invented one rule for both would work everywhere except the
        // side-by-side install the variable exists for.
        let both = (daemon_home(), node_home());
        match std::env::var("MYOWNMESH_HOME") {
            Ok(h) if !h.trim().is_empty() => {
                let base = std::path::PathBuf::from(h.trim());
                assert_eq!(both.0, Some(base.clone()));
                assert_eq!(both.1, Some(base.join(".myownmesh")));
            }
            _ => {
                // Unset: both land in the same place, which is why this is
                // easy to get wrong and only wrong where it matters.
                assert_eq!(both.0, both.1);
                assert!(both.0.is_some() || std::env::var("HOME").is_err());
            }
        }
    }
}
