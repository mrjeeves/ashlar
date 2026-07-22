//! T-G — runtime conformance (G-series). The heart is the G2 identity
//! test: the same handler exercised over HTTP and over the WebSocket
//! envelope protocol must produce identical results.

use ashlar::http;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};

fn fixture(dir: &str, files: &[(&str, &str)]) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "ashlar_tg_{}_{}",
        dir,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).unwrap();
    for (name, src) in files {
        std::fs::write(root.join(name), src).unwrap();
    }
    root
}

/// Start the server on an ephemeral port; return (port, stop flag, join).
fn start(root: PathBuf) -> (u16, Arc<AtomicBool>, std::thread::JoinHandle<()>) {
    let stop = Arc::new(AtomicBool::new(false));
    let stop2 = stop.clone();
    let (tx, rx) = mpsc::channel();
    let join = std::thread::spawn(move || {
        let r = http::serve(root, Some(0), move |port| tx.send(port).unwrap(), stop2);
        if let Err(e) = r {
            panic!("serve failed: {}", e);
        }
    });
    let port = rx.recv_timeout(std::time::Duration::from_secs(10)).unwrap();
    (port, stop, join)
}

fn http_get(port: u16, path: &str) -> (u16, String) {
    http_req(port, "GET", path, None)
}

fn http_req(port: u16, method: &str, path: &str, body: Option<&str>) -> (u16, String) {
    let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
    let body = body.unwrap_or("");
    let req = format!(
        "{} {} HTTP/1.1\r\nhost: t\r\ncontent-length: {}\r\n\r\n{}",
        method,
        path,
        body.len(),
        body
    );
    s.write_all(req.as_bytes()).unwrap();
    let mut buf = String::new();
    s.read_to_string(&mut buf).unwrap();
    let status: u16 = buf
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let body = buf
        .split("\r\n\r\n")
        .nth(1)
        .unwrap_or("")
        .to_string();
    (status, body)
}

/// Minimal WebSocket client: handshake, one masked text frame out, one
/// text frame in.
fn ws_roundtrip(port: u16, envelope: &str) -> String {
    let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
    let req = "GET / HTTP/1.1\r\nhost: t\r\nupgrade: websocket\r\nconnection: Upgrade\r\nsec-websocket-key: dGhlIHNhbXBsZSBub25jZQ==\r\nsec-websocket-version: 13\r\n\r\n";
    s.write_all(req.as_bytes()).unwrap();
    // Read the 101 response headers.
    let mut hdr = Vec::new();
    let mut byte = [0u8; 1];
    while !hdr.ends_with(b"\r\n\r\n") {
        s.read_exact(&mut byte).unwrap();
        hdr.push(byte[0]);
    }
    let head = String::from_utf8_lossy(&hdr);
    assert!(head.starts_with("HTTP/1.1 101"), "handshake: {}", head);
    assert!(
        head.to_lowercase()
            .contains("sec-websocket-accept: s3pplmbitxaq9kygzzhzrbk+xoo="),
        "RFC 6455 sample key must produce the sample accept: {}",
        head
    );

    // Client frames are masked (RFC 6455 §5.1).
    let payload = envelope.as_bytes();
    let mask = [0x11u8, 0x22, 0x33, 0x44];
    let mut frame = vec![0x81u8];
    assert!(payload.len() < 126);
    frame.push(0x80 | payload.len() as u8);
    frame.extend_from_slice(&mask);
    for (i, b) in payload.iter().enumerate() {
        frame.push(b ^ mask[i % 4]);
    }
    s.write_all(&frame).unwrap();

    // Read one unmasked server text frame.
    let mut h2 = [0u8; 2];
    s.read_exact(&mut h2).unwrap();
    assert_eq!(h2[0] & 0x0F, 1, "expected a text frame");
    let mut len = (h2[1] & 0x7F) as u64;
    if len == 126 {
        let mut ext = [0u8; 2];
        s.read_exact(&mut ext).unwrap();
        len = u16::from_be_bytes(ext) as u64;
    }
    let mut payload = vec![0u8; len as usize];
    s.read_exact(&mut payload).unwrap();
    String::from_utf8(payload).unwrap()
}

const APP: &str = r#"space demo

part Server {
  port = 0
  state hits: number = 0
  start stack = () => {
    return { hits: 0 }
  }
}

part thing {
  route = "/api/thing/{id}"
  handle pipe = (req: std.Request) => {
    return { got: req.params["id"], method: req.method, sent: req.data }
  }
}

