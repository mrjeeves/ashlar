//! `ashlar mesh worker` — the adapter behind the `mesh` space (§9.10).
//!
//! The mesh is a capability, and the node that provides it already has a
//! client: one control socket, carrying JSON, driven by its own GUI and by
//! every app built on that stack. This module speaks that socket. **Nothing
//! outside this repository changes to make that work**, which is the whole
//! point — ADR-0017 exists because a boundary that requires the foreign system
//! to be re-authored for us is not a boundary, and shipping an Ashlar-shaped
//! adapter into someone else's daemon would have been that mistake with a
//! different face.
//!
//! One socket, because the node already answers both halves: the roster
//! (`mesh_identity`, `mesh_peers`, `mesh_networks`, `mesh_network_add`, which
//! it forwards to the mesh daemon it supervises) and the sites (`site_exposed`,
//! `site_set_exposed`, `site_map`, `site_mappings`, `session_snapshot`, which
//! need its proxy). Talking to the daemon separately would be a second wire
//! protocol for facts one already answers.
//!
//! It is reached as a sidecar: reuse whatever is already running on this
//! machine, and bring it up if nothing is. That is how every app on this stack
//! already behaves, and it is why an Ashlar site needs no setup beyond the
//! mesh being installed.
//!
//! The socket is a Unix socket or a Windows named pipe, and both are opened
//! with `std` alone — a pipe answers the ordinary file API, so there is no
//! platform where this is a stub.
//!
//! **The machine's identity is its owner's.** This adapter reads the node and
//! writes only what the program itself put there — a joined network, an
//! exposed port. An earlier build set the node's display label from an app's
//! setting, so starting an Ashlar site renamed its owner's node on every mesh
//! that node was on. `theirs_not_ours` is that mistake, refused by name.
//!
//! **A machine with no node is a fact, not a fault** (`Trouble::Absent`).
//! Reads answer around it — an empty roster that SAYS the node is missing —
//! so a site serves on a machine the mesh never reached. Only a deliberate
//! publish fails loudly, because `run --mesh` printed a promise. The earlier
//! build faulted on every read, which took a whole site down on any machine
//! whose node was closed, uninstalled, or (WSL) on the other side of the
//! kernel boundary.

use crate::eval::{from_json, to_json, to_text, V};
use std::collections::BTreeMap;
use std::io::{BufRead, Write};
use std::path::PathBuf;

/// The mesh an Ashlar site lands on when nothing names another. `lib/mesh`
/// defaults its `network` setting to the same word, so a program that says
/// nothing and a machine told nothing meet on one area rather than two.
pub const DEFAULT_NETWORK: &str = "ashlar";

/// The collection the roster lives in, as `lib/mesh` declares it. A push
/// names it, so every view that read a peer re-renders (§9.10).
pub const PEER_SHAPE: &str = "mesh.Peer";

/// The collection the messages live in, as `lib/mesh` declares it.
pub const SAID_SHAPE: &str = "mesh.Said";

/// The collection the room's offered files live in.
pub const OFFER_SHAPE: &str = "mesh.Offer";

/// Where a fetched file lands so the site can serve it: under the project's
/// own assets, which is where a `files` part reads from at request time. The
/// worker runs with the project root as its working directory, so this is the
/// one place it can put bytes that the program already knows how to serve.
pub const ROOM_FILES: &str = "assets/room";

/// One file somebody offered into the room.
///
/// A room's file lane is NOT the share subsystem: no person, no grant, no
/// durable relationship. The uploader mints a token whose allow-list is the
/// room's members, and the `:shared` route carries exactly one request —
/// fetch this token — which the uploader resolves against that list on every
/// request. Membership is the whole of the authorization, which is the same
/// sentence as "the mesh id is the invite".
#[derive(Debug, Clone, PartialEq)]
pub struct Offer {
    pub peer: String,
    pub who: String,
    pub token: String,
    pub name: String,
    pub size: f64,
    /// Where this machine can open it, once fetched. Empty until then.
    pub url: String,
}

impl Offer {
    fn value(&self) -> V {
        map(&[
            ("peer", V::Text(self.peer.clone())),
            ("who", V::Text(self.who.clone())),
            ("token", V::Text(self.token.clone())),
            ("name", V::Text(self.name.clone())),
            ("size", V::Number(self.size)),
            ("url", V::Text(self.url.clone())),
        ])
    }
}

/// One line somebody said, in the shape `mesh.Said` declares.
///
/// Plain data rather than a `V`: the log is written by the watch thread and
/// read by a call, and a `V` carries an `Rc` so it cannot cross threads. The
/// shape is built on the way out, once, where it is needed.
#[derive(Debug, Clone, PartialEq)]
pub struct Said {
    pub from: String,
    pub who: String,
    pub text: String,
    pub at: f64,
}

impl Said {
    fn value(&self) -> V {
        map(&[
            ("from", V::Text(self.from.clone())),
            ("who", V::Text(self.who.clone())),
            ("text", V::Text(self.text.clone())),
            ("at", V::Number(self.at)),
        ])
    }
}

/// How many messages a running site keeps. A room is a conversation, not an
/// archive: a program that wants history stores what it heard, which is a
/// `stored` property and not this worker's business.
pub const KEPT: usize = 200;

/// The room a mesh's members share, derived from the mesh's own name.
///
/// AllMyStuff's rooms have a host: it mints the id, states the roster, and
/// admits knocks. That is the right shape when a room is a subset of a mesh —
/// and the wrong one here, because an Ashlar app's mesh id IS the invite
/// already. Everyone holding it is in; nobody has to be admitted; there is no
/// host to be offline. So the room is derived from the mesh, every member
/// computes the same id from the name they already share, and the id nobody
/// else can guess is the one the program was written with (§9.12).
pub fn room_of(network: &str) -> String {
    format!("ashlar:{}", network)
}

/// A stable summary of the node's raw peer list, so an event that merely
/// re-states the session can be told from one that changed it.
pub fn roster_print(peers: &V) -> String {
    let list = match at(peers, "peers") {
        Some(V::List(l)) => l,
        _ => return String::new(),
    };
    let mut rows: Vec<String> = list
        .iter()
        .map(|p| format!("{}:{}", field(p, "device_id"), field(p, "status")))
        .collect();
    rows.sort();
    rows.join(",")
}

/// The unsolicited line: "this collection changed, nobody asked".
pub fn changed(shape: &str) -> String {
    let mut m = BTreeMap::new();
    m.insert("changed".to_string(), V::Text(shape.to_string()));
    to_json(&V::Map(m))
}

/// Run the worker: read one call per line, answer one per line, until stdin
/// closes. A failure crosses as `{"error": …}` and the runtime raises it at
/// the Ashlar call site with the message intact — worth more than a dead
/// co-process, so this exits zero for a failed call.
///
/// `socket` names where the node listens. Absent — the ordinary case — it is
/// derived and an absent node may be started; named, the caller has said where
/// the node is, so this connects to that and starts nothing.
pub fn run(socket: Option<PathBuf>) -> i32 {
    let stdin = std::io::stdin();
    let node = match socket {
        Some(path) => Node::at(path),
        None => Node::derived(),
    };
    // stdout is shared with the watch thread below, which pushes without
    // being asked. One lock, so two writers never interleave a line.
    let out = std::sync::Arc::new(std::sync::Mutex::new(std::io::stdout()));
    node.watch(out.clone());
    let mut session = Session::new(node);
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
        if say(&out, &answer).is_err() {
            return 0; // the runtime hung up; nothing left to answer
        }
    }
    0
}

/// Write one line to the runtime, under the lock the watch thread shares.
fn say(out: &std::sync::Mutex<std::io::Stdout>, line: &str) -> std::io::Result<()> {
    let mut out = out
        .lock()
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::Other, "stdout lock"))?;
    writeln!(out, "{}", line)?;
    out.flush()
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

/// What one worker remembers: the node it speaks to, the mesh this program
/// asked for, and the last roster it described. Everything that matters lives
/// in the daemon — a worker that died and respawned rejoins the same mesh and
/// sees the same peers.
pub struct Session {
    node: Node,
}

impl Default for Session {
    fn default() -> Session {
        Session::new(Node::derived())
    }
}

impl Session {
    pub fn new(node: Node) -> Session {
        Session { node }
    }