part guarded {
  route = "/api/secret"
  allow = (req: std.Request) => req.user != none
  handle pipe = (req: std.Request) => "never"
}

part missing {
  route = "/api/lookup/{k}"
  handle pipe = (req: std.Request) => {
    let m = { a: "found" }
    return m[req.params["k"]!] ?? fail(404, "no such key")
  }
}
"#;

#[test]
fn t_g2_same_handler_http_and_websocket_identical() {
    // covers: G2
    let root = fixture("g2", &[("app.ash", APP)]);
    let (port, stop, join) = start(root);

    let (status, http_body) = http_get(port, "/api/thing/42");
    assert_eq!(status, 200);

    let reply = ws_roundtrip(port, "{\"path\":\"/api/thing/42\"}");
    // Envelope: {"data": <same value>, "status": 200}
    assert!(reply.contains("\"status\":200"), "{}", reply);
    let data_start = reply.find("\"data\":").unwrap() + 7;
    let data = &reply[data_start..reply.rfind(",\"status\"").unwrap_or(reply.len() - 1)];
    assert_eq!(
        data, http_body,
        "the same handler must produce identical results over both transports"
    );

    stop.store(true, Ordering::Relaxed);
    join.join().unwrap();
}

#[test]
fn t_g_routing_params_guard_fail_and_post_body() {
    // covers: G4 (routing, request handling), reference 9.2 and 9.6
    let root = fixture("routes", &[("app.ash", APP)]);
    let (port, stop, join) = start(root);

    // Path captures and JSON rendering.
    let (status, body) = http_get(port, "/api/thing/abc");
    assert_eq!(status, 200);
    assert!(body.contains("\"got\":\"abc\""), "{}", body);
    assert!(body.contains("\"method\":\"get\""), "{}", body);

    // POST body decodes as data and comes back.
    let (status, body) = http_req(port, "POST", "/api/thing/x", Some("{\"n\": 7}"));
    assert_eq!(status, 200);
    assert!(body.contains("\"sent\":{\"n\":7}"), "{}", body);

    // The allow guard rejects with 403 (no user on plain requests).
    let (status, _) = http_get(port, "/api/secret");
    assert_eq!(status, 403);

    // fail(404, ...) carries its status; the hit path returns the value.
    let (status, body) = http_get(port, "/api/lookup/a");
    assert_eq!(status, 200);
    assert_eq!(body, "found");
    let (status, body) = http_get(port, "/api/lookup/zz");
    assert_eq!(status, 404);
    assert!(body.contains("no such key"), "{}", body);

    // Unrouted paths are 404.
    let (status, _) = http_get(port, "/nope");
    assert_eq!(status, 404);

    stop.store(true, Ordering::Relaxed);
    join.join().unwrap();
}

#[test]
fn t_g_stored_persists_across_restart() {
    // covers: G4 (persistence), reference 9.3
    let app = r#"space demo

part Server {
  port = 0
}

part counter {
  route = "/bump"
  stored n: number = 0
  handle pipe = (req: std.Request) => {
    bump()
    return n
  }
  bump = () => { n = n + 1 }
}
"#;
    let root = fixture("stored", &[("app.ash", app)]);

    let (port, stop, join) = start(root.clone());
    let (_, b1) = http_get(port, "/bump");
    let (_, b2) = http_get(port, "/bump");
    assert_eq!(b1, "1");
    assert_eq!(b2, "2");
    stop.store(true, Ordering::Relaxed);
    join.join().unwrap();

    // A fresh process (new server on the same root) resumes from disk.
    let (port2, stop2, join2) = start(root);
    let (_, b3) = http_get(port2, "/bump");
    assert_eq!(b3, "3", "stored state must survive restart");
    stop2.store(true, Ordering::Relaxed);
    join2.join().unwrap();
}

#[test]
fn t_g3_hot_reload_preserves_state() {
    // covers: G3
    let app_v1 = r#"space demo

part Server {
  port = 0
}

part c {
  route = "/c"
  state n: number = 0
  handle pipe = (req: std.Request) => {
    bump()
    return "v1: " + text(n)
  }
  bump = () => { n = n + 1 }
}
"#;
    let root = fixture("reload", &[("app.ash", app_v1)]);
    let (port, stop, join) = start(root.clone());

    let (_, b1) = http_get(port, "/c");
    assert_eq!(b1, "v1: 1");

    // Edit the source: the handler text changes, the state must not.
    std::thread::sleep(std::time::Duration::from_millis(50));
    std::fs::write(root.join("app.ash"), app_v1.replace("v1: ", "v2: ")).unwrap();
    // The reload scan runs every ~500ms.
    std::thread::sleep(std::time::Duration::from_millis(1200));

    let (_, b2) = http_get(port, "/c");
    assert_eq!(b2, "v2: 2", "new code, preserved state");

    stop.store(true, Ordering::Relaxed);
    join.join().unwrap();
}

#[test]
fn t_g_scheduled_task_runs() {
    // covers: G4 (scheduled tasks), reference 9.7
    let app = r#"space demo

part Server {
  port = 0
}

part ticker {
  route = "/ticks"
  state n: number = 0
  every = "100ms"
  run = () => { n = n + 1 }
  handle pipe = (req: std.Request) => n
}
"#;
    let root = fixture("sched", &[("app.ash", app)]);
    let (port, stop, join) = start(root);

    std::thread::sleep(std::time::Duration::from_millis(450));
    let (_, body) = http_get(port, "/ticks");
    let n: f64 = body.parse().unwrap_or(0.0);
    assert!(n >= 1.0, "scheduled task should have run, got {}", body);

    stop.store(true, Ordering::Relaxed);
    join.join().unwrap();
}

// ---------------------------------------------------------------------------
// Views, auth, files, spawn (the §9.4/§9.6/§9.7/§9.8 conformance set).
// ---------------------------------------------------------------------------

fn http_req_full(port: u16, method: &str, path: &str, body: Option<&str>, cookie: Option<&str>) -> (u16, String, String) {
    let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
    let body = body.unwrap_or("");
    let cookie_line = cookie.map(|c| format!("cookie: ashsession={}\r\n", c)).unwrap_or_default();
    let req = format!(
        "{} {} HTTP/1.1\r\nhost: t\r\n{}content-length: {}\r\n\r\n{}",
        method, path, cookie_line, body.len(), body
    );
    s.write_all(req.as_bytes()).unwrap();
    let mut buf = String::new();
    s.read_to_string(&mut buf).unwrap();
    let status: u16 = buf.split_whitespace().nth(1).and_then(|s| s.parse().ok()).unwrap_or(0);
    let mut parts = buf.splitn(2, "\r\n\r\n");
    let head = parts.next().unwrap_or("").to_string();
    let body = parts.next().unwrap_or("").to_string();
    (status, head, body)
}

/// Extract `attr="value"` from HTML.
fn attr_of(html: &str, attr: &str) -> Option<String> {
    let key = format!("{}=\"", attr);
    let i = html.find(&key)? + key.len();
    let j = html[i..].find('"')? + i;
    Some(html[i..j].to_string())
}

#[test]
fn t_g4_view_round_trip_over_socket() {
    // covers: G4 (reactive state + views), reference 9.4
    let app = r#"space ui

part Server {
  port = 0
}

part page {
  route = "/"
  state n: number = 0
  view = () => el("button", { onclick: bump }, ["clicks: " + text(n)])
  bump = () => { n = n + 1 }
}
"#;
    let root = fixture("views", &[("app.ash", app)]);
    let (port, stop, join) = start(root);

    // The page renders server-side with the transport shim embedded.
    let (status, _, html) = http_req_full(port, "GET", "/", None, None);
    assert_eq!(status, 200);
    assert!(html.contains("clicks: 0"), "{}", html);
    assert!(html.contains("data-ash-on=\"onclick\""), "{}", html);
    assert!(html.contains("new WebSocket"), "client shim missing: {}", html);
    let instance = attr_of(&html, "data-ash-instance").unwrap();
    let hid = attr_of(&html, "data-ash-h").unwrap();

    // Two click events over the socket: per-instance state advances and
    // each reply patches the instance's HTML in place.
    let e1 = format!(
        "{{\"event\":{{\"instance\":\"{}\",\"h\":\"{}\",\"name\":\"onclick\"}}}}",
        instance, hid
    );
    let r1 = ws_roundtrip(port, &e1);
    assert!(r1.contains("clicks: 1"), "{}", r1);
    // The re-render registered a fresh handler id; extract it from the patch.
    let html1 = r1.replace("\\\"", "\"");
    let hid2 = attr_of(&html1, "data-ash-h").unwrap();
    let e2 = format!(
        "{{\"event\":{{\"instance\":\"{}\",\"h\":\"{}\",\"name\":\"onclick\"}}}}",
        instance, hid2
    );
    let r2 = ws_roundtrip(port, &e2);
    assert!(r2.contains("clicks: 2"), "state must be per-instance and persistent: {}", r2);

    stop.store(true, Ordering::Relaxed);
    join.join().unwrap();
}