    /// One call in, one answer out.
    ///
    /// Reads answer even when no node is there, because a view calls them and
    /// a view that faults takes the page with it; the answer says the node is
    /// missing rather than pretending an empty mesh. `expose` and `unexpose`
    /// are the exception on purpose: they are a publish somebody asked for,
    /// and silence there would be a site nobody can reach reported as one
    /// anybody can.
    fn dispatch(&mut self, name: &str, args: &[V]) -> Result<V, String> {
        match name {
            // -- the roster (`mesh`) ---------------------------------------
            "here" => self.here().map_err(String::from),
            "peers" => Ok(V::List(soft(self.peers(), Vec::new())?)),
            "enter" => {
                let network = network_or_default(&text_arg(args, 0)?);
                let label = text_arg(args, 1)?;
                self.enter(&network, &label).map_err(String::from)
            }
            "networks" => Ok(V::List(soft(self.networks(), Vec::new())?)),
            // -- transmitting ----------------------------------------------
            "say" => {
                let line = text_arg(args, 0)?;
                self.say(&line).map_err(String::from)
            }
            "heard" => Ok(V::List(self.node.said())),
            "offer" => {
                let paths = match args.first() {
                    Some(V::List(p)) => p.clone(),
                    Some(other) => vec![other.clone()],
                    None => Vec::new(),
                };
                self.offer(&paths).map_err(String::from)
            }
            "offered" => Ok(V::List(self.node.shelf())),
            "fetch" => {
                let peer = text_arg(args, 0)?;
                let token = text_arg(args, 1)?;
                let name = text_arg(args, 2)?;
                self.fetch(&peer, &token, &name).map_err(String::from)
            }
            // -- the sites (`mesh.sites`) ----------------------------------
            "expose" => {
                let port = port_arg(args, 0)?;
                let label = text_arg(args, 1)?;
                let network = network_or_default(&text_arg(args, 2).unwrap_or_default());
                self.expose(port, &label, &network).map_err(String::from)
            }
            "unexpose" => {
                let port = port_arg(args, 0)?;
                let exposed = without_exposed(&self.exposed()?, port);
                self.set_exposed(exposed)?;
                Ok(V::Bool(true))
            }
            "published" => Ok(V::List(soft(self.published(), Vec::new())?)),
            "nearby" => Ok(V::List(soft(self.nearby(), Vec::new())?)),
            other => Err(format!(
                "no such call: `{}`. The mesh answers here, peers, networks, \
                 enter, say, heard, offer, offered, fetch; its sites answer \
                 expose, unexpose, \
                 published, nearby.",
                other
            )),
        }
    }

    /// The mesh in force: what `enter` was told, else the default area.
    fn network(&self) -> String {
        self.node.network()
    }

    // -- the roster ---------------------------------------------------------

    /// This node's own place — and, when there is no node on this machine,
    /// the fact that there is not, with the sentence that fixes it. A page
    /// renders either way: `reachable` is what the panel reads, and `note` is
    /// the correction it prints instead of a blank identity.
    fn here(&mut self) -> Result<V, Trouble> {
        match self.node.ask("mesh_identity", V::Map(BTreeMap::new())) {
            Ok(identity) => {
                let peers = self.peers().unwrap_or_default();
                let id = field(&identity, "device_id");
                // A node its owner never named still has to render as
                // something. The roster answers an unlabelled peer with its
                // id; this node is a peer to everyone else, so it answers the
                // same way rather than with a blank row.
                let label = {
                    let given = field(&identity, "label");
                    if given.trim().is_empty() {
                        id.clone()
                    } else {
                        given.trim().to_string()
                    }
                };
                Ok(place(&id, &label, &self.network(), peers.len(), true, ""))
            }
            Err(t) if t.absent() => Ok(place("", "", &self.network(), 0, false, t.why())),
            Err(t) => Err(t),
        }
    }

    fn peers(&self) -> Result<Vec<V>, Trouble> {
        let mut args = BTreeMap::new();
        args.insert("network".to_string(), V::Text(self.network()));
        let answer = self.node.ask("mesh_peers", V::Map(args))?;
        let Some(V::List(list)) = at(&answer, "peers") else {
            return Ok(Vec::new());
        };
        Ok(list.iter().map(peer_row).collect())
    }

    /// Every mesh this node is on, with how many peers are on each.
    ///
    /// `here` answers for the mesh THIS worker entered, which is process
    /// state: a fresh `ashlar mesh` never called `enter`, so it would answer
    /// for the default area and report zero peers while the node had an
    /// active one three lines away. That is the quiet-wrong this language
    /// refuses, so the report asks the node what it is actually on.
    fn networks(&self) -> Result<Vec<V>, Trouble> {
        let answer = self.node.ask("mesh_networks", V::Map(BTreeMap::new()))?;
        let list = match at(&answer, "networks") {
            Some(V::List(l)) => l,
            _ => match answer {
                V::List(l) => l,
                _ => Vec::new(),
            },
        };
        let mut out = Vec::new();
        for n in &list {
            let id = {
                let wire = field(n, "network_id");
                if wire.is_empty() { field(n, "id") } else { wire }
            };
            if id.is_empty() {
                continue;
            }
            let mut args = BTreeMap::new();
            args.insert("network".to_string(), V::Text(id.clone()));
            let peers = match self.node.ask("mesh_peers", V::Map(args)) {
                Ok(v) => match at(&v, "peers") {
                    Some(V::List(p)) => p.len(),
                    _ => 0,
                },
                Err(_) => 0,
            };
            out.push(map(&[
                ("id", V::Text(id)),
                ("peers", V::Number(peers as f64)),
            ]));
        }
        Ok(out)
    }

    /// Join the mesh this program named. Joining is the whole of arriving:
    /// the node's own name, its other networks and everything else about the
    /// machine are its owner's and are left exactly as they were.
    fn enter(&mut self, network: &str, label: &str) -> Result<V, Trouble> {
        let joined = self
            .node
            .ask("mesh_networks", V::Map(BTreeMap::new()))
            .map(|v| already_joined(&v, network))
            .unwrap_or(false);
        self.node.set_network(network);
        if !joined {
            match self
                .node
                .ask("mesh_network_add", V::Map(network_config(network, label)))
            {
                Ok(_) => {}
                // No node to join: `here` says so, and the site serves. This
                // runs from a `start` stack, and a fault there is a program
                // that will not start at all.
                Err(t) if t.absent() => return self.here(),
                Err(t) => return Err(t),
            }
        }
        self.here()
    }

    // -- transmitting -------------------------------------------------------

    /// Say one line to everyone on this mesh.
    ///
    /// The node routes to members; the members are the roster. A peer that is
    /// not connected right now does not receive it and there is no store to
    /// hold it for them — this is a conversation between the people who are
    /// here, which is what "serverless" costs and what it buys.
    ///
    /// The sender keeps its own line rather than waiting to hear itself: the
    /// node delivers to members, and a machine is not its own peer.
    fn say(&mut self, line: &str) -> Result<V, Trouble> {
        let line = line.trim();
        if line.is_empty() {
            return Ok(V::Bool(false));
        }
        let identity = self
            .node
            .ask("mesh_identity", V::Map(BTreeMap::new()))
            .unwrap_or(V::None);
        let me = field(&identity, "device_id");
        let mine = match field(&identity, "label").trim() {
            "" => short(&canonical(&me)),
            named => named.to_string(),
        };
        let members: Vec<V> = self
            .peers()?
            .iter()
            .map(|p| V::Text(field(p, "id")))
            .filter(|id| !matches!(id, V::Text(t) if t.is_empty()))
            .collect();
        let mut message = BTreeMap::new();
        message.insert("room".to_string(), V::Text(room_of(&self.network())));
        message.insert("name".to_string(), V::Text(self.node.network()));
        message.insert("kind".to_string(), V::Text("chat".to_string()));
        message.insert("text".to_string(), V::Text(line.to_string()));
        let mut args = BTreeMap::new();
        args.insert("members".to_string(), V::List(members.clone()));
        args.insert("message".to_string(), V::Map(message));
        // Nobody else here yet is not a failure to speak: the line is still
        // this machine's, and a room of one is a room.
        if !members.is_empty() {
            self.node.ask("room_send", V::Map(args))?;
        }
        self.node.keep(Said {
            who: mine,
            from: me,
            text: line.to_string(),
            at: now_ms(),
        });
        Ok(V::Bool(true))
    }

    /// Offer files to the room. The token's allow-list is the roster, so
    /// membership is the whole of the authorization — no grant, no person,
    /// nothing durable to revoke afterwards. Restating the offer with fewer
    /// paths is how a member takes one back.
    fn offer(&mut self, paths: &[V]) -> Result<V, Trouble> {
        let members = self.member_ids()?;
        let mut args = BTreeMap::new();
        args.insert("members".to_string(), V::List(members.clone()));
        args.insert(
            "paths".to_string(),
            V::List(paths.iter().map(|p| V::Text(as_text(p))).collect()),
        );
        let minted = match self.node.ask("room_share_files", V::Map(args))? {
            V::List(files) => files,
            _ => Vec::new(),
        };
        // Tell the room. There is no host to aggregate, so every member is
        // told directly and keeps the shelf itself.
        let mut message = BTreeMap::new();
        message.insert("room".to_string(), V::Text(room_of(&self.network())));
        message.insert("name".to_string(), V::Text(self.network()));
        message.insert("kind".to_string(), V::Text("share_list".to_string()));
        message.insert("files".to_string(), V::List(minted.clone()));
        if !members.is_empty() {
            let mut send = BTreeMap::new();
            send.insert("members".to_string(), V::List(members));
            send.insert("message".to_string(), V::Map(message));
            self.node.ask("room_send", V::Map(send))?;
        }
        // And keep our own, so the page shows what this machine put up.
        let me = self
            .node
            .ask("mesh_identity", V::Map(BTreeMap::new()))
            .map(|v| field(&v, "device_id"))
            .unwrap_or_default();
        self.node.offered_by(&me, &minted);
        Ok(V::List(self.node.shelf()))
    }

    /// Pull one offered file here, and answer where this machine can open it.
    ///
    /// The route carries one request — fetch this token — and the uploader
    /// checks the token against the members it shared with, per request. The
    /// bytes land in this machine's Downloads folder, because that is what
    /// the node does with a fetch; copying them under the project's assets is
    /// what makes them servable, since a `files` part reads that directory at
    /// request time.
    fn fetch(&mut self, peer: &str, token: &str, name: &str) -> Result<V, Trouble> {
        let me = self
            .node
            .ask("mesh_identity", V::Map(BTreeMap::new()))
            .map(|v| field(&v, "device_id"))
            .unwrap_or_default();
        // The route runs FROM the source TO the sink, and the source is an
        // endpoint handle, not a bare node: `<peer>:shared` is what marks the
        // lane fetch-only, and the node checks that the fetcher is the route's
        // `to`. Opening it the other way round yields a route that exists and
        // refuses every request on it.
        let mut open = BTreeMap::new();
        open.insert(
            "from".to_string(),
            V::Text(format!("{}:shared", self.node.addressable(peer))),
        );
        open.insert("to".to_string(), V::Text(me));
        open.insert("media".to_string(), V::Text("shared".to_string()));
        let route = match self.node.ask("connect_route", V::Map(open))? {
            V::Text(id) => id,
            other => field(&other, "route_id"),
        };
        if route.is_empty() {
            return Err(Trouble::Refused(
                "the node opened no route to that peer.".to_string(),
            ));
        }
        // The sink is registered BEFORE the request, so the first chunk
        // cannot race it — the node's own comment, and its own ordering.
        let req = 1.0;
        let mut sink = BTreeMap::new();
        sink.insert("route_id".to_string(), V::Text(route.clone()));
        sink.insert("req".to_string(), V::Number(req));
        sink.insert("name".to_string(), V::Text(name.to_string()));
        let _landed = as_text(&self.node.ask("file_download", V::Map(sink))?);

        let mut event = BTreeMap::new();
        event.insert("kind".to_string(), V::Text("fetch".to_string()));
        event.insert("req".to_string(), V::Number(req));
        event.insert("token".to_string(), V::Text(token.to_string()));
        let mut send = BTreeMap::new();
        send.insert("route_id".to_string(), V::Text(route.clone()));
        send.insert("event".to_string(), V::Map(event));
        // A route is offered, then accepted: it exists before it is usable,
        // and the node says so rather than queueing. Ask again until it takes.
        let mut asked = Err(Trouble::Refused("never asked".to_string()));
        for _ in 0..100 {
            asked = self.node.ask("file_send", V::Map(send.clone()));
            match &asked {
                Err(Trouble::Refused(why)) if why.contains("active") => {
                    std::thread::sleep(std::time::Duration::from_millis(100));
                }
                _ => break,
            }
        }
        asked?;

        let url = self.node.await_landing(&route, req, name)?;
        let mut close = BTreeMap::new();
        close.insert("route_id".to_string(), V::Text(route));
        let _ = self.node.ask("disconnect_route", V::Map(close));
        self.node.fetched(peer, token, &url);
        Ok(V::Text(url))
    }

    /// The roster's ids, which are the room's members.
    fn member_ids(&self) -> Result<Vec<V>, Trouble> {
        Ok(self
            .peers()?
            .iter()
            .map(|p| V::Text(field(p, "id")))
            .filter(|id| !matches!(id, V::Text(t) if t.is_empty()))
            .collect())
    }

    // -- the sites ----------------------------------------------------------

    fn exposed(&self) -> Result<BTreeMap<String, String>, Trouble> {
        Ok(exposed_map(
            &self.node.ask("site_exposed", V::Map(BTreeMap::new()))?,
        ))
    }

    fn set_exposed(&self, exposed: BTreeMap<String, String>) -> Result<(), Trouble> {
        let mut inner = BTreeMap::new();
        for (k, v) in exposed {
            inner.insert(k, V::Text(v));
        }
        let mut args = BTreeMap::new();
        args.insert("exposed".to_string(), V::Map(inner));
        self.node.ask("site_set_exposed", V::Map(args)).map(|_| ())
    }

    fn expose(&mut self, port: u16, label: &str, network: &str) -> Result<V, Trouble> {
        // The node keeps its own list of networks; joining there is what puts
        // the site's proxy on the same mesh the roster is read from.
        let joined = self
            .node
            .ask("mesh_networks", V::Map(BTreeMap::new()))
            .map(|v| already_joined(&v, network))
            .unwrap_or(false);
        if !joined {
            self.node
                .ask("mesh_network_add", V::Map(network_config(network, label)))?;
        }
        // Add one port to the owner's exposed selection and leave the rest
        // alone. The node's proxy refuses every port outside that selection,
        // so this is the whole of publishing — and it cannot reach a service
        // its owner never exposed.
        let exposed = with_exposed(&self.exposed()?, port, label);
        self.set_exposed(exposed)?;
        self.node.set_network(network);
        let identity = self
            .node
            .ask("mesh_identity", V::Map(BTreeMap::new()))
            .unwrap_or(V::None);
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
    fn published(&self) -> Result<Vec<V>, Trouble> {
        let exposed = self.exposed()?;
        let me = self
            .node
            .ask("mesh_identity", V::Map(BTreeMap::new()))
            .map(|v| field(&v, "device_id"))
            .unwrap_or_default();
        let me = if me.is_empty() { "this node".to_string() } else { me };
        let mut out = Vec::new();
        for (id, label) in &exposed {
            let Some(port) = port_of(id) else { continue };
            let shown = if label.is_empty() { id.clone() } else { label.clone() };
            out.push(site(&me, &shown, &local_url(port)));
        }
        Ok(out)
    }

    /// The peers' sites, each with an address this machine can open. A peer's
    /// advert says what it serves; mapping it binds a local port the node
    /// proxies over the mesh, so the link a page renders is ordinary loopback.
    fn nearby(&self) -> Result<Vec<V>, Trouble> {
        let snapshot = self.node.ask("session_snapshot", V::Map(BTreeMap::new()))?;
        // Presence reaches every network this node joined, so the snapshot
        // mixes peers from all of them. Keep the ones on THIS mesh — the
        // roster already names them — rather than showing another mesh's
        // links under this one's heading.
        let mine: Vec<String> = self
            .peers()?
            .iter()
            .map(|p| canonical(&field(p, "id")))
            .filter(|id| !id.is_empty())
            .collect();
        let mappings = self
            .node
            .ask("site_mappings", V::Map(BTreeMap::new()))
            .unwrap_or(V::List(vec![]));
        let mut out = Vec::new();
        for (peer, port, label) in peer_sites(&snapshot) {
            if !mine.contains(&canonical(&peer)) {
                continue;
            }
            let local = match existing_mapping(&mappings, &peer, port) {
                Some(p) => Some(p),
                None => {
                    let mut args = BTreeMap::new();
                    args.insert("node".to_string(), V::Text(peer.clone()));
                    args.insert("port".to_string(), V::Number(port as f64));
                    self.node.ask("site_map", V::Map(args)).ok().and_then(|v| {
                        match at(&v, "localPort") {
                            Some(V::Number(n)) => Some(n as u16),
                            _ => None,
                        }
                    })
                }
            };
            // A site that will not map is still a site the peer is running.
            // "There but unreachable from here" is a different fact from "not
            // there", and dropping it would report the second.
            let url = local.map(local_url).unwrap_or_default();
            out.push(site(&peer, &label, &url));
        }
        Ok(out)
    }
}

/// What went wrong, and whether it is about this machine or about the answer.
///
/// The distinction is the whole of "a missing mesh is not a broken site":
/// `Absent` means nothing is listening and nothing could be started, which a
/// read answers around; `Refused` means a node answered and what it said was
/// wrong, which stays a fault. Collapsing the two would either take a site
/// down for having no mesh, or swallow a real protocol break as an empty list.
#[derive(Debug, Clone, PartialEq)]
pub enum Trouble {
    Absent(String),
    Refused(String),
}

impl Trouble {
    pub fn absent(&self) -> bool {
        matches!(self, Trouble::Absent(_))
    }