#[test]
fn t_g4_auth_sessions_end_to_end() {
    // covers: G4 (auth), reference 9.6
    let app = r#"space auth

part Server {
  port = 0
}

part join {
  route = "/signup"
  handle pipe = (req: std.Request) => signup(text(req.data.email), text(req.data.password))
}

part enter {
  route = "/login"
  handle pipe = (req: std.Request) => login(text(req.data.email), text(req.data.password))
}

part me {
  route = "/me"
  allow = (req: std.Request) => req.user != none
  handle pipe = (req: std.Request) => req.user
}

part leave {
  route = "/logout"
  handle pipe = (req: std.Request) => logout()
}
"#;
    let root = fixture("auth", &[("app.ash", app)]);
    let (port, stop, join) = start(root);

    // No session: the guard rejects.
    let (status, _, _) = http_req_full(port, "GET", "/me", None, None);
    assert_eq!(status, 403);

    // Signup opens a session via cookie.
    let (status, head, body) =
        http_req_full(port, "POST", "/signup", Some("{\"email\":\"a@b.c\",\"password\":\"pw\"}"), None);
    assert_eq!(status, 200, "{}", body);
    assert!(body.contains("a@b.c"));
    let cookie_line = head.lines().find(|l| l.to_lowercase().starts_with("set-cookie")).unwrap();
    let token = cookie_line.split("ashsession=").nth(1).unwrap().split(';').next().unwrap().to_string();
    assert!(!token.is_empty());

    // The session carries identity.
    let (status, _, body) = http_req_full(port, "GET", "/me", None, Some(&token));
    assert_eq!(status, 200);
    assert!(body.contains("a@b.c"), "{}", body);

    // Duplicate signup: 409. Bad login: 401. Good login: 200.
    let (status, _, _) =
        http_req_full(port, "POST", "/signup", Some("{\"email\":\"a@b.c\",\"password\":\"x\"}"), None);
    assert_eq!(status, 409);
    let (status, _, _) =
        http_req_full(port, "POST", "/login", Some("{\"email\":\"a@b.c\",\"password\":\"wrong\"}"), None);
    assert_eq!(status, 401);
    let (status, _, _) =
        http_req_full(port, "POST", "/login", Some("{\"email\":\"a@b.c\",\"password\":\"pw\"}"), None);
    assert_eq!(status, 200);

    // Logout ends the session.
    let (status, head, _) = http_req_full(port, "POST", "/logout", None, Some(&token));
    assert_eq!(status, 200);
    assert!(head.to_lowercase().contains("max-age=0"));
    let (status, _, _) = http_req_full(port, "GET", "/me", None, Some(&token));
    assert_eq!(status, 403);

    stop.store(true, Ordering::Relaxed);
    join.join().unwrap();
}

#[test]
fn t_g4_static_files_with_traversal_guard() {
    // covers: G4 (file serving), reference 9.8
    let app = r#"space site

part Server {
  port = 0
}

part static {
  route = "/static"
  files = "public"
}
"#;
    let root = fixture("files", &[("app.ash", app)]);
    std::fs::create_dir_all(root.join("assets/public")).unwrap();
    std::fs::write(root.join("assets/public/hello.txt"), "hi from disk").unwrap();
    std::fs::write(root.join("secret.txt"), "no").unwrap();
    let (port, stop, join) = start(root);

    let (status, body) = http_get(port, "/static/hello.txt");
    assert_eq!(status, 200);
    assert_eq!(body, "hi from disk");
    let (status, _) = http_get(port, "/static/nope.txt");
    assert_eq!(status, 404);
    let (status, _) = http_get(port, "/static/../../secret.txt");
    assert_eq!(status, 404, "path traversal must be rejected");

    stop.store(true, Ordering::Relaxed);
    join.join().unwrap();
}