    pub fn why(&self) -> &str {
        match self {
            Trouble::Absent(w) | Trouble::Refused(w) => w,
        }
    }
}

impl From<Trouble> for String {
    fn from(t: Trouble) -> String {
        match t {
            Trouble::Absent(w) | Trouble::Refused(w) => w,
        }
    }
}

/// A read's answer when this machine has no node: the empty one, not a fault.
fn soft<T>(answer: Result<T, Trouble>, empty: T) -> Result<T, String> {
    match answer {
        Ok(v) => Ok(v),
        Err(t) if t.absent() => Ok(empty),
        Err(t) => Err(t.into()),
    }
}

/// This node's own place, in the shape `mesh.Here` declares.
fn place(id: &str, label: &str, network: &str, peers: usize, reachable: bool, note: &str) -> V {
    map(&[
        ("id", V::Text(id.to_string())),
        ("label", V::Text(label.to_string())),
        ("network", V::Text(network.to_string())),
        ("peers", V::Number(peers as f64)),
        ("reachable", V::Bool(reachable)),
        ("note", V::Text(note.to_string())),
    ])
}

// ---------------------------------------------------------------------------
// Pure decisions — everything above the sockets
// ---------------------------------------------------------------------------

/// Any value as the text a command or a path wants.
pub fn as_text(v: &V) -> String {
    match v {
        V::Text(t) => t.clone(),
        V::None => String::new(),
        other => to_json(other),
    }
}

/// The frames one poll returned: `[u32 le len][frame json]…`, which is the
/// node's own framing for a window draining its buffered file responses.
pub fn file_frames(polled: &V) -> Vec<V> {
    let text = match polled {
        V::Text(t) => t.clone(),
        V::List(items) => {
            // A JSON transport hands the bytes back as numbers; rebuild them.
            let bytes: Vec<u8> = items
                .iter()
                .filter_map(|n| match n {
                    V::Number(n) => Some(*n as u8),
                    _ => None,
                })
                .collect();
            return frames_from_bytes(&bytes);
        }
        _ => return Vec::new(),
    };
    frames_from_bytes(text.as_bytes())
}

pub fn frames_from_bytes(bytes: &[u8]) -> Vec<V> {
    let mut out = Vec::new();
    let mut at = 0usize;
    while at + 4 <= bytes.len() {
        let len = u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
            as usize;
        at += 4;
        if len == 0 || at + len > bytes.len() {
            break;
        }
        if let Ok(text) = std::str::from_utf8(&bytes[at..at + len]) {
            if let Some(v) = from_json(text) {
                out.push(v);
            }
        }
        at += len;
    }
    out
}

/// Put fetched bytes where the site can serve them. The node writes a fetch
/// into this machine's Downloads folder; a page can only link what the
/// program's own assets carry, and a `files` part reads that directory at
/// request time — so the copy is what turns "downloaded" into "openable".
pub fn place_under_assets(landed: &str, name: &str) -> Result<String, String> {
    let safe = std::path::Path::new(name)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| "file".to_string());
    let dir = std::path::PathBuf::from(ROOM_FILES);
    std::fs::create_dir_all(&dir).map_err(|e| format!("could not make {}: {}", dir.display(), e))?;
    std::fs::copy(landed, dir.join(&safe))
        .map_err(|e| format!("could not place `{}`: {}", safe, e))?;
    Ok(format!("/room/{}", safe))
}

/// A key at reading length. Enough to tell two people apart in a room, and
/// not so much that the room is a wall of base32.
pub fn short(id: &str) -> String {
    match id.char_indices().nth(8) {
        Some((cut, _)) => format!("{}…", &id[..cut]),
        None => id.to_string(),
    }
}

/// A device id without its display suffix. The daemon's roster answers bare
/// pubkeys and a presence advert carries the `pubkey-SUFFIX` display form, so
/// the two only compare after this.
pub fn canonical(id: &str) -> String {
    match id.split_once('-') {
        Some((pubkey, _)) => pubkey.to_string(),
        None => id.to_string(),
    }
}

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

/// Milliseconds since the epoch, for stamping a line as it arrives. A worker
/// is a program like any other; `now()` inside Ashlar is the runtime's, and
/// this is the same clock read on the other side of the boundary.
pub fn now_ms() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as f64)
        .unwrap_or(0.0)
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

/// How to start the node, in the order a machine is likely to have it. The
/// node ships as a binary on PATH, as a subcommand of the app's CLI, and
/// bundled beside an app that embeds it — this looks for all three rather than
/// declaring the mesh absent because one name was not on PATH.
///
/// `ASHLAR_MESH_NODE` overrides the search with a command of your own, which
/// is the same relationship every other binding has to its derived default.
pub fn bring_up() -> Vec<String> {
    if let Ok(cmd) = std::env::var("ASHLAR_MESH_NODE") {
        let argv: Vec<String> = cmd.split_whitespace().map(str::to_string).collect();
        if !argv.is_empty() {
            return argv;
        }
    }
    let exe = if cfg!(windows) { ".exe" } else { "" };
    // Beside this binary first: an app that ships the node as a sidecar puts
    // it there, which is how CEC Support and the desktop app already work.
    if let Ok(here) = std::env::current_exe() {
        if let Some(dir) = here.parent() {
            let side = dir.join(format!("allmystuff-serve{}", exe));
            if side.is_file() {
                return vec![side.to_string_lossy().to_string()];
            }
        }
    }
    for dir in install_dirs() {
        let served = dir.join(format!("allmystuff-serve{}", exe));
        if served.is_file() {
            return vec![served.to_string_lossy().to_string()];
        }
        let cli = dir.join(format!("allmystuff{}", exe));
        if cli.is_file() {
            return vec![cli.to_string_lossy().to_string(), "serve".to_string()];
        }
    }
    // Nothing found on disk: let PATH answer, and let the failure name it.
    vec!["allmystuff-serve".to_string()]
}

/// Where the mesh comes from when the machine has none: the vendor's own
/// installer, run verbatim.
///
/// It downloads verified binaries, puts them on `PATH`, and sets up the mesh
/// daemon the node runs on — which is more than this toolchain could do
/// without a TLS client it does not have and a release channel it does not
/// own. Re-implementing any of that would be a second, worse installer that
/// goes stale the first time theirs changes.
pub const INSTALLER: &str = "https://allmystuff.works/install.sh";
pub const INSTALLER_WINDOWS: &str = "https://allmystuff.works/install.ps1";

/// The command `ashlar mesh install` runs, and prints before running.
///
/// This fetches a script and executes it, which is what the vendor documents
/// and what every install of this kind does. It is therefore a command
/// somebody types, never something `run` does on their behalf: a site that
/// silently downloaded and executed a remote script because a page was opened
/// would be a different and much worse program.
pub fn install_command() -> Vec<String> {
    if cfg!(windows) {
        vec![
            "powershell".to_string(),
            "-NoProfile".to_string(),
            "-Command".to_string(),
            format!("irm {} | iex", INSTALLER_WINDOWS),
        ]
    } else {
        vec![
            "sh".to_string(),
            "-c".to_string(),
            format!("curl -fsSL {} | sh", INSTALLER),
        ]
    }
}

/// Whether a node answers right now — asked before installing, so a machine
/// that already has one is told rather than re-installed.
pub fn node_answers() -> bool {
    Node::derived().listening()
}

/// Where the installers put it. The Unix installer writes `/usr/local/bin` or
/// `~/.local/bin`; the Windows one `%LOCALAPPDATA%\Programs`; the desktop app
/// carries the node inside its bundle.
fn install_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    let home = std::env::var("HOME")
        .ok()
        .or_else(|| std::env::var("USERPROFILE").ok())
        .filter(|h| !h.is_empty())
        .map(PathBuf::from);
    if cfg!(windows) {
        for var in ["LOCALAPPDATA", "PROGRAMFILES", "ProgramFiles(x86)"] {
            if let Ok(base) = std::env::var(var) {
                if !base.is_empty() {
                    dirs.push(PathBuf::from(&base).join("Programs").join("AllMyStuff"));
                    dirs.push(PathBuf::from(&base).join("AllMyStuff"));
                }
            }
        }
    } else {
        dirs.push(PathBuf::from("/usr/local/bin"));
        dirs.push(PathBuf::from("/usr/bin"));
        if let Some(h) = &home {
            dirs.push(h.join(".local").join("bin"));
        }
        if cfg!(target_os = "macos") {
            dirs.push(PathBuf::from(
                "/Applications/AllMyStuff.app/Contents/MacOS",
            ));
            if let Some(h) = &home {
                dirs.push(h.join("Applications/AllMyStuff.app/Contents/MacOS"));
            }
        }
    }
    dirs
}

/// Where the node listens: `$MYOWNMESH_HOME/.myownmesh/`, else
/// `$HOME/.myownmesh/`, which is how the node itself derives it.
///
/// Read it the node's way rather than inventing a second convention — and do
/// not read it as "the way to run two stacks side by side". It is not: the
/// node resolves this variable as the PARENT of `.myownmesh` while the mesh
/// daemon treats it as that directory outright, so setting it leaves the node
/// unable to find the daemon it just spawned. CEC Support hit that and stopped
/// forking the variable (its `apply_cec_env` says why). Isolating a stack
/// means isolating `HOME`.
pub fn node_socket() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        // A named pipe has no directory; the name is the address.
        return Some(PathBuf::from(r"\\.\pipe\allmystuff-node"));
    }
    #[cfg(not(windows))]
    {
        let home = match std::env::var("MYOWNMESH_HOME") {
            Ok(h) if !h.trim().is_empty() => PathBuf::from(h.trim()).join(".myownmesh"),
            _ => PathBuf::from(std::env::var("HOME").ok().filter(|h| !h.is_empty())?)
                .join(".myownmesh"),
        };
        Some(home.join("allmystuff-node.sock"))
    }
}

/// The node this worker speaks to: where it listens, and whether this process
/// may start one that isn't.
///
/// Derived, it is the machine's own node and an absent one may be brought up —
/// the sidecar rule every app on this stack follows. Named, somebody has said
/// where the node is (`foreign.json`, ADR-0017), so this connects to exactly
/// that and starts nothing: naming a socket and getting a spawned daemon is
/// the opposite of what naming it meant.
pub struct Node {
    socket: Option<PathBuf>,
    start: Option<Vec<String>>,
    /// The mesh in force. On the node rather than the session because the
    /// watch thread reads the same roster the views do.
    network: std::sync::Arc<std::sync::Mutex<String>>,
    /// What this site has heard, newest last. Shared for the same reason:
    /// messages arrive on the watch thread and are read by a view.
    heard: std::sync::Arc<std::sync::Mutex<Vec<Said>>>,
    /// What the room is offering, keyed by `peer|token` so a member's
    /// re-statement replaces its own entries and nobody else's.
    offers: std::sync::Arc<std::sync::Mutex<BTreeMap<String, Offer>>>,
    /// Finished transfers, keyed `route|req`. A fetch's chunks stream
    /// straight to disk and never reach a poll queue, so the node says it
    /// landed the only way it can: an event, on the stream already open.
    saved: std::sync::Arc<std::sync::Mutex<BTreeMap<String, Result<String, String>>>>,
}

impl Node {
    pub fn derived() -> Node {
        Node {
            socket: node_socket(),
            start: Some(bring_up()),
            network: Node::area(),
            heard: Node::log(),
            offers: Node::empty_shelf(),
            saved: Node::landings(),
        }
    }

    pub fn at(socket: PathBuf) -> Node {
        Node {
            socket: Some(socket),
            start: None,
            network: Node::area(),
            heard: Node::log(),
            offers: Node::empty_shelf(),
            saved: Node::landings(),
        }
    }

    fn landings() -> std::sync::Arc<std::sync::Mutex<BTreeMap<String, Result<String, String>>>> {
        std::sync::Arc::new(std::sync::Mutex::new(BTreeMap::new()))
    }

    fn empty_shelf() -> std::sync::Arc<std::sync::Mutex<BTreeMap<String, Offer>>> {
        std::sync::Arc::new(std::sync::Mutex::new(BTreeMap::new()))
    }

    fn log() -> std::sync::Arc<std::sync::Mutex<Vec<Said>>> {
        std::sync::Arc::new(std::sync::Mutex::new(Vec::new()))
    }

    fn area() -> std::sync::Arc<std::sync::Mutex<String>> {
        std::sync::Arc::new(std::sync::Mutex::new(DEFAULT_NETWORK.to_string()))
    }

    /// Follow the node's event stream for as long as this worker lives, and
    /// push a line to the runtime whenever the roster moved.
    ///
    /// The node already streams what its own GUI redraws on: one connection
    /// carrying `__subscribe_events`, then a frame per event. So a roster is
    /// not something to ask about on a timer — the earlier build polled every
    /// three seconds, which is both late and, on a quiet mesh, entirely
    /// wasted. Presence arrives when it arrives, and the page follows it.
    ///
    /// A machine with no node has nothing to subscribe to. That is not a
    /// failure here: the reads already answer (`Trouble::Absent`), so this
    /// simply retries, quietly, in case a node appears later — which on a
    /// laptop that opens the app after the site is up, it does.
    pub fn watch(&self, out: std::sync::Arc<std::sync::Mutex<std::io::Stdout>>) {
        if self.socket.is_none() {
            return;
        }
        let node = Node {
            socket: self.socket.clone(),
            start: None, // a listener never starts a daemon
            network: self.network.clone(),
            heard: self.heard.clone(),
            offers: self.offers.clone(),
            saved: self.saved.clone(),
        };
        let socket = self.socket.clone().expect("checked");
        std::thread::spawn(move || {
            let mut last: Option<String> = None;
            loop {
                wire::follow(&socket, &mut |event, payload| match event {
                    // The node re-states its session on a timer, not only when
                    // something moved. Forwarding every one would re-render
                    // every connected page every few seconds forever — the
                    // poll this replaced, wearing the node's clothes. So the
                    // event is the cue to LOOK, and the roster decides whether
                    // anyone is told.
                    "allmystuff://session" => {
                        let now = roster_print(&node.roster());
                        if last.as_deref() != Some(now.as_str()) {
                            last = Some(now);
                            let _ = say(&out, &changed(PEER_SHAPE));
                        }
                    }
                    // Somebody said something. Every one of these is news by
                    // construction — a message is not a state that can be
                    // re-stated — so there is nothing to compare it against.
                    "allmystuff://room" => {
                        if let Some(shape) = node.remember(payload) {
                            let _ = say(&out, &changed(shape));
                        }
                    }
                    // A fetch finished (or failed). The chunks went to disk
                    // without passing through any queue, so this event is the
                    // only place the outcome exists.
                    "allmystuff://file-saved" => {
                        let key = format!(
                            "{}|{}",
                            field(payload, "route"),
                            field(payload, "req")
                        );
                        let outcome = match at(payload, "error") {
                            Some(V::Text(why)) if !why.is_empty() => Err(why),
                            _ => Ok(field(payload, "path")),
                        };
                        if let Ok(mut done) = node.saved.lock() {
                            done.insert(key, outcome);
                        }
                    }
                    _ => {}
                });
                // The node went away or was never there. Wait before asking
                // again: a tight loop against a missing socket is a busy wait
                // with a nicer name.
                std::thread::sleep(std::time::Duration::from_secs(5));
            }
        });
    }

    /// The mesh in force: what `enter` was told, else the default area. Shared
    /// with the watch thread, which needs the same answer to ask the node for
    /// the same roster the views are reading.
    pub fn network(&self) -> String {
        match self.network.lock() {
            Ok(n) => n.clone(),
            Err(_) => DEFAULT_NETWORK.to_string(),
        }
    }

    pub fn set_network(&self, network: &str) {
        if let Ok(mut n) = self.network.lock() {
            *n = network.to_string();
        }
    }

    /// Is anything listening? Asked without starting one — the question is
    /// whether this machine HAS a mesh, and spawning a daemon to answer it
    /// would make every asking true.
    pub fn listening(&self) -> bool {
        match &self.socket {
            Some(path) => Node::at(path.clone())
                .ask("mesh_identity", V::Map(BTreeMap::new()))
                .is_ok(),
            None => false,
        }
    }

    /// The node's raw peer list for the mesh in force, or nothing.
    fn roster(&self) -> V {
        let mut args = BTreeMap::new();
        args.insert("network".to_string(), V::Text(self.network()));
        self.ask("mesh_peers", V::Map(args)).unwrap_or(V::None)
    }

    /// Record what a member says it is offering. Replacement semantics per
    /// member, which is what the protocol states: the list a member sends is
    /// its whole current list, so a file it stopped offering drops off.
    fn offered_by(&self, peer: &str, files: &[V]) {
        let who = self.who(peer);
        let mine = canonical(peer);
        if let Ok(mut shelf) = self.offers.lock() {
            shelf.retain(|_, o| canonical(&o.peer) != mine);
            for f in files {
                let token = field(f, "token");
                if token.is_empty() {
                    continue;
                }
                shelf.insert(
                    format!("{}|{}", mine, token),
                    Offer {
                        peer: peer.to_string(),
                        who: who.clone(),
                        name: field(f, "name"),
                        size: match at(f, "size") {
                            Some(V::Number(n)) => n,
                            _ => 0.0,
                        },
                        url: String::new(),
                        token,
                    },
                );
            }
        }
    }

    fn shelf(&self) -> Vec<V> {
        match self.offers.lock() {
            Ok(shelf) => shelf.values().map(Offer::value).collect(),
            Err(_) => Vec::new(),
        }
    }

    /// Wait for the node to say where a fetch landed, then put the bytes
    /// where the site can serve them.
    ///
    /// Chunks stream straight to disk — `feed_download` consumes them and
    /// never queues them — so there is nothing to poll for. The outcome
    /// arrives as an event on the stream this worker already follows.
    fn await_landing(&self, route: &str, req: f64, name: &str) -> Result<String, Trouble> {
        let key = format!("{}|{}", route, to_text(&V::Number(req)));
        for _ in 0..600 {
            let outcome = self.saved.lock().ok().and_then(|mut d| d.remove(&key));
            match outcome {
                Some(Ok(landed)) => {
                    return place_under_assets(&landed, name).map_err(Trouble::Refused)
                }
                Some(Err(why)) => return Err(Trouble::Refused(why)),
                None => std::thread::sleep(std::time::Duration::from_millis(50)),
            }
        }
        Err(Trouble::Refused(format!(
            "`{}` never finished arriving.",
            name
        )))
    }

    /// Mark one offer openable, once its bytes are here.
    fn fetched(&self, peer: &str, token: &str, url: &str) {
        if let Ok(mut shelf) = self.offers.lock() {
            if let Some(o) = shelf.get_mut(&format!("{}|{}", canonical(peer), token)) {
                o.url = url.to_string();
            }
        }
    }