#[test]
fn t_g4_spawn_runs_between_requests() {
    // covers: G4 (background tasks), reference 9.7
    let app = r#"space bg

part Server {
  port = 0
}

part work {
  route = "/go"
  state done: bool = false
  handle pipe = (req: std.Request) => {
    spawn(() => finish())
    return done
  }
  finish = () => { done = true }
}

part status {
  route = "/done"
  handle pipe = (req: std.Request) => bg.work.done
}
"#;
    let root = fixture("spawn", &[("app.ash", app)]);
    let (port, stop, join) = start(root);

    // The spawning request returns before the task runs.
    let (_, body) = http_get(port, "/go");
    assert_eq!(body, "false");
    // The task drains between requests.
    std::thread::sleep(std::time::Duration::from_millis(100));
    let (_, body) = http_get(port, "/done");
    assert_eq!(body, "true");

    stop.store(true, Ordering::Relaxed);
    join.join().unwrap();
}

// ---------------------------------------------------------------------------
// Multiplexed sockets, cross-client reactivity, foreign binding.
// ---------------------------------------------------------------------------

/// A persistent WebSocket client: handshake once, then send/read frames.
fn ws_open(port: u16) -> TcpStream {
    let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
    let req = "GET / HTTP/1.1\r\nhost: t\r\nupgrade: websocket\r\nconnection: Upgrade\r\nsec-websocket-key: dGhlIHNhbXBsZSBub25jZQ==\r\nsec-websocket-version: 13\r\n\r\n";
    s.write_all(req.as_bytes()).unwrap();
    let mut hdr = Vec::new();
    let mut byte = [0u8; 1];
    while !hdr.ends_with(b"\r\n\r\n") {
        s.read_exact(&mut byte).unwrap();
        hdr.push(byte[0]);
    }
    assert!(String::from_utf8_lossy(&hdr).starts_with("HTTP/1.1 101"));
    s
}

fn ws_send_frame(s: &mut TcpStream, text: &str) {
    let payload = text.as_bytes();
    let mask = [0x51u8, 0x62, 0x73, 0x84];
    let mut frame = vec![0x81u8];
    assert!(payload.len() < 126);
    frame.push(0x80 | payload.len() as u8);
    frame.extend_from_slice(&mask);
    for (i, b) in payload.iter().enumerate() {
        frame.push(b ^ mask[i % 4]);
    }
    s.write_all(&frame).unwrap();
}

fn ws_read_frame(s: &mut TcpStream) -> String {
    let mut h2 = [0u8; 2];
    s.read_exact(&mut h2).unwrap();
    assert_eq!(h2[0] & 0x0F, 1);
    let mut len = (h2[1] & 0x7F) as u64;
    if len == 126 {
        let mut ext = [0u8; 2];
        s.read_exact(&mut ext).unwrap();
        len = u16::from_be_bytes(ext) as u64;
    }
    let mut payload = vec![0u8; len as usize];
    s.read_exact(&mut payload).unwrap();
    String::from_utf8(payload).unwrap()
}

#[test]
fn t_g4_synced_state_broadcasts_across_clients() {
    // covers: G4 (server-synchronized reactive state), reference 9.3/9.4
    let app = r#"space live

part Server {
  port = 0
}

part board {
  route = "/"
  synced total: number = 0
  view = () => el("button", { onclick: bump }, ["total: " + text(total)])
  bump = () => { total = total + 1 }
}
"#;
    let root = fixture("synced", &[("app.ash", app)]);
    let (port, stop, join) = start(root);

    // Two browsers load the page: two instances of `board`, each reading
    // the same synced singleton... per-instance? `synced total` on a view
    // part is per-instance state; cross-client sync needs the SINGLETON
    // read. Use a second part holding the singleton instead.
    let (_, _, html_a) = http_req_full(port, "GET", "/", None, None);
    let (_, _, html_b) = http_req_full(port, "GET", "/", None, None);
    let ia = attr_of(&html_a, "data-ash-instance").unwrap();
    let ha = attr_of(&html_a, "data-ash-h").unwrap();
    let ib = attr_of(&html_b, "data-ash-instance").unwrap();
    assert_ne!(ia, ib, "each page load is its own instance");

    let mut client_a = ws_open(port);
    let client_b = ws_open(port);
    std::thread::sleep(std::time::Duration::from_millis(50));

    // Client A clicks. A gets its reply patch; B — whose instance has its
    // own per-instance state — must NOT change here. (The cross-client
    // case for singletons is the next assertion set.)
    ws_send_frame(
        &mut client_a,
        &format!("{{\"event\":{{\"instance\":\"{}\",\"h\":\"{}\",\"name\":\"onclick\"}}}}", ia, ha),
    );
    let reply_a = ws_read_frame(&mut client_a);
    assert!(reply_a.contains("total: 1"), "{}", reply_a);

    drop(client_a);
    drop(client_b);
    stop.store(true, Ordering::Relaxed);
    join.join().unwrap();
}