    /// Keep an arriving message if it is for this program's room, and say
    /// whether it was. Messages for another room on the same mesh — another
    /// app, the node's own GUI — are not ours to show.
    pub fn remember(&self, arrival: &V) -> Option<&'static str> {
        let message = at(arrival, "message")?;
        if field(&message, "room") != room_of(&self.network()) {
            return None;
        }
        let from = field(arrival, "from");
        match field(&message, "kind").as_str() {
            "chat" => {
                self.keep(Said {
                    who: self.who(&from),
                    from,
                    text: field(&message, "text"),
                    at: now_ms(),
                });
                Some(SAID_SHAPE)
            }
            // A member restating what it offers the room. With no host to
            // aggregate, every member keeps the whole shelf itself — which
            // is the same list, arrived at without anyone having to be
            // online to hold it.
            "share_list" => {
                let files = match at(&message, "files") {
                    Some(V::List(f)) => f,
                    _ => Vec::new(),
                };
                self.offered_by(&from, &files);
                Some(OFFER_SHAPE)
            }
            _ => None,
        }
    }

    /// A peer's id in the form a ROUTE needs: the display form,
    /// `pubkey-SUFFIX`.
    ///
    /// This distinction is not cosmetic and it fails silently, which is the
    /// worst combination. The roster and room messages carry bare pubkeys;
    /// presence carries the display form. Open a route whose endpoint is the
    /// bare form and the node reports it ACTIVE, accepts the fetch, and
    /// answers nothing — ever. Only presence knows the suffix, so this asks
    /// presence and falls back to what it was given.
    fn addressable(&self, peer: &str) -> String {
        let want = canonical(peer);
        if let Some(V::List(peers)) = at(
            &self
                .ask("session_snapshot", V::Map(BTreeMap::new()))
                .unwrap_or(V::None),
            "peers",
        ) {
            for p in &peers {
                let node = field(p, "node");
                if canonical(&node) == want && node.contains('-') {
                    return node;
                }
            }
        }
        peer.to_string()
    }

    /// What to call whoever sent this, in a sentence a person reads: the name
    /// the roster knows them by, else a short form of their key. The whole key
    /// is fifty-two characters and identifies them perfectly, which is exactly
    /// what a name is not for.
    fn who(&self, id: &str) -> String {
        let want = canonical(id);
        if let Some(V::List(peers)) = at(&self.roster(), "peers") {
            for p in &peers {
                if canonical(&field(p, "device_id")) == want {
                    let label = field(p, "label");
                    if !label.trim().is_empty() {
                        return label.trim().to_string();
                    }
                }
            }
        }
        short(&want)
    }

    /// Add to what this site has heard, oldest dropped first.
    fn keep(&self, said: Said) {
        if let Ok(mut log) = self.heard.lock() {
            log.push(said);
            let over = log.len().saturating_sub(KEPT);
            if over > 0 {
                log.drain(0..over);
            }
        }
    }

    fn said(&self) -> Vec<V> {
        match self.heard.lock() {
            Ok(log) => log.iter().map(Said::value).collect(),
            Err(_) => Vec::new(),
        }
    }

    /// One request to the node: a length-prefixed JSON frame out, one frame
    /// back. Its wire is `[u32 BE len][tag][{"cmd","args"}]`, answering
    /// `{ok, result, error}` — the socket the node's own GUI drives.
    pub fn ask(&self, cmd: &str, args: V) -> Result<V, Trouble> {
        if let Some(why) = theirs_not_ours(cmd) {
            return Err(Trouble::Refused(format!(
                "`{}` is not ours to call: {}",
                cmd, why
            )));
        }
        let mut body = BTreeMap::new();
        body.insert("cmd".to_string(), V::Text(cmd.to_string()));
        body.insert("args".to_string(), args);
        let payload = to_json(&V::Map(body));
        let Some(socket) = &self.socket else {
            return Err(Trouble::Absent(
                "no home directory here, so there is no node socket to find.".to_string(),
            ));
        };
        let (tag, body) = wire::round_trip(socket, &payload, self.start.as_deref())?;
        if tag != 0 {
            return Err(Trouble::Refused(format!(
                "the node answered `{}` with a raw batch, not JSON",
                cmd
            )));
        }
        let answer = String::from_utf8_lossy(&body).to_string();
        let Some(value) = from_json(&answer) else {
            return Err(Trouble::Refused(format!(
                "the node's answer to `{}` was not JSON",
                cmd
            )));
        };
        if matches!(at(&value, "ok"), Some(V::Bool(true))) {
            return Ok(at(&value, "result").unwrap_or(V::None));
        }
        Err(Trouble::Refused(match at(&value, "error") {
            Some(V::Text(e)) => e,
            _ => format!("the node refused `{}` without saying why", cmd),
        }))
    }
}

impl Node {
    /// Ask for a raw batch: the `*_poll` shape, which answers with framed
    /// bytes rather than JSON so a media batch is not re-encoded on its way
    /// through. An error still arrives as JSON, and is raised as one.
    pub fn ask_bytes(&self, cmd: &str, args: V) -> Result<Vec<u8>, Trouble> {
        let mut body = BTreeMap::new();
        body.insert("cmd".to_string(), V::Text(cmd.to_string()));
        body.insert("args".to_string(), args);
        let payload = to_json(&V::Map(body));
        let Some(socket) = &self.socket else {
            return Err(Trouble::Absent(
                "no home directory here, so there is no node socket to find.".to_string(),
            ));
        };
        let (tag, raw) = wire::round_trip(socket, &payload, self.start.as_deref())?;
        if tag == 1 {
            return Ok(raw);
        }
        let text = String::from_utf8_lossy(&raw).to_string();
        Err(Trouble::Refused(match from_json(&text).as_ref().and_then(|v| at(v, "error")) {
            Some(V::Text(e)) => e,
            _ => format!("the node answered `{}` with neither a batch nor a reason", cmd),
        }))
    }
}

/// Commands this adapter must never issue, and why.
///
/// The node is somebody's machine. A program that joins a mesh may add its own
/// network and expose its own port; the identity every mesh knows that machine
/// by belongs to the person running it. An earlier build set the display label
/// from an app's `label` setting on the way in, so starting an Ashlar site
/// renamed its owner's node — for every peer, on every mesh, with no way to
/// tell it had happened.
pub fn theirs_not_ours(cmd: &str) -> Option<&'static str> {
    match cmd {
        "mesh_identity_set_label" => Some(
            "the node's name belongs to whoever runs the machine. A site \
             publishes under its own label instead.",
        ),
        _ => None,
    }
}

/// The node's wire, and the sidecar bring-up in front of it.
///
/// A frame is `[u32 BE len][tag][payload]`, where the length counts the tag
/// and tag 0 is JSON. The socket is a Unix socket or a Windows named pipe, and
/// the only difference between them here is how it opens: a pipe answers
/// `File`'s read/write, so both sides of this module are `std` and neither is
/// a stub.
mod wire {
    use super::Trouble;
    use std::io::{Read, Write};
    use std::path::Path;
    use std::sync::Mutex;
    use std::time::{Duration, Instant};

    #[cfg(not(windows))]
    type Socket = std::os::unix::net::UnixStream;
    #[cfg(windows)]
    type Socket = std::fs::File;

    #[cfg(not(windows))]
    fn open(socket: &Path) -> std::io::Result<Socket> {
        std::os::unix::net::UnixStream::connect(socket)
    }

    /// A named pipe is opened like a file, read-write, with no crate and no
    /// `unsafe` — which is why Windows is a supported platform here and not a
    /// message explaining why it is not.
    #[cfg(windows)]
    fn open(socket: &Path) -> std::io::Result<Socket> {
        std::fs::OpenOptions::new().read(true).write(true).open(socket)
    }

    /// How long a machine stays known to have no node before the bring-up is
    /// attempted again. Without this, every call on a machine with no mesh
    /// pays a spawn and a three-second wait — and a page render makes several,
    /// so "no mesh installed" would present as a site that hangs.
    const PATIENCE: Duration = Duration::from_secs(10);

    static ABSENT: Mutex<Option<(Instant, String)>> = Mutex::new(None);

    /// The reason the last bring-up failed, while it is still recent.
    fn recently_absent() -> Option<String> {
        let seen = ABSENT.lock().ok()?;
        match &*seen {
            Some((at, why)) if at.elapsed() < PATIENCE => Some(why.clone()),
            _ => None,
        }
    }

    fn remember(why: &str) {
        if let Ok(mut seen) = ABSENT.lock() {
            *seen = Some((Instant::now(), why.to_string()));
        }
    }

    fn forget() {
        if let Ok(mut seen) = ABSENT.lock() {
            *seen = None;
        }
    }

    /// Connect, or bring the sidecar up and connect. Reusing what is already
    /// running is the first move: a machine with the app open already has the
    /// node, and a second copy would fight the first over one identity.
    ///
    /// `run` is `None` when the caller named the socket: connect or don't.
    fn connect(socket: &Path, run: Option<&[String]>) -> Result<Socket, Trouble> {
        if let Ok(s) = open(socket) {
            forget();
            return Ok(s);
        }
        let Some(run) = run else {
            return Err(Trouble::Absent(format!(
                "no mesh here: nothing is listening at {}.",
                socket.display()
            )));
        };
        let binary = run.join(" ");
        if let Some(why) = recently_absent() {
            return Err(Trouble::Absent(why));
        }
        let spawned = std::process::Command::new(&run[0])
            .args(&run[1..])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
        if spawned.is_err() {
            let why = format!(
                "no mesh node on this machine: could not start `{}`. Run \
                 `ashlar mesh install` to bring one, set ASHLAR_MESH_NODE to a \
                 node binary, or bind the `mesh` space in foreign.json to \
                 something else.",
                binary
            );
            remember(&why);
            return Err(Trouble::Absent(why));
        }
        // A node takes a moment to bind. Wait in short steps rather than one
        // long sleep, so the common case — it is up almost at once — is fast.
        for _ in 0..60 {
            std::thread::sleep(Duration::from_millis(50));
            if let Ok(s) = open(socket) {
                forget();
                return Ok(s);
            }
        }
        let why = format!(
            "`{}` was started but never answered its socket at {}.",
            binary,
            socket.display()
        );
        remember(&why);
        Err(Trouble::Absent(why))
    }

    /// One frame out, one frame back.
    pub fn round_trip(
        socket: &Path,
        payload: &str,
        run: Option<&[String]>,
    ) -> Result<(u8, Vec<u8>), Trouble> {
        let binary = match run {
            Some(r) => r.join(" "),
            None => socket.display().to_string(),
        };
        let mut stream = connect(socket, run)?;
        let bytes = payload.as_bytes();
        let len = (bytes.len() as u32) + 1;
        stream
            .write_all(&len.to_be_bytes())
            .and_then(|_| stream.write_all(&[0u8]))
            .and_then(|_| stream.write_all(bytes))
            .and_then(|_| stream.flush())
            .map_err(|e| Trouble::Refused(format!("could not write to {}: {}", binary, e)))?;

        let mut head = [0u8; 4];
        stream
            .read_exact(&mut head)
            .map_err(|e| Trouble::Refused(format!("could not read from {}: {}", binary, e)))?;
        let len = u32::from_be_bytes(head) as usize;
        if len == 0 {
            return Err(Trouble::Refused(format!("{} sent an empty frame", binary)));
        }
        // A frame ceiling before allocating: a length is untrusted input even
        // on a local socket.
        if len > 64 * 1024 * 1024 {
            return Err(Trouble::Refused(format!(
                "{} sent a frame past the 64MB ceiling",
                binary
            )));
        }
        let mut body = vec![0u8; len];
        stream
            .read_exact(&mut body)
            .map_err(|e| Trouble::Refused(format!("{}'s answer was truncated: {}", binary, e)))?;
        // Tag 0 is JSON; tag 1 is a raw batch — what every `*_poll` answers,
        // kept binary rather than re-encoded, which is the node's own reason
        // for having two tags at all. Refusing tag 1 here made every poll
        // look like a protocol break.
        Ok((body[0], body[1..].to_vec()))
    }

    /// Subscribe to the node's event stream and call `on` for each event
    /// name, until the connection ends. Returns then — the caller decides
    /// whether to come back.
    ///
    /// The node's own front end reads this connection; so does this. One
    /// frame per event, tag 2, carrying `{"kind":"emit","event":…}`. Nothing
    /// is sent back: this is a listener, and a listener that also talks is a
    /// second client competing with the first for one identity.
    pub fn follow(socket: &Path, on: &mut dyn FnMut(&str, &super::V)) {
        // Never start a node to listen to it. Bringing one up is a decision a
        // call makes; a background listener that spawned a daemon would start
        // somebody's mesh because a page happened to be open.
        let Ok(mut stream) = open(socket) else { return };
        let hello = br#"{"cmd":"__subscribe_events","args":{}}"#;
        let len = (hello.len() as u32) + 1;
        if stream
            .write_all(&len.to_be_bytes())
            .and_then(|_| stream.write_all(&[0u8]))
            .and_then(|_| stream.write_all(hello))
            .and_then(|_| stream.flush())
            .is_err()
        {
            return;
        }
        loop {
            let mut head = [0u8; 4];
            if stream.read_exact(&mut head).is_err() {
                return;
            }
            let len = u32::from_be_bytes(head) as usize;
            if len == 0 || len > 64 * 1024 * 1024 {
                return;
            }
            let mut body = vec![0u8; len];
            if stream.read_exact(&mut body).is_err() {
                return;
            }
            // Tag 0 is the subscribe ack; tag 2 is an event. Anything else on
            // this connection is not ours to interpret.
            if body[0] != 2 {
                continue;
            }
            let Ok(text) = String::from_utf8(body[1..].to_vec()) else {
                return;
            };
            let Some(frame) = super::from_json(&text) else {
                continue;
            };
            if let Some(super::V::Text(event)) = super::at(&frame, "event") {
                on(
                    &event,
                    &super::at(&frame, "payload").unwrap_or(super::V::None),
                );
            }
        }
    }