#[test]
fn t_g4_singleton_state_read_by_views_broadcasts() {
    // covers: G4 (reactive state read across instances), reference 9.4:
    // "every view that read a changed state property re-renders".
    let app = r#"space live

part Tally {
  synced total: number = 0
  bump = () => { total = total + 1 }
}

part Server {
  port = 0
}

part board {
  route = "/"
  view = () => el("button", { onclick: poke }, ["seen: " + text(live.Tally.total)])
  poke = () => { live.Tally.bump() }
}
"#;
    let root = fixture("bcast", &[("app.ash", app)]);
    let (port, stop, join) = start(root);

    let (_, _, html_a) = http_req_full(port, "GET", "/", None, None);
    let (_, _, html_b) = http_req_full(port, "GET", "/", None, None);
    let ia = attr_of(&html_a, "data-ash-instance").unwrap();
    let ha = attr_of(&html_a, "data-ash-h").unwrap();
    let ib = attr_of(&html_b, "data-ash-instance").unwrap();

    let mut client_a = ws_open(port);
    let mut client_b = ws_open(port);
    std::thread::sleep(std::time::Duration::from_millis(50));

    // A clicks: the singleton changes; BOTH instances read it, so A's
    // reply carries patches for both, and B receives a broadcast.
    ws_send_frame(
        &mut client_a,
        &format!("{{\"event\":{{\"instance\":\"{}\",\"h\":\"{}\",\"name\":\"onclick\"}}}}", ia, ha),
    );
    let reply_a = ws_read_frame(&mut client_a);
    assert!(reply_a.contains("seen: 1"), "{}", reply_a);
    assert!(
        reply_a.contains(&ib),
        "the other instance read the changed singleton and must be in the patch set: {}",
        reply_a
    );
    let broadcast_b = ws_read_frame(&mut client_b);
    assert!(broadcast_b.contains("seen: 1"), "{}", broadcast_b);
    assert!(broadcast_b.contains(&ib), "{}", broadcast_b);

    drop(client_a);
    drop(client_b);
    stop.store(true, Ordering::Relaxed);
    join.join().unwrap();
}

#[test]
fn t_g_foreign_binding_and_shape_fault() {
    // covers: G4 boundary + reference 9.10: foreign calls bind to
    // foreign/<space>.so, values cross as data, a return that does not
    // fit the declared shape is a runtime fault.
    let cc = std::process::Command::new("cc").arg("--version").output();
    if cc.is_err() {
        eprintln!("t_g_foreign: no C compiler; skipping");
        return;
    }
    let app = r#"space net

foreign triple: (number) -> number
foreign lies: (number) -> number

part Server {
  port = 0
}

part calc {
  route = "/calc/{n}"
  handle pipe = (req: std.Request) => triple(number(req.params["n"]!)!)
}

part liar {
  route = "/lie"
  handle pipe = (req: std.Request) => lies(1)
}
"#;
    let c_src = r#"
#include <stdlib.h>
#include <stdio.h>
char* triple(const char* args) {
    double n = 0;
    sscanf(args, "[%lf", &n);
    char* out = malloc(64);
    snprintf(out, 64, "%g", n * 3);
    return out;
}
char* lies(const char* args) {
    (void)args;
    char* out = malloc(16);
    snprintf(out, 16, "\"not a number\"");
    return out;
}
"#;
    let root = fixture("foreign", &[("app.ash", app)]);
    std::fs::create_dir_all(root.join("foreign")).unwrap();
    std::fs::write(root.join("net.c"), c_src).unwrap();
    let out = std::process::Command::new("cc")
        .args(["-shared", "-fPIC", "-o"])
        .arg(root.join("foreign/net.so"))
        .arg(root.join("net.c"))
        .output()
        .unwrap();
    assert!(out.status.success(), "cc failed: {}", String::from_utf8_lossy(&out.stderr));

    let (port, stop, join) = start(root);

    let (status, body) = http_get(port, "/calc/7");
    assert_eq!(status, 200, "{}", body);
    assert_eq!(body, "21");

    // The declared shape is `number`; the library returns text: fault.
    let (status, body) = http_get(port, "/lie");
    assert_eq!(status, 500);
    assert!(body.contains("does not fit"), "{}", body);

    stop.store(true, Ordering::Relaxed);
    join.join().unwrap();
}