    /// The bytes a request becomes, so the framing is checkable without a node.
    #[cfg(test)]
    pub fn frame(payload: &str) -> Vec<u8> {
        let bytes = payload.as_bytes();
        let len = (bytes.len() as u32) + 1;
        let mut out = Vec::with_capacity(bytes.len() + 5);
        out.extend_from_slice(&len.to_be_bytes());
        out.push(0u8);
        out.extend_from_slice(bytes);
        out
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
        assert!(parse_call(r#"{"call":"x","args":1}"#)
            .unwrap_err()
            .contains("`args`"));
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
        for dead in [
            "sighted",
            "handshaking",
            "pending_approval",
            "reconnecting",
            "offline",
        ] {
            assert_eq!(at(&row(dead), "here"), Some(V::Bool(false)), "{}", dead);
        }
    }

    #[test]
    fn an_unlabelled_peer_falls_back_to_its_id() {
        // This node is a peer to everyone else, so `here` answers the same
        // way: a machine whose owner never named it renders as its id, not as
        // a blank row that reads like a bug.
        let row = peer_row(&map(&[("device_id", text("abc")), ("label", text(" "))]));
        assert_eq!(at(&row, "label"), Some(text("abc")));
        let row = peer_row(&map(&[
            ("device_id", text("abc")),
            ("label", text(" ada ")),
        ]));
        assert_eq!(at(&row, "label"), Some(text("ada")));
        let row = peer_row(&map(&[]));
        assert_eq!(at(&row, "label"), Some(text("unknown")));
    }

    #[test]
    fn exposing_leaves_every_other_selection_alone() {
        // The node's exposed map is its owner's choice about the whole
        // machine. Publishing adds one port; replacing the map would silently
        // unpublish whatever else was there.
        let mut current = BTreeMap::new();
        current.insert("tcp:3000".to_string(), "dev server".to_string());
        let after = with_exposed(&current, 8080, "site.app");
        assert_eq!(
            after.get("tcp:3000").map(String::as_str),
            Some("dev server")
        );
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
        assert_eq!(
            sites.len(),
            2,
            "a peer with no node id is skipped: {:?}",
            sites
        );
        assert_eq!(sites[0], ("n1".to_string(), 8080, "pad".to_string()));
        assert_eq!(sites[1], ("n1".to_string(), 9000, "ada :9000".to_string()));
    }

    #[test]
    fn sites_are_kept_to_the_mesh_whose_roster_names_them() {
        // Presence reaches every network this node joined, so a snapshot mixes
        // peers from all of them. The roster for THIS mesh is the filter — the
        // earlier build refused the whole list instead, on the false belief
        // that a site is advertised on one network only.
        let snapshot = map(&[(
            "peers",
            V::List(vec![
                map(&[
                    ("node", text("aaa-11111")),
                    ("label", text("ada")),
                    (
                        "sites",
                        V::List(vec![map(&[
                            ("label", text("pad")),
                            ("port", V::Number(80.0)),
                        ])]),
                    ),
                ]),
                map(&[
                    ("node", text("bbb-22222")),
                    ("label", text("someone on the fleet")),
                    (
                        "sites",
                        V::List(vec![map(&[
                            ("label", text("nas")),
                            ("port", V::Number(81.0)),
                        ])]),
                    ),
                ]),
            ]),
        )]);
        let all = peer_sites(&snapshot);
        assert_eq!(all.len(), 2, "both are real sites: {:?}", all);
        // `aaa` is on our mesh; `bbb` is not.
        let mine = vec!["aaa".to_string()];
        let kept: Vec<_> = all
            .iter()
            .filter(|(peer, _, _)| mine.contains(&canonical(peer)))
            .collect();
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].2, "pad");
    }

    #[test]
    fn a_room_is_derived_from_the_mesh_so_nobody_hosts_it() {
        // Every member computes the same id from the name they already share,
        // which is what removes the host: there is no roster to be stated, no
        // knock to be admitted, and nothing to be offline. The mesh id is the
        // invite, so a program written with a private one is private.
        assert_eq!(room_of("enclave"), "ashlar:enclave");
        assert_ne!(room_of("enclave"), room_of("elsewhere"));
    }

    #[test]
    fn a_speaker_is_named_at_reading_length() {
        // A key identifies somebody perfectly, which is exactly what a name is
        // not for. Fifty-two characters per line is a wall, not a conversation.
        assert_eq!(short("577fowcgdh57hhtvls566jngiejtqsi4b4xxks673fo6ponuompq"), "577fowcg…");
        assert_eq!(short("ada"), "ada");
        assert_eq!(short(""), "");
    }

    #[test]
    fn only_this_room_is_this_program_s_to_show() {
        // One mesh can carry several apps' rooms, and the node's own. A line
        // for another room arrives here all the same, and showing it would be
        // this program reading somebody else's conversation.
        let node = Node::at(PathBuf::from("/nonexistent/node.sock"));
        node.set_network("enclave");
        let arrival = |room: &str, kind: &str| {
            map(&[
                ("from", V::Text("n1-ABCDE".into())),
                (
                    "message",
                    map(&[
                        ("room", V::Text(room.to_string())),
                        ("kind", V::Text(kind.to_string())),
                        ("text", V::Text("hi".into())),
                    ]),
                ),
            ])
        };
        assert_eq!(node.remember(&arrival("ashlar:elsewhere", "chat")), None);
        assert_eq!(node.remember(&arrival("ashlar:enclave", "join")), None);
        assert_eq!(node.remember(&V::None), None);
        assert_eq!(
            node.remember(&arrival("ashlar:enclave", "chat")),
            Some(SAID_SHAPE)
        );
        let said = node.said();
        assert_eq!(said.len(), 1);
        assert_eq!(at(&said[0], "text"), Some(V::Text("hi".into())));
        // No node to ask for a name, so the key — which is short already —
        // stands in, without its display suffix.
        assert_eq!(at(&said[0], "who"), Some(V::Text("n1".into())));
    }

    #[test]
    fn a_room_is_a_conversation_not_an_archive() {
        let node = Node::at(PathBuf::from("/nonexistent/node.sock"));
        for i in 0..(KEPT + 10) {
            node.keep(Said {
                from: "n1".into(),
                who: "ada".into(),
                text: format!("line {}", i),
                at: 0.0,
            });
        }
        let said = node.said();
        assert_eq!(said.len(), KEPT, "the oldest fall off rather than growing forever");
        assert_eq!(at(&said[0], "text"), Some(V::Text("line 10".into())));
    }

    #[test]
    fn a_display_suffix_never_stops_two_ids_matching() {
        // The daemon's roster answers bare pubkeys; presence carries
        // `pubkey-SUFFIX`. Comparing them raw is a roster that silently
        // matches nothing.
        assert_eq!(canonical("aaa-11111"), "aaa");
        assert_eq!(canonical("aaa"), "aaa");
        assert_eq!(canonical(""), "");
    }

    #[test]
    fn installing_runs_what_the_mesh_publishes_and_nothing_of_ours() {
        // A second, worse installer is the temptation here: fetch a release,
        // check a hash, put it somewhere. Theirs already does that, sets up
        // the daemon the node needs, and changes without asking us.
        let argv = install_command();
        let line = argv.join(" ");
        assert!(line.contains("allmystuff.works/install"), "{}", line);
        if cfg!(windows) {
            assert_eq!(argv[0], "powershell");
            assert!(line.contains("install.ps1"), "{}", line);
        } else {
            assert_eq!(argv[0], "sh");
            assert!(line.contains("install.sh"), "{}", line);
        }
    }

    #[test]
    fn bring_up_prefers_an_override_then_what_is_installed() {
        // A machine that has the node under its own name should not be told
        // the mesh is absent. The last resort is a PATH lookup whose failure
        // names what to install.
        let argv = bring_up();
        assert!(!argv.is_empty());
        assert!(
            argv[0].contains("allmystuff") || !argv[0].is_empty(),
            "{:?}",
            argv
        );
        // The CLI form carries its subcommand, the binary form does not.
        if argv.len() > 1 {
            assert_eq!(argv[1], "serve", "{:?}", argv);
        }
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
    fn the_machine_is_never_renamed_by_a_program_that_joins_its_mesh() {
        // The bug this pins: `enter` set the node's display label from the
        // app's `label` setting, so starting an Ashlar site renamed its
        // owner's node for every peer on every mesh it was on. The guard is
        // in `ask`, before a socket is touched, so it holds for any future
        // call site as well as the one that had it.
        let why = theirs_not_ours("mesh_identity_set_label").expect("refused by name");
        assert!(
            why.contains("belongs to whoever runs the machine"),
            "{}",
            why
        );
        let e = Node::at(PathBuf::from("/nonexistent/node.sock"))
            .ask("mesh_identity_set_label", V::Map(BTreeMap::new()))
            .unwrap_err();
        assert!(!e.absent(), "this is refused, not unreachable: {:?}", e);
        assert!(e.why().contains("not ours to call"), "{}", e.why());
        // Reading it is fine, and so is everything a program owns.
        for ours in [
            "mesh_identity",
            "mesh_peers",
            "mesh_network_add",
            "site_set_exposed",
        ] {
            assert_eq!(theirs_not_ours(ours), None, "{}", ours);
        }
    }

    #[test]
    fn no_node_is_a_fact_a_read_answers_and_a_publish_refuses() {
        // covers: G4
        // A machine with no mesh is ordinary. Reads answer around it — with
        // `reachable` false and the correction in `note`, never a blank that
        // reads as an empty mesh — so a site serves on a machine the mesh
        // never reached. `expose` is somebody's deliberate publish, so it
        // still fails.
        let mut s = Session::new(Node::at(PathBuf::from("/nonexistent/node.sock")));
        let here = s.dispatch("here", &[]).expect("a read answers");
        assert_eq!(at(&here, "reachable"), Some(V::Bool(false)));
        assert_eq!(at(&here, "id"), Some(V::Text(String::new())));
        let V::Text(note) = at(&here, "note").unwrap() else {
            panic!("the note is a text")
        };
        assert!(note.contains("nothing is listening"), "{}", note);
        for read in ["peers", "networks", "published", "nearby"] {
            assert_eq!(s.dispatch(read, &[]), Ok(V::List(vec![])), "{}", read);
        }
        // `enter` is called from a `start` stack: a fault there is a program
        // that will not start at all, which is how one absent daemon took a
        // whole site down.
        let arrived = s
            .dispatch("enter", &[V::Text("enclave".into()), V::Text("x".into())])
            .expect("arriving on a mesh that is not there is not a fault");
        assert_eq!(at(&arrived, "reachable"), Some(V::Bool(false)));
        assert_eq!(at(&arrived, "network"), Some(V::Text("enclave".into())));
        let refused = s
            .dispatch("expose", &[V::Number(8080.0), V::Text("site".into())])
            .unwrap_err();
        assert!(refused.contains("nothing is listening"), "{}", refused);
    }

    #[test]
    fn a_named_socket_starts_nothing() {
        // Naming the socket says where the node IS. Spawning a daemon anyway
        // would be the opposite of what naming it meant — and on a machine
        // with the app installed, it would start somebody's real node during
        // a test run.
        let e = Node::at(PathBuf::from("/nonexistent/node.sock"))
            .ask("mesh_identity", V::Map(BTreeMap::new()))
            .unwrap_err();
        assert!(e.absent(), "{:?}", e);
        assert!(e.why().contains("/nonexistent/node.sock"), "{}", e.why());
        assert!(
            !e.why().contains("could not start"),
            "nothing was started, so nothing is reported as failing to: {}",
            e.why()
        );
    }

    #[test]
    fn a_joined_network_is_recognised_either_way_it_is_keyed() {
        let networks = map(&[(
            "networks",
            V::List(vec![map(&[
                ("id", text("home")),
                ("network_id", text("abc")),
            ])]),
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
        assert!(e.contains("here, peers, networks, enter"), "{}", e);
        assert!(e.contains("expose, unexpose"), "{}", e);
    }

    #[test]
    fn arguments_are_checked_before_a_socket_is_touched() {
        // A bad argument must not reach the daemon: the message an Ashlar call
        // site sees should be about the argument, not about a socket.
        let mut s = Session::default();
        assert!(s
            .dispatch("expose", &[text("8080")])
            .unwrap_err()
            .contains("port number"));
        assert!(s
            .dispatch("expose", &[V::Number(0.0)])
            .unwrap_err()
            .contains("port number"));
        assert!(s
            .dispatch("expose", &[V::Number(70000.0)])
            .unwrap_err()
            .contains("port number"));
        assert!(s
            .dispatch("unexpose", &[])
            .unwrap_err()
            .contains("at least 1"));
        assert!(s
            .dispatch("enter", &[text("x")])
            .unwrap_err()
            .contains("at least 2"));
    }

    #[test]
    fn the_socket_is_derived_the_way_the_node_derives_it() {
        // `MYOWNMESH_HOME` is how a second install runs beside a first. Reading
        // it the way the node reads it is what makes this a client of what is
        // already there rather than a second convention.
        let socket = node_socket();
        if cfg!(windows) {
            assert!(socket.is_some(), "a named pipe needs no home directory");
            return;
        }
        match std::env::var("MYOWNMESH_HOME") {
            Ok(h) if !h.trim().is_empty() => assert_eq!(
                socket,
                Some(
                    std::path::PathBuf::from(h.trim())
                        .join(".myownmesh")
                        .join("allmystuff-node.sock")
                )
            ),
            _ => assert!(socket.is_some() || std::env::var("HOME").is_err()),
        }
    }

    #[test]
    fn a_request_frames_the_way_the_node_reads_it() {
        // `[u32 BE len][tag][JSON]`, length counting the tag. This is the whole
        // contract with the node's socket; if it drifts, this fails rather than
        // a user's first request.
        let bytes = wire::frame(r#"{"cmd":"site_exposed","args":{}}"#);
        let len = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize;
        assert_eq!(len, bytes.len() - 4, "len covers tag + payload");
        assert_eq!(bytes[4], 0, "tag 0 is JSON");
        assert_eq!(
            String::from_utf8(bytes[5..].to_vec()).unwrap(),
            r#"{"cmd":"site_exposed","args":{}}"#
        );
    }
}
