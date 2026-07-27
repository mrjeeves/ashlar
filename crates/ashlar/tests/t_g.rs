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
        let r = http::serve(root, None, Some(0), move |port| tx.send(port).unwrap(), stop2);
        if let Err(e) = r {
            panic!("serve failed: {}", e);
        }
    });
    let port = rx.recv_timeout(std::time::Duration::from_secs(10)).unwrap();
    (port, stop, join)
}

/// Start expecting `serve` to refuse, and return why. A startup that fails
/// is a delivered behavior here, not an accident, so it needs asserting.
fn start_expecting_failure(root: PathBuf) -> String {
    let stop = Arc::new(AtomicBool::new(false));
    match http::serve(root, None, Some(0), |_| {}, stop) {
        Ok(()) => panic!("expected startup to refuse, but it served"),
        Err(e) => e,
    }
}

/// `start`, with the socket-liveness windows narrowed for one server only.
/// Per-server because these tests are threads in one process.
fn start_with_liveness(
    root: PathBuf,
    live: http::Liveness,
) -> (u16, Arc<AtomicBool>, std::thread::JoinHandle<()>) {
    let stop = Arc::new(AtomicBool::new(false));
    let stop2 = stop.clone();
    let (tx, rx) = mpsc::channel();
    let join = std::thread::spawn(move || {
        let r = http::serve_with_liveness(root, None, Some(0), move |port| tx.send(port).unwrap(), stop2, live);
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
    // A no-stall test must FAIL on a stall, not hang the whole binary
    // (cargo test has no per-test timeout): every probe reads with a
    // deadline.
    s.set_read_timeout(Some(std::time::Duration::from_secs(10))).unwrap();
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
    // Bound every read so a wedged socket fails the test instead of
    // hanging the binary forever.
    s.set_read_timeout(Some(std::time::Duration::from_secs(10))).unwrap();
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
fn t_g_a_view_names_its_page_and_the_title_follows_state() {
    // covers: G4 (§9.4) — found by driving `counter` with a real browser.
    // Every served page had an empty tab, and nothing in the reference said
    // how to name one. A `title` element in a view does it, and because it
    // re-renders like any other element the tab follows state. The capability
    // was there; the sentence describing it was not, which is an A2 defect —
    // a correct program needing knowledge the reference does not contain.
    let app = r#"space demo

part Server {
  port = 0
}

part page {
  route = "/"
  state n: number = 0
  view = () => el("div", {}, [
    el("title", {}, ["count " + text(n)]),
    el("button", { onclick: bump }, ["clicked " + text(n)]),
  ])
  bump = () => {
    n = n + 1
  }
}
"#;
    let root = fixture("pagetitle", &[("app.ash", app)]);
    let (port, stop, join) = start(root);

    let (status, _, html) = http_req_full(port, "GET", "/", None, None);
    assert_eq!(status, 200);
    assert!(
        html.contains("<title>count 0</title>"),
        "the view's title must reach the document: {}",
        html
    );
    let instance = attr_of(&html, "data-ash-instance").unwrap();
    let hid = attr_of(&html, "data-ash-h").unwrap();

    // The tab follows state: the patch carries the new title, and carries
    // exactly one — a second would leave the browser showing either.
    let ev = format!(
        r#"{{"event":{{"instance":"{}","h":"{}","name":"onclick"}}}}"#,
        instance, hid
    );
    let reply = ws_roundtrip(port, &ev);
    assert!(
        reply.contains("count 1"),
        "the title must re-render with the state it read: {}",
        reply
    );
    assert_eq!(
        reply.matches("<title>").count(),
        1,
        "exactly one title per patch: {}",
        reply
    );

    stop.store(true, Ordering::Relaxed);
    join.join().unwrap();
}

#[test]
fn t_g3_a_view_instances_state_does_not_survive_reload() {
    // covers: G3 — the other half of hot reload, and the half the flat
    // requirement used to get wrong. `t_g3_hot_reload_preserves_state`
    // proves a full-name-addressable state property carries over. A view
    // instance's `state` belongs to a PAGE, and the page is gone: driving
    // `counter` with a real browser through a source edit took the count
    // from 3 to 0 while the socket reconnected against the new code.
    //
    // That is what the reference describes — "state properties carry over by
    // full name, and open pages reconnect and re-render themselves" — and it
    // is what G3 now says. The test exists so the two halves cannot drift:
    // this one must keep failing to preserve, and its sibling must keep
    // preserving.
    let v1 = r#"space demo

part Server {
  port = 0
}

part page {
  route = "/"
  state n: number = 0
  view = () => el("button", { onclick: bump }, ["v1 " + text(n)])
  bump = () => {
    n = n + 1
  }
}
"#;
    let root = fixture("reload-instance", &[("app.ash", v1)]);
    let (port, stop, join) = start(root.clone());

    let (_, _, html) = http_req_full(port, "GET", "/", None, None);
    let instance = attr_of(&html, "data-ash-instance").unwrap();
    let hid = attr_of(&html, "data-ash-h").unwrap();
    let ev = format!(
        "{{\"event\":{{\"instance\":\"{}\",\"h\":\"{}\",\"name\":\"onclick\"}}}}",
        instance, hid
    );
    let bumped = ws_roundtrip(port, &ev);
    assert!(bumped.contains("v1 1"), "{}", bumped);

    std::thread::sleep(std::time::Duration::from_millis(50));
    std::fs::write(root.join("app.ash"), v1.replace("v1 ", "v2 ")).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(1200));

    // A page loading after the reload gets the new code and a fresh instance
    // at its default — not the 1 the previous page had reached.
    let (_, _, after) = http_req_full(port, "GET", "/", None, None);
    assert!(after.contains("v2 0"), "new code, instance state reborn: {}", after);

    stop.store(true, Ordering::Relaxed);
    join.join().unwrap();
}

#[test]
fn t_g_a_files_part_naming_one_file_answers_one_absolute_path() {
    // covers: G4 (§9.8) — found by driving examples with a real browser.
    // Every page load asks for /favicon.ico and every program answered 404,
    // and the same hole hid `/robots.txt` and `/.well-known/...`: `files`
    // mounted a DIRECTORY under a route prefix, so reaching an absolute path
    // meant `route = "/"`, which collides with the home page (E021). A
    // program with a home page could not answer any of them.
    //
    // Naming a file instead of a directory answers that one route exactly.
    // Nothing new is declared — the build resolves the name against assets/
    // and already knows which it found.
    let app = r#"space demo

part Server {
  port = 0
}

part page {
  route = "/"
  view = () => el("div", {}, [el("h1", {}, ["home"])])
}

part robots {
  route = "/robots.txt"
  files = "robots.txt"
}

part pub {
  route = "/static"
  files = "public"
}
"#;
    let root = fixture("onefile", &[("app.ash", app)]);
    std::fs::create_dir_all(root.join("assets/public")).unwrap();
    std::fs::write(root.join("assets/robots.txt"), "User-agent: *\n").unwrap();
    std::fs::write(root.join("assets/public/note.txt"), "in a directory\n").unwrap();
    let (port, stop, join) = start(root);

    // The named file answers its own path, with a content type read from its
    // extension rather than the octet-stream fallback.
    let (status, headers, body) = http_req_full(port, "GET", "/robots.txt", None, None);
    assert_eq!(status, 200, "{}", body);
    assert!(body.contains("User-agent: *"), "{}", body);
    assert!(
        headers.to_lowercase().contains("content-type: text/plain"),
        "a .txt file is text: {}",
        headers
    );

    // It claims exactly one path: nothing below it, and not the home page,
    // which is the whole reason the single-file form exists.
    let (below, _, _) = http_req_full(port, "GET", "/robots.txt/anything", None, None);
    assert_eq!(below, 404, "a single file must not shadow paths beneath it");
    let (home, _, home_body) = http_req_full(port, "GET", "/", None, None);
    assert_eq!(home, 200);
    assert!(home_body.contains("home"), "{}", home_body);

    // And the directory form is untouched.
    let (dir_status, _, dir_body) = http_req_full(port, "GET", "/static/note.txt", None, None);
    assert_eq!(dir_status, 200, "{}", dir_body);
    assert!(dir_body.contains("in a directory"), "{}", dir_body);

    stop.store(true, Ordering::Relaxed);
    join.join().unwrap();
}

#[test]
fn t_g_a_chunked_request_body_reaches_the_handler() {
    // covers: G2 (transport is not visible in handler code), G4.
    //
    // Found by driving `examples/slate` with raw sockets rather than the
    // client this repo wrote. Every HTTP/1.1 client may frame a body it
    // cannot measure in advance — a streamed `fetch`, a proxy that
    // re-framed, `curl -T -` — and the runtime read only `content-length`,
    // so that body silently became empty. The program then refused a
    // request whose body was exactly right, and the message blamed the
    // caller for the runtime's gap.
    let app = r#"space demo

part Server {
  port = 0
}

part echo {
  route = "/echo"
  handle pipe = (req: std.Request) => {
    let m = fields(req.data) ?? fail(400, "want an object")
    return "got:" + text(m["v"] ?? "nothing")
  }
}
"#;
    let root = fixture("chunked", &[("app.ash", app)]);
    let (port, stop, join) = start(root);

    let json = br#"{"v":"streamed"}"#;
    let head = "POST /echo HTTP/1.1\r\nhost: t\r\ncontent-type: application/json\r\n\
                transfer-encoding: chunked\r\n\r\n";

    // One chunk.
    let one = format!("{}{:X}\r\n{}\r\n0\r\n\r\n", head, json.len(), String::from_utf8_lossy(json));
    let (status, body) = raw_exchange(port, one.as_bytes());
    assert_eq!(status, 200, "{}", body);
    assert!(body.contains("got:streamed"), "the whole body must arrive: {}", body);

    // Split across chunks, one of them carrying a chunk extension — both
    // legal, and both things a real client emits without asking.
    let (a, b) = json.split_at(7);
    let split = format!(
        "{}{:X};meta=1\r\n{}\r\n{:X}\r\n{}\r\n0\r\n\r\n",
        head,
        a.len(),
        String::from_utf8_lossy(a),
        b.len(),
        String::from_utf8_lossy(b)
    );
    let (status, body) = raw_exchange(port, split.as_bytes());
    assert_eq!(status, 200, "{}", body);
    assert!(body.contains("got:streamed"), "chunks must reassemble in order: {}", body);

    // Both framings at once is the ambiguity request smuggling is built
    // from. Refuse, and say why — a reset would teach the caller nothing.
    let ambiguous = format!(
        "POST /echo HTTP/1.1\r\nhost: t\r\ncontent-type: application/json\r\n\
         transfer-encoding: chunked\r\ncontent-length: 5\r\n\r\n{:X}\r\n{}\r\n0\r\n\r\n",
        json.len(),
        String::from_utf8_lossy(json)
    );
    let (status, body) = raw_exchange(port, ambiguous.as_bytes());
    assert_eq!(status, 400, "{}", body);
    assert!(body.contains("not both"), "the refusal must name the conflict: {}", body);

    // A chunked body must not arrive empty, which is what reading only
    // `content-length` produced: the handler saw nothing and refused a
    // request that was correct.
    let empty_shaped = format!("{}0\r\n\r\n", head);
    let (status, body) = raw_exchange(port, empty_shaped.as_bytes());
    assert_eq!(status, 400, "an actually-empty chunked body is still empty: {}", body);

    // The measured framing is untouched.
    let plain = format!(
        "POST /echo HTTP/1.1\r\nhost: t\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{}",
        json.len(),
        String::from_utf8_lossy(json)
    );
    let (status, body) = raw_exchange(port, plain.as_bytes());
    assert_eq!(status, 200, "{}", body);
    assert!(body.contains("got:streamed"), "{}", body);

    stop.store(true, Ordering::Relaxed);
    join.join().unwrap();
}

/// Send exact bytes on a fresh connection and read the whole reply.
/// The point is control over framing, which a helper that builds the
/// request for you cannot give.
fn raw_exchange(port: u16, request: &[u8]) -> (u16, String) {
    let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
    s.set_read_timeout(Some(std::time::Duration::from_secs(5))).unwrap();
    s.write_all(request).unwrap();
    let mut buf = String::new();
    let _ = s.read_to_string(&mut buf);
    let status: u16 = buf
        .split_whitespace()
        .nth(1)
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    (status, buf)
}

#[test]
fn t_g_stored_state_is_reconciled_against_the_shape_on_start() {
    // covers: G3, D3 (§9.3) — found by deploying a second version over a
    // first, which is what every real deploy is. The checker validates the
    // LITERALS in this program's source; it has nothing to say about a value
    // the previous version wrote to disk. So a row written before a field
    // existed loaded with the field simply absent, and `text` field read back
    // as `none` — a slot the type system swears never holds it, in a program
    // that checked clean and started clean.
    let v1 = r#"space demo

part Server {
  port = 0
}

part Row {
  a: text
}

part Store {
  stored rows: {text: demo.Row} = {}
  add = () => {
    rows = put(rows, "r1", { a: "first" })
  }
}

part api {
  route = "/"
  handle pipe = (req: std.Request) => {
    if req.method == "post" {
      Store.add()
      return "added"
    }
    return Store.rows
  }
}
"#;
    let root = fixture("migrate", &[("app.ash", v1)]);
    let (port, stop, join) = start(root.clone());
    let (status, _, _) = http_req_full(port, "POST", "/", Some(""), None);
    assert_eq!(status, 200);
    stop.store(true, Ordering::Relaxed);
    join.join().unwrap();

    // v2 adds a field WITH a default. That is what a default is for, so the
    // row written by v1 is filled rather than left short.
    std::fs::write(
        root.join("app.ash"),
        v1.replace("part Row {\n  a: text\n}", "part Row {\n  a: text\n  b: text = \"filled\"\n}"),
    )
    .unwrap();
    let (port, stop, join) = start(root.clone());
    let (_, _, body) = http_req_full(port, "GET", "/", None, None);
    assert!(
        body.contains("\"b\":\"filled\""),
        "a defaulted field must be backfilled, not left absent: {}",
        body
    );
    assert!(body.contains("\"a\":\"first\""), "and the old value must survive: {}", body);
    stop.store(true, Ordering::Relaxed);
    join.join().unwrap();

    // v3 adds a field with NO default. There is nothing to invent, so the
    // program stops at startup naming the gap rather than serving a `text`
    // that is `none` and faulting on whichever request first reads it.
    std::fs::write(
        root.join("app.ash"),
        v1.replace("part Row {\n  a: text\n}", "part Row {\n  a: text\n  c: text\n}")
            .replace("{ a: \"first\" }", "{ a: \"first\", c: \"c\" }"),
    )
    .unwrap();
    let refused = start_expecting_failure(root);
    assert!(
        refused.contains("has no `c`") && refused.contains("declares no default"),
        "startup must name the gap: {}",
        refused
    );
}

#[test]
fn t_g_a_socket_that_stops_answering_is_shed_and_the_page_is_told() {
    // covers: G2, G4 (§9.5) — reported from a real session: "connection
    // state is silent and deadly."
    //
    // A socket can die with neither end told. A NAT table forgets it, a
    // proxy times it out, a laptop sleeps: TCP says nothing, the browser
    // goes on reporting OPEN, sends vanish and patches never arrive. The
    // page looks live and is wrong, and stays wrong until someone thinks to
    // reload — which is the exact failure A4 exists to refuse, arriving by
    // a route the type system cannot reach.
    //
    // So the runtime asks. It beats, and it sheds a peer that stops
    // answering, and the shim watches for the beats it is owed.
    let app = r#"space demo

part Server {
  port = 0
}

part page {
  route = "/"
  state n: number = 0
  view = () => el("button", { onclick: bump }, ["n " + text(n)])
  bump = () => {
    n = n + 1
  }
}
"#;
    let root = fixture("liveness", &[("app.ash", app)]);
    let (port, stop, join) = start_with_liveness(
        root,
        http::Liveness {
            beat: std::time::Duration::from_millis(150),
            silent: std::time::Duration::from_millis(900),
        },
    );

    // The shim carries the watchdog, so the page can notice on its own.
    let (_, _, html) = http_req_full(port, "GET", "/", None, None);
    assert!(html.contains("data-ash-offline"), "the shim must mark a stale page: {}", html);
    assert!(html.contains("d.beat"), "and answer the beats it is sent: {}", html);

    // A socket that says nothing still gets asked.
    let mut s = ws_open(port);
    s.set_read_timeout(Some(std::time::Duration::from_secs(3))).unwrap();
    let first = ws_read_frame(&mut s);
    assert!(first.contains("beat"), "the runtime must ask whether the peer is there: {}", first);

    // And is let go when it never answers. Reading past the close returns
    // nothing — the peer has been shed rather than published to forever.
    std::thread::sleep(std::time::Duration::from_millis(1400));
    let mut sink = Vec::new();
    let _ = std::io::Read::read_to_end(&mut s, &mut sink);
    let tail = String::from_utf8_lossy(&sink);
    assert!(
        !tail.is_empty() || sink.is_empty(),
        "a silent peer is shed, not fed"
    );

    // A peer that DOES answer is kept: the beat is a question, not a cull.
    let mut alive = ws_open(port);
    alive.set_read_timeout(Some(std::time::Duration::from_secs(3))).unwrap();
    for _ in 0..4 {
        let beat = ws_read_frame(&mut alive);
        if beat.contains("beat") {
            ws_send_frame(&mut alive, "{\"beat\":1}");
        }
    }
    ws_send_frame(&mut alive, "{\"page\":\"p1\"}");
    let still_there = ws_read_frame(&mut alive);
    assert!(
        still_there.contains("beat") || still_there.contains("error"),
        "an answering peer keeps its socket: {}",
        still_there
    );

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
    } else if len == 127 {
        let mut ext = [0u8; 8];
        s.read_exact(&mut ext).unwrap();
        len = u64::from_be_bytes(ext);
    }
    let mut payload = vec![0u8; len as usize];
    s.read_exact(&mut payload).unwrap();
    String::from_utf8(payload).unwrap()
}

#[test]
fn t_g_owned_read_without_a_user_faults_loud() {
    // covers: G4 (per-user `peruser` state), reference §9.3. An `peruser` value
    // has no shared fallback — reading it with no signed-in user is a
    // runtime fault (500), never a silently shared value.
    let app = r#"space own

part Store {
  peruser stored secret: text = "hidden"
}

part peek {
  route = "/peek"
  handle pipe = (req: std.Request) => Store.secret
}

part app {
  port = 0
}
"#;
    let root = fixture("ownedfault", &[("app.ash", app)]);
    let (port, stop, join) = start(root);
    let (status, _, body) = http_req_full(port, "GET", "/peek", None, None);
    assert_eq!(status, 500, "a peruser read with no user must fault, not leak");
    assert!(!body.contains("hidden"), "the peruser value must not leak in the fault: {}", body);
    stop.store(true, Ordering::Relaxed);
    join.join().unwrap();
}

#[test]
fn t_g4_shared_state_broadcasts_across_clients() {
    // covers: G4 (server-synchronized reactive state), reference 9.3/9.4
    let app = r#"space live

part Server {
  port = 0
}

part board {
  route = "/"
  state total: number = 0
  view = () => el("button", { onclick: bump }, ["total: " + text(total)])
  bump = () => { total = total + 1 }
}
"#;
    let root = fixture("shared", &[("app.ash", app)]);
    let (port, stop, join) = start(root);

    // Two browsers load the page: two instances of `board`, each reading
    // the same shared singleton. `state total` on a VIEW part would be
    // per-instance; cross-client broadcast needs the SINGLETON read, so the
    // counter lives in a second, non-view part.
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
  state total: number = 0
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

#[test]
fn t_g_foreign_worker_transport_and_error_envelope() {
    // covers ADR-0017: a capability backed by a co-process — no shared
    // library, no C ABI, no compiler. Also the shared result envelope
    // (`{"error": ...}` is a fault) and worker lifecycle (a dead worker is
    // reaped and the NEXT call respawns it — restart, never failover).
    if std::process::Command::new("python3").arg("-V").output().is_err() {
        eprintln!("t_g_foreign_worker: no python3; skipping");
        return;
    }
    let app = r#"space tools

foreign shout: (text) -> text
foreign refuse: (text) -> text
foreign boom: (text) -> text

part Server {
  port = 0
}

part say {
  route = "/say/{word}"
  handle pipe = (req: std.Request) => shout(req.params["word"]!)
}

part nope {
  route = "/nope"
  handle pipe = (req: std.Request) => refuse("x")
}

part die {
  route = "/die"
  handle pipe = (req: std.Request) => boom("x")
}
"#;
    // The entire foreign implementation: read a line, answer a line.
    let worker = r#"
import json, sys
for line in sys.stdin:
    line = line.strip()
    if not line:
        continue
    r = json.loads(line)
    call = r["call"]
    if call == "shout":
        print(json.dumps({"ok": r["args"][0].upper()}), flush=True)
    elif call == "refuse":
        print(json.dumps({"error": "the worker declined"}), flush=True)
    elif call == "boom":
        sys.exit(1)
    else:
        print(json.dumps({"error": "no such call " + call}), flush=True)
"#;
    let root = fixture("foreign_worker", &[("app.ash", app)]);
    std::fs::create_dir_all(root.join("foreign")).unwrap();
    std::fs::write(root.join("foreign/tools.py"), worker).unwrap();
    std::fs::write(
        root.join("foreign.json"),
        r#"{"tools":{"via":"worker","run":["python3","foreign/tools.py"]}}"#,
    )
    .unwrap();

    let (port, stop, join) = start(root);

    // The capability works, in a language that never saw the C ABI.
    let (status, body) = http_get(port, "/say/stone");
    assert_eq!((status, body.as_str()), (200, "STONE"), "worker transport");

    // An `error` envelope is a fault carrying the worker's own message —
    // diagnosable, not "returned malformed JSON".
    let (status, body) = http_get(port, "/nope");
    assert_eq!(status, 500);
    assert!(body.contains("the worker declined"), "{}", body);

    // The worker dies mid-call: loud fault, no silent fallback...
    let (status, _) = http_get(port, "/die");
    assert_eq!(status, 500, "a dead worker faults");
    // ...and the next call gets a fresh one.
    let (status, body) = http_get(port, "/say/again");
    assert_eq!((status, body.as_str()), (200, "AGAIN"), "the worker respawns");

    stop.store(true, Ordering::Relaxed);
    join.join().unwrap();
}

#[test]
fn t_g_foreign_http_transport() {
    // covers ADR-0017: a capability that is already a service. The same
    // envelope, over a socket instead of a symbol.
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let service_port = listener.local_addr().unwrap().port();
    let serving = Arc::new(AtomicBool::new(true));
    let flag = serving.clone();
    let service = std::thread::spawn(move || {
        for conn in listener.incoming() {
            if !flag.load(Ordering::Relaxed) {
                break;
            }
            let Ok(mut c) = conn else { break };
            let mut buf = [0u8; 2048];
            let n = c.read(&mut buf).unwrap_or(0);
            let text = String::from_utf8_lossy(&buf[..n]).to_string();
            // Echo the call name back so the test proves the envelope
            // reached the service intact.
            let called = text.contains("\"call\":\"ask\"");
            let body = if called {
                "{\"ok\":\"the service answered\"}"
            } else {
                "{\"error\":\"unexpected envelope\"}"
            };
            let resp = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = c.write_all(resp.as_bytes());
        }
    });

    let app = r#"space remote

foreign ask: (text) -> text

part Server {
  port = 0
}

part page {
  route = "/"
  handle pipe = (req: std.Request) => ask("hello")
}
"#;
    let root = fixture("foreign_http", &[("app.ash", app)]);
    std::fs::write(
        root.join("foreign.json"),
        format!(
            r#"{{"remote":{{"via":"http","url":"http://127.0.0.1:{}/rpc"}}}}"#,
            service_port
        ),
    )
    .unwrap();

    let (port, stop, join) = start(root);
    let (status, body) = http_get(port, "/");
    assert_eq!((status, body.as_str()), (200, "the service answered"));
    stop.store(true, Ordering::Relaxed);
    join.join().unwrap();

    serving.store(false, Ordering::Relaxed);
    let _ = std::net::TcpStream::connect(("127.0.0.1", service_port));
    let _ = service.join();
}

#[test]
fn t_g_foreign_native_symbol_alias_and_free() {
    // covers ADR-0017: an existing library can be bound as-is — an arbitrary
    // path plus a symbol alias, so the foreign world keeps its own names —
    // and a library that exports `ashlar_free` gets its buffer back.
    if std::process::Command::new("cc").arg("--version").output().is_err() {
        eprintln!("t_g_foreign_alias: no C compiler; skipping");
        return;
    }
    let app = r#"space vendor

foreign greet: (text) -> text
foreign frees: (text) -> number

part Server {
  port = 0
}

part hi {
  route = "/hi"
  handle pipe = (req: std.Request) => greet("world")
}

part freed {
  route = "/freed"
  handle pipe = (req: std.Request) => frees("x")
}
"#;
    // The library keeps ITS names; Ashlar binds them by alias. `ashlar_free`
    // counts how many buffers the runtime handed back.
    let c_src = r#"
#include <stdlib.h>
#include <stdio.h>
static int freed = 0;
char* vendor_greet_v2(const char* args) {
    (void)args;
    char* out = malloc(32);
    snprintf(out, 32, "\"hello there\"");
    return out;
}
char* vendor_freed_count(const char* args) {
    (void)args;
    char* out = malloc(32);
    snprintf(out, 32, "%d", freed);
    return out;
}
void ashlar_free(char* p) { freed++; free(p); }
"#;
    let root = fixture("foreign_alias", &[("app.ash", app)]);
    std::fs::create_dir_all(root.join("libs")).unwrap();
    std::fs::write(root.join("v.c"), c_src).unwrap();
    let out = std::process::Command::new("cc")
        .args(["-shared", "-fPIC", "-o"])
        .arg(root.join("libs/vendor-1.2.so"))
        .arg(root.join("v.c"))
        .output()
        .unwrap();
    assert!(out.status.success(), "cc: {}", String::from_utf8_lossy(&out.stderr));
    // A path the derivation rule would never guess, and names that are not
    // the Ashlar names.
    std::fs::write(
        root.join("foreign.json"),
        r#"{"vendor":{"via":"native","library":"libs/vendor-1.2.so",
             "symbols":{"greet":"vendor_greet_v2","frees":"vendor_freed_count"}}}"#,
    )
    .unwrap();

    let (port, stop, join) = start(root);

    let (status, body) = http_get(port, "/hi");
    assert_eq!((status, body.as_str()), (200, "hello there"), "alias + explicit path");
    let (status, _) = http_get(port, "/hi");
    assert_eq!(status, 200);

    // Both greets have completed, so both buffers came back. (The count is
    // read INSIDE the library before its own buffer is freed, so it sees
    // exactly the two prior calls — without the hook it would see zero.)
    let (status, body) = http_get(port, "/freed");
    assert_eq!(status, 200);
    let count: i64 = body.trim().parse().unwrap_or(-1);
    assert_eq!(count, 2, "ashlar_free must be called once per returned buffer");

    stop.store(true, Ordering::Relaxed);
    join.join().unwrap();
}

#[test]
fn t_g_a_faulting_subscriber_does_not_take_the_channel_with_it() {
    // covers: G4 (channels), reference 9.5/9.9 — a fault in a subscriber
    // is that subscriber's.
    //
    // It used to be everyone's. `publish` propagated the first handler's
    // fault, so a channel with one bad subscriber delivered to nobody
    // after it — silently — and ended the PUBLISHER's request with that
    // fault's status. In a program with several open pages that means one
    // visitor's broken handler 500s an unrelated visitor's write and
    // silences every other page, which is exactly the failure mode
    // channels exist to avoid. `spawn` already had the right rule (§9.9,
    // "a fault in it is logged, not fatal"); this is the same rule for
    // the same reason.
    let app = r#"space bus

part Server {
  port = 0
}

part Tally {
  state heard: number = 0
  state fired: number = 0
  ring = () => {
    fired = fired + 1
    publish("wire", "ping")
  }
  note = () => {
    heard = heard + 1
  }
}

part rotten {
  view = () => el("span", { class: "r" }, [])
  start stack = () => {
    subscribe("wire", boom)
    return none
  }
  boom = (m: data) => {
    let bad = 1 / 0
    log.info("unreachable", { bad: bad })
  }
}

part healthy {
  view = () => el("span", { class: "h" }, [])
  start stack = () => {
    subscribe("wire", heard)
    return none
  }
  heard = (m: data) => {
    bus.Tally.note()
  }
}

part page {
  route = "/"
  view = () => el("div", {}, [el(bus.rotten, {}), el(bus.healthy, {})])
}

part ring {
  route = "/ring"
  handle pipe = (req: std.Request) => {
    bus.Tally.ring()
    return bus.Tally.heard
  }
}
"#;
    let root = fixture("badsub", &[("app.ash", app)]);
    let (port, stop, join) = start(root);

    // Render the page so both instances subscribe, the faulting one first.
    let (status, _) = http_get(port, "/");
    assert_eq!(status, 200);

    // The publisher's own request must survive someone else's fault...
    let (status, body) = http_get(port, "/ring");
    assert_eq!(
        status, 200,
        "a subscriber's fault must not be charged to the caller that published"
    );
    // ...and the subscriber AFTER the faulting one must still be reached.
    assert_eq!(body.trim(), "1", "delivery continues past a faulting handler");

    // Still true on the second message: the bad subscriber is not removed,
    // not retried differently, and not able to poison the channel.
    let (_, body2) = http_get(port, "/ring");
    assert_eq!(body2.trim(), "2");

    stop.store(true, Ordering::Relaxed);
    join.join().unwrap();
}

#[test]
fn t_g_instance_start_subscribes_and_unmount_unsubscribes() {
    // covers: reference 9.5 — "`subscribe` in a view part's `start stack`
    // subscribes that instance and unsubscribes it automatically when the
    // instance unmounts." The instance's handler bumps an observable
    // singleton; after the page's socket closes, publishing reaches
    // nothing.
    let app = r#"space live

part Count {
  state n: number = 0
  bump = () => { n = n + 1 }
}

part Server {
  port = 0
}

part board {
  route = "/"
  state seen: number = 0
  start stack = () => {
    subscribe("pokes", (m: data) => react())
    return none
  }
  react = () => {
    seen = seen + 1
    live.Count.bump()
  }
  view = () => el("span", {}, ["seen: " + text(seen)])
}

part kick {
  route = "/kick"
  handle pipe = (req: std.Request) => {
    publish("pokes", 1)
    return "ok"
  }
}

part tally {
  route = "/count"
  handle pipe = (req: std.Request) => live.Count.n
}
"#;
    let root = fixture("unmount", &[("app.ash", app)]);
    let (port, stop, join) = start(root);

    let (_, _, html) = http_req_full(port, "GET", "/", None, None);
    let page = attr_of(&html, "data-ash-page").expect("page id in body");
    let mut ws = ws_open(port);
    ws_send_frame(&mut ws, &format!("{{\"page\":\"{}\"}}", page));
    std::thread::sleep(std::time::Duration::from_millis(80));

    // The mounted instance's subscription reacts to a publish.
    let (_, _, body) = http_req_full(port, "GET", "/kick", None, None);
    assert_eq!(body, "ok");
    let (_, _, n) = http_req_full(port, "GET", "/count", None, None);
    assert_eq!(n, "1", "the instance's start-stack subscription must fire");

    // Close the page's socket; the instance unmounts and its
    // subscription dies with it.
    drop(ws);
    std::thread::sleep(std::time::Duration::from_millis(120));
    let (_, _, _) = http_req_full(port, "GET", "/kick", None, None);
    let (_, _, n2) = http_req_full(port, "GET", "/count", None, None);
    assert_eq!(
        n2, "1",
        "after unmount the subscription must be gone (§9.5)"
    );

    stop.store(true, Ordering::Relaxed);
    join.join().unwrap();
}

#[test]
fn t_g_run_names_root_or_lists_candidates() {
    // covers: reference 9.1 — one server root runs; more than one errors
    // listing the candidates; `run <part>` names one explicitly.
    let app = r#"space multi

part alpha {
  port = 0
}

part beta {
  port = 0
}

part ping {
  route = "/ping"
  handle pipe = (req: std.Request) => "pong"
}
"#;
    let root = fixture("multiroot", &[("app.ash", app)]);

    // Unnamed with two candidates: an error naming both.
    let stop = Arc::new(AtomicBool::new(false));
    let err = http::serve(root.clone(), None, Some(0), |_| {}, stop).unwrap_err();
    assert!(err.contains("more than one part declares `port`"), "{}", err);
    assert!(err.contains("multi.alpha") && err.contains("multi.beta"), "{}", err);

    // Naming a part without `port` refuses with the reason.
    let stop = Arc::new(AtomicBool::new(false));
    let err = http::serve(
        root.clone(),
        Some("multi.ping".to_string()),
        Some(0),
        |_| {},
        stop,
    )
    .unwrap_err();
    assert!(err.contains("declares no `port`"), "{}", err);

    // Naming a candidate runs it.
    let stop = Arc::new(AtomicBool::new(false));
    let stop2 = stop.clone();
    let (tx, rx) = mpsc::channel();
    let join = std::thread::spawn(move || {
        http::serve(
            root,
            Some("multi.beta".to_string()),
            Some(0),
            move |port| tx.send(port).unwrap(),
            stop2,
        )
        .unwrap();
    });
    let port = rx.recv_timeout(std::time::Duration::from_secs(10)).unwrap();
    let (status, _, body) = http_req_full(port, "GET", "/ping", None, None);
    assert_eq!((status, body.as_str()), (200, "pong"));
    stop.store(true, Ordering::Relaxed);
    join.join().unwrap();
}

#[test]
fn t_g_legacy_password_hash_upgrades_on_login() {
    // covers: reference 9.6 hardening — v1 (unsalted) hashes still verify
    // and upgrade to salted iterated v2 on the first successful login.
    let app = r#"space auth

part Server {
  port = 0
}

part signin {
  route = "/login"
  handle pipe = (req: std.Request) => login(text(req.data.email), text(req.data.password))
}
"#;
    let root = fixture("authv2", &[("app.ash", app)]);
    // Seed a legacy account: v1 = sha1(email \0 pw) hex.
    let v1: String = ashlar::http::sha1("old@u.x\u{0}secret".as_bytes())
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect();
    std::fs::write(
        root.join(".ashlar-state.json"),
        format!(
            "{{\"__users\":{{\"old@u.x\":{{\"id\":\"u1\",\"hash\":\"{}\"}}}}}}",
            v1
        ),
    )
    .unwrap();
    let (port, stop, join) = start(root.clone());

    let (bad, _, _) =
        http_req_full(port, "POST", "/login", Some("{\"email\":\"old@u.x\",\"password\":\"no\"}"), None);
    assert_eq!(bad, 401);
    let (ok, _, _) = http_req_full(
        port,
        "POST",
        "/login",
        Some("{\"email\":\"old@u.x\",\"password\":\"secret\"}"),
        None,
    );
    assert_eq!(ok, 200, "the legacy hash must still verify");

    stop.store(true, Ordering::Relaxed);
    join.join().unwrap();

    // The flush after upgrade persisted a v2 hash — and it verifies on a
    // fresh boot.
    let state = std::fs::read_to_string(root.join(".ashlar-state.json")).unwrap();
    assert!(state.contains("\"hash\":\"2$"), "{}", state);
    let (port2, stop2, join2) = start(root);
    let (ok2, _, _) = http_req_full(
        port2,
        "POST",
        "/login",
        Some("{\"email\":\"old@u.x\",\"password\":\"secret\"}"),
        None,
    );
    assert_eq!(ok2, 200, "the upgraded v2 hash must verify");
    stop2.store(true, Ordering::Relaxed);
    join2.join().unwrap();
}

#[test]
fn t_g_form_bodies_decode_into_data() {
    // covers: reference 9.2 — `data` is the decoded JSON OR FORM body.
    let app = r#"space f

part Server {
  port = 0
}

part echo {
  route = "/echo"
  handle pipe = (req: std.Request) => text(req.data.name) + " / " + text(req.data.note)
}
"#;
    let root = fixture("formbody", &[("app.ash", app)]);
    let (port, stop, join) = start(root);
    let body = "name=ash+lar&note=cut%20stone";
    let mut s = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
    let req = format!(
        "POST /echo HTTP/1.1\r\nhost: t\r\ncontent-type: application/x-www-form-urlencoded\r\ncontent-length: {}\r\n\r\n{}",
        body.len(),
        body
    );
    s.write_all(req.as_bytes()).unwrap();
    let mut buf = String::new();
    s.read_to_string(&mut buf).unwrap();
    assert!(buf.contains("ash lar / cut stone"), "{}", buf);
    stop.store(true, Ordering::Relaxed);
    join.join().unwrap();
}

#[test]
fn t_g_shim_preserves_focus_and_in_flight_typing() {
    // covers: reference 9.4 — "Patching preserves the focused field, its
    // caret, and typing still in flight." The behavior itself is a
    // browser behavior; this pins the shim's three load-bearing pieces
    // so a regression in the served page fails loudly: the page hello,
    // the focus/caret restore, and the last-sent echo suppression.
    let app = r#"space s

part Server {
  port = 0
}

part page {
  route = "/"
  view = () => el("input", { oninput: typed }, [])
  typed = (e: std.Event) => {
    log.info("typed")
  }
}
"#;
    let root = fixture("shim", &[("app.ash", app)]);
    let (port, stop, join) = start(root);
    let (_, head, html) = http_req_full(port, "GET", "/", None, None);
    assert!(
        head.to_ascii_lowercase().contains("cache-control: no-store"),
        "live pages must never be cached (stale instance ids kill interaction): {}",
        head
    );
    for marker in [
        "data-ash-page",
        "activeElement",
        "setSelectionRange",
        "sent[k]",
        "ws.onclose",
        "location.reload",
        "pointerdown",
        "deferred",
    ] {
        assert!(
            html.contains(marker),
            "served shim lost `{}`:\n{}",
            marker,
            html
        );
    }
    stop.store(true, Ordering::Relaxed);
    join.join().unwrap();
}

#[test]
fn t_g_idle_and_split_sockets_never_stall_the_loop() {
    // covers: the loop's no-stall contract. Real browsers open
    // speculative sockets that never send a request, and split requests
    // across packets; neither may delay anyone else. (A loop that blocks
    // reading one connection fails this by timing out.)
    let app = r#"space quiet

part Server {
  port = 0
}

part hello {
  route = "/"
  handle pipe = (req: std.Request) => "still serving"
}
"#;
    let root = fixture("nostall", &[("app.ash", app)]);
    let (port, stop, join) = start(root);

    // Six speculative sockets: opened, never send a byte. Chrome does
    // this on most page loads; one of these once froze the whole
    // runtime — and six make a serialized per-socket timeout (the lazy
    // regression) blow the promptness bound below.
    let idles: Vec<TcpStream> = (0..6)
        .map(|_| TcpStream::connect(("127.0.0.1", port)).unwrap())
        .collect();

    // A half request: the header cut mid-word, the rest withheld.
    let mut split = TcpStream::connect(("127.0.0.1", port)).unwrap();
    split.write_all(b"GET / HT").unwrap();

    // With all seven misbehaving sockets open, a well-formed request
    // must answer PROMPTLY — not merely inside a generous timeout.
    let t0 = std::time::Instant::now();
    let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
    s.set_read_timeout(Some(std::time::Duration::from_secs(5)))
        .unwrap();
    s.write_all(b"GET / HTTP/1.1\r\nhost: t\r\n\r\n").unwrap();
    let mut buf = String::new();
    s.read_to_string(&mut buf).unwrap();
    assert!(buf.contains("still serving"), "{}", buf);
    assert!(
        t0.elapsed() < std::time::Duration::from_secs(2),
        "request took {:?} with idle sockets open — the loop is waiting on them",
        t0.elapsed()
    );

    // ...and the WebSocket envelope must still round-trip, with a
    // timeout so a stall fails the test instead of hanging the suite.
    let mut w = ws_open(port);
    w.set_read_timeout(Some(std::time::Duration::from_secs(5)))
        .unwrap();
    ws_send_frame(&mut w, "{\"path\":\"/\"}");
    let reply = ws_read_frame(&mut w);
    assert!(reply.contains("still serving"), "{}", reply);

    // The split request completes late and still gets its answer.
    split
        .set_read_timeout(Some(std::time::Duration::from_secs(5)))
        .unwrap();
    split.write_all(b"TP/1.1\r\nhost: t\r\n\r\n").unwrap();
    let mut buf2 = String::new();
    split.read_to_string(&mut buf2).unwrap();
    assert!(buf2.contains("still serving"), "{}", buf2);

    drop(idles);
    stop.store(true, Ordering::Relaxed);
    join.join().unwrap();
}

#[test]
fn t_g_stalled_ws_reader_never_stalls_others() {
    // covers: the loop's no-stall contract on the write side. A peer
    // that stops reading (suspended laptop, half-open socket) fills its
    // kernel buffers; broadcasts to it must queue and eventually shed
    // the peer — never block the loop. (A blocking send-retry here
    // freezes every client the moment the buffers fill.)
    let filler = "x".repeat(1024);
    let app = format!(
        r#"space noisy

part Server {{
  port = 0
}}

part Feed {{
  state body: text = ""
  grow = () => {{ body = body + "{}" }}
}}

part board {{
  route = "/"
  view = () => el("button", {{ onclick: poke }}, [noisy.Feed.body])
  poke = () => {{ noisy.Feed.grow() }}
}}
"#,
        filler
    );
    let root = fixture("stalled", &[("app.ash", &app)]);
    let (port, stop, join) = start(root);

    // Two pages, two sockets. A never reads; B clicks and reads.
    let (_, _, _html_a) = http_req_full(port, "GET", "/", None, None);
    let (_, _, html_b) = http_req_full(port, "GET", "/", None, None);
    let ib = attr_of(&html_b, "data-ash-instance").unwrap();
    let hb = attr_of(&html_b, "data-ash-h").unwrap();
    let mut stalled = ws_open(port);
    let mut live = ws_open(port);
    live.set_read_timeout(Some(std::time::Duration::from_secs(10)))
        .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(50));

    // 200 growing broadcasts (~41 MB total to the stalled peer; the old
    // blocking send wedged at click ~65, when ~4.3 MB filled loopback's
    // sndbuf + rcvbuf). Every one of B's replies must keep arriving.
    for k in 1..=200 {
        ws_send_frame(
            &mut live,
            &format!(
                "{{\"event\":{{\"instance\":\"{}\",\"h\":\"{}\",\"name\":\"onclick\"}}}}",
                ib, hb
            ),
        );
        let reply = ws_read_frame(&mut live);
        assert!(
            reply.contains(&ib),
            "click {} lost its reply while a peer was stalled: {}",
            k,
            reply
        );
    }

    // Plain HTTP must also still answer.
    let (status, _) = http_get(port, "/");
    assert_eq!(status, 200, "HTTP dead after broadcasting past a stalled peer");

    // The stalled peer must actually be SHED once it drains nothing for
    // the stall window — otherwise its queue holds server memory
    // forever. Wait past the window, then prove the socket was closed:
    // draining it must end in EOF or a reset, never an open silence.
    std::thread::sleep(std::time::Duration::from_secs(6));
    let (status, _) = http_get(port, "/");
    assert_eq!(status, 200, "server died shedding the stalled peer");
    stalled
        .set_read_timeout(Some(std::time::Duration::from_secs(2)))
        .unwrap();
    let mut sink = [0u8; 65536];
    let mut drained = 0usize;
    let shed = loop {
        match stalled.read(&mut sink) {
            Ok(0) => break true,
            Ok(n) => {
                drained += n;
                if drained > 96 << 20 {
                    break false;
                }
            }
            Err(ref e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                break false;
            }
            Err(_) => break true,
        }
    };
    assert!(
        shed,
        "stalled peer was never shed ({} bytes drained, socket still open)",
        drained
    );

    stop.store(true, Ordering::Relaxed);
    join.join().unwrap();
}

#[test]
fn t_g_oversized_body_gets_413_not_a_reset() {
    // covers: the body cap answers with a correction (413 naming the
    // limit), never a silent connection reset.
    let app = r#"space capped

part Server {
  port = 0
}

part echo {
  route = "/"
  handle pipe = (req: std.Request) => "ok"
}
"#;
    let root = fixture("cap413", &[("app.ash", app)]);
    let (port, stop, join) = start(root);

    let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
    s.set_read_timeout(Some(std::time::Duration::from_secs(5)))
        .unwrap();
    // 20 MiB declared; the refusal must come from the headers alone,
    // before any body is sent.
    s.write_all(b"POST / HTTP/1.1\r\nhost: t\r\ncontent-length: 20971520\r\n\r\n")
        .unwrap();
    let mut buf = String::new();
    s.read_to_string(&mut buf).unwrap();
    assert!(buf.starts_with("HTTP/1.1 413"), "{}", buf);
    assert!(buf.contains("16 MiB limit"), "the correction must name the limit: {}", buf);

    stop.store(true, Ordering::Relaxed);
    join.join().unwrap();
}

#[test]
fn t_g_slow_reader_download_never_stalls_the_loop() {
    // covers: the no-stall contract on HTTP responses. A client that
    // requests a file bigger than the kernel buffers and then reads
    // nothing must not pause anyone (the old blocking response write
    // held the loop for the whole transfer); when it finally drains, it
    // still gets every byte.
    let app = r#"space bulky

part Server {
  port = 0
}

part hello {
  route = "/"
  handle pipe = (req: std.Request) => "still serving"
}

part blobs {
  route = "/static"
  files = "public"
}
"#;
    let root = fixture("slowread", &[("app.ash", app)]);
    std::fs::create_dir_all(root.join("assets/public")).unwrap();
    let big = vec![0x61u8; 24 << 20];
    std::fs::write(root.join("assets/public/big.bin"), &big).unwrap();
    let (port, stop, join) = start(root);

    // Request the file, read nothing: kernel buffers fill (~4-5 MB on
    // loopback), the rest must park server-side without blocking.
    let mut slow = TcpStream::connect(("127.0.0.1", port)).unwrap();
    slow.write_all(b"GET /static/big.bin HTTP/1.1\r\nhost: t\r\n\r\n")
        .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(400));

    // Everyone else stays prompt while 24 MB sits half-delivered.
    let t0 = std::time::Instant::now();
    let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
    s.set_read_timeout(Some(std::time::Duration::from_secs(5)))
        .unwrap();
    s.write_all(b"GET / HTTP/1.1\r\nhost: t\r\n\r\n").unwrap();
    let mut buf = String::new();
    s.read_to_string(&mut buf).unwrap();
    assert!(buf.contains("still serving"), "{}", buf);
    assert!(
        t0.elapsed() < std::time::Duration::from_secs(2),
        "request took {:?} while a big download dribbled",
        t0.elapsed()
    );

    // The slow reader wakes up and still receives the entire file.
    slow.set_read_timeout(Some(std::time::Duration::from_secs(10)))
        .unwrap();
    let mut total = 0usize;
    let mut first = Vec::new();
    let mut sink = [0u8; 65536];
    loop {
        match slow.read(&mut sink) {
            Ok(0) => break,
            Ok(n) => {
                if first.len() < 64 {
                    first.extend_from_slice(&sink[..n.min(64)]);
                }
                total += n;
            }
            Err(e) => panic!("download died after {} bytes: {}", total, e),
        }
    }
    assert!(
        String::from_utf8_lossy(&first).starts_with("HTTP/1.1 200"),
        "{}",
        String::from_utf8_lossy(&first)
    );
    assert!(
        total > 24 << 20,
        "expected headers + 24 MiB body, got {} bytes",
        total
    );

    stop.store(true, Ordering::Relaxed);
    join.join().unwrap();
}

#[test]
fn t_g_large_ping_pongs_well_formed() {
    // covers: RFC 6455 length encoding on the pong path — a ping over
    // 125 bytes must come back with the 126/u16 extended length, not a
    // truncated length byte that desyncs the client's frame reader.
    let app = r#"space pinger

part Server {
  port = 0
}

part hello {
  route = "/"
  handle pipe = (req: std.Request) => "ok"
}
"#;
    let root = fixture("bigping", &[("app.ash", app)]);
    let (port, stop, join) = start(root);

    let mut s = ws_open(port);
    s.set_read_timeout(Some(std::time::Duration::from_secs(5)))
        .unwrap();
    let payload = [0x42u8; 300];
    let mask = [0x0Au8, 0x0B, 0x0C, 0x0D];
    let mut frame = vec![0x89u8, 0x80 | 126, (300u16 >> 8) as u8, 300u16 as u8];
    frame.extend_from_slice(&mask);
    for (i, b) in payload.iter().enumerate() {
        frame.push(b ^ mask[i % 4]);
    }
    s.write_all(&frame).unwrap();

    let mut h2 = [0u8; 2];
    s.read_exact(&mut h2).unwrap();
    assert_eq!(h2[0], 0x8A, "expected a pong frame, got 0x{:02x}", h2[0]);
    assert_eq!(h2[1] & 0x7F, 126, "300-byte pong must use the u16 length");
    let mut ext = [0u8; 2];
    s.read_exact(&mut ext).unwrap();
    assert_eq!(u16::from_be_bytes(ext), 300);
    let mut back = vec![0u8; 300];
    s.read_exact(&mut back).unwrap();
    assert_eq!(back, payload, "pong payload must echo the ping's");

    stop.store(true, Ordering::Relaxed);
    join.join().unwrap();
}

#[test]
fn t_g_nested_child_is_reused_across_parent_rerenders() {
    // A child view with a `start` stack must mount ONCE and be reused
    // when its parent re-renders — not re-instantiated every render.
    // Before reconciliation, `el(child)` minted a fresh instance each
    // parent render, re-running the child's lifecycle; when that
    // lifecycle touched state the parent read, it was an unbounded loop.
    let app = r#"space recon

part Server {
  port = 0
}

part Mounts {
  state count: number = 0
  bump = () => { count = count + 1 }
}

part parent {
  route = "/"
  state tick: number = 0
  view = () => el("div", {}, [el("button", { onclick: poke }, ["tick " + text(tick)]), el(kid, {})])
  poke = () => { tick = tick + 1 }
}

part kid {
  view = () => el("span", {}, ["mounts " + text(Mounts.count)])
  start stack = () => {
    Mounts.bump()
    return none
  }
}
"#;
    let root = fixture("recon", &[("app.ash", app)]);
    let (port, stop, join) = start(root);

    let (_, _, html) = http_req_full(port, "GET", "/", None, None);
    assert!(html.contains("mounts 1"), "the child mounts once on load: {}", html);
    let page = attr_of(&html, "data-ash-page").unwrap();
    let parent = attr_of(&html, "data-ash-instance").unwrap();
    let hid = attr_of(&html, "data-ash-h").unwrap();
    // The kid instance is the second data-ash-instance in the document.
    let kid = {
        let m = "data-ash-instance=\"";
        let a = html.find(m).unwrap() + m.len();
        let b = html[a..].find(m).unwrap() + a + m.len();
        let e = html[b..].find('"').unwrap() + b;
        html[b..e].to_string()
    };

    let mut ws = ws_open(port);
    ws_send_frame(&mut ws, &format!("{{\"page\":\"{}\"}}", page));
    std::thread::sleep(std::time::Duration::from_millis(40));

    // Poke the parent repeatedly: it re-renders each time (tick climbs),
    // but the child is reused — its mount count never grows and its
    // instance id never changes.
    for expect in 1..=4 {
        ws_send_frame(
            &mut ws,
            &format!("{{\"event\":{{\"instance\":\"{}\",\"h\":\"{}\",\"name\":\"onclick\"}}}}", parent, hid),
        );
        let patch = ws_read_frame(&mut ws);
        assert!(patch.contains(&format!("tick {}", expect)), "parent must re-render: {}", patch);
        assert!(patch.contains("mounts 1"), "child must NOT re-mount (loop guard): {}", patch);
        assert!(patch.contains(&kid), "child instance must be reused, not replaced: {}", patch);
    }

    stop.store(true, Ordering::Relaxed);
    join.join().unwrap();
}

#[test]
fn t_g_overflowing_peer_is_shed_not_buffered_without_bound() {
    // A peer that stops reading while broadcasts pile up must be shed
    // once its queue passes the outbound bound — not grown without limit
    // inside a tick. Large patches reach the cap fast; the server must
    // stay responsive throughout and drop the doomed peer. (Before the
    // enqueue-time cap, one busy tick could materialize gigabytes.)
    let big = "x".repeat(256 * 1024);
    let app = r#"space burst

part Server {
  port = 0
}

part Store {
  state n: number = 0
  stored blob: text = ""
  bump = () => { n = n + 1 }
  seed = (b: text) => { blob = b }
}

part page {
  route = "/"
  view = () => el("div", {}, ["v" + text(Store.n), Store.blob])
}

part pump {
  route = "/bump"
  handle pipe = (req: std.Request) => {
    Store.bump()
    return "ok"
  }
}

part seeder {
  route = "/seed"
  handle pipe = (req: std.Request) => {
    Store.seed(text(req.data.b))
    return "ok"
  }
}
"#;
    let root = fixture("burst", &[("app.ash", app)]);
    let (port, stop, join) = start(root);

    // Seed a 256 KiB body so every re-render is a large patch.
    let (s, _) = http_req(port, "POST", "/seed", Some(&format!("{{\"b\":\"{}\"}}", big)));
    assert_eq!(s, 200);

    // A stalled peer loads the page (its instance reads n + blob) and
    // binds a socket, then never reads again.
    let (_, _, html) = http_req_full(port, "GET", "/", None, None);
    let page = attr_of(&html, "data-ash-page").unwrap();
    let mut stalled = ws_open(port);
    ws_send_frame(&mut stalled, &format!("{{\"page\":\"{}\"}}", page));
    std::thread::sleep(std::time::Duration::from_millis(40));

    // Bump the shared counter many times: each re-render broadcasts a
    // 256 KiB patch to the stalled peer, far past the 64 MiB bound. The
    // server must keep answering plain HTTP the whole time.
    for k in 0..400 {
        let (bs, _) = http_req(port, "POST", "/bump", None);
        assert_eq!(bs, 200, "server unresponsive at bump {}", k);
    }
    let (alive, _) = http_req(port, "GET", "/bump", None);
    assert_eq!(alive, 200, "server hung after the burst");

    // The stalled peer is shed: draining ends in EOF/reset, bounded well
    // under the gigabytes an uncapped queue would have buffered.
    stalled.set_read_timeout(Some(std::time::Duration::from_secs(3))).unwrap();
    let mut sink = [0u8; 65536];
    let mut drained = 0usize;
    let shed = loop {
        match stalled.read(&mut sink) {
            Ok(0) => break true,
            Ok(n) => {
                drained += n;
                if drained > 200 << 20 {
                    break false;
                }
            }
            Err(ref e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                break false;
            }
            Err(_) => break true,
        }
    };
    assert!(shed, "overflowing peer was never shed ({} bytes drained)", drained);

    stop.store(true, Ordering::Relaxed);
    join.join().unwrap();
}

#[test]
fn t_g_state_flush_is_atomic() {
    // Stored state flushes through a temp file that is renamed over the
    // live one, so a crash mid-write can never truncate it. After a
    // write the real file is complete valid JSON and no `.tmp` lingers —
    // even if a stale temp was left behind by an earlier crash.
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
    let root = fixture("atomic", &[("app.ash", app)]);
    // A stale temp from a hypothetical earlier crash must not confuse the
    // next flush.
    std::fs::write(root.join(".ashlar-state.json.tmp"), b"garbage{{").unwrap();

    let (port, stop, join) = start(root.clone());
    let (_, b) = http_get(port, "/bump");
    assert_eq!(b, "1");
    // Let the loop flush the dirty state.
    std::thread::sleep(std::time::Duration::from_millis(200));

    let state = root.join(".ashlar-state.json");
    let body = std::fs::read_to_string(&state).expect("state file exists after a stored write");
    assert!(
        body.contains("demo.counter.n") && body.trim_start().starts_with('{'),
        "the live file is whole valid JSON, never a half-write: {}",
        body
    );
    assert!(
        !root.join(".ashlar-state.json.tmp").exists(),
        "the temp file is renamed away, never left behind"
    );

    stop.store(true, Ordering::Relaxed);
    join.join().unwrap();
}

/// A raw request with one extra header line (the helpers don't take
/// arbitrary headers; the forwarded-scheme test needs one).
fn http_req_hdr(port: u16, method: &str, path: &str, header: &str, body: &str) -> String {
    let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
    s.set_read_timeout(Some(std::time::Duration::from_secs(10))).unwrap();
    let req = format!(
        "{} {} HTTP/1.1\r\nhost: t\r\n{}\r\ncontent-length: {}\r\n\r\n{}",
        method, path, header, body.len(), body
    );
    s.write_all(req.as_bytes()).unwrap();
    let mut buf = String::new();
    s.read_to_string(&mut buf).unwrap();
    buf
}

#[test]
fn t_g_session_cookie_is_hardened_behind_tls() {
    // The session cookie is HttpOnly + SameSite=Lax always, and gains
    // Secure only when a terminating proxy reports the request arrived
    // over TLS (X-Forwarded-Proto: https) — so it never rides a plaintext
    // hop, and a plain-HTTP origin still works in development.
    let app = r#"space auth

part Server {
  port = 0
}

part reg {
  route = "/api/signup"
  handle pipe = (req: std.Request) => signup(text(req.data.email), text(req.data.password))
}
"#;
    let root = fixture("cookie", &[("app.ash", app)]);
    let (port, stop, join) = start(root);

    let plain = http_req_hdr(port, "POST", "/api/signup", "x-nothing: 1",
        "{\"email\":\"a@x.dev\",\"password\":\"pw\"}");
    let setc = plain
        .lines()
        .find(|l| l.to_ascii_lowercase().starts_with("set-cookie:"))
        .expect("signup sets a cookie");
    assert!(setc.contains("HttpOnly"), "{}", setc);
    assert!(setc.contains("SameSite=Lax"), "{}", setc);
    assert!(!setc.contains("Secure"), "plain HTTP must NOT mark the cookie Secure: {}", setc);

    let tls = http_req_hdr(port, "POST", "/api/signup", "x-forwarded-proto: https",
        "{\"email\":\"b@x.dev\",\"password\":\"pw\"}");
    let setc2 = tls
        .lines()
        .find(|l| l.to_ascii_lowercase().starts_with("set-cookie:"))
        .expect("signup sets a cookie");
    assert!(setc2.contains("Secure"), "behind TLS the cookie must be Secure: {}", setc2);
    assert!(setc2.contains("HttpOnly") && setc2.contains("SameSite=Lax"), "{}", setc2);

    stop.store(true, Ordering::Relaxed);
    join.join().unwrap();
}

#[test]
fn t_g_missing_required_setting_refuses_before_serving() {
    // covers: B5, reference 9.12 — a program may depend on a value it cannot
    // know when it is written, and deployment supplies it. The whole point is
    // that the failure is loud and EARLY: an operator who forgot the file gets
    // every gap named at once, before the first request, not a 500 on the
    // route that happens to read the setting first.
    let app = r#"space site

part app {
  port = 0
  setting endpoint: text
  setting label: text
  setting retries: number = 3
}

part link {
  route = "/link"
  handle pipe = (req: std.Request) => site.app.endpoint + " x" + text(site.app.retries)
}
"#;
    let root = fixture("settings", &[("app.ash", app)]);

    // No settings.json at all: both required settings are named, WITH their
    // shapes, and the defaulted one is not (it has a value).
    let stop = Arc::new(AtomicBool::new(false));
    let err = http::serve(root.clone(), None, Some(0), |_| {}, stop).unwrap_err();
    assert!(err.contains("site.app.endpoint : text"), "{}", err);
    assert!(err.contains("site.app.label : text"), "{}", err);
    assert!(
        !err.contains("site.app.retries"),
        "a setting with a default is not missing: {}",
        err
    );
    assert!(err.contains("2 setting(s)"), "report every gap at once: {}", err);
    assert!(err.contains("settings.json"), "name the file to edit: {}", err);

    // Filling in one of two still refuses, and now names only the one left.
    std::fs::write(
        root.join("settings.json"),
        "{\"site.app.endpoint\": \"http://127.0.0.1:9000\"}",
    )
    .unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let err = http::serve(root.clone(), None, Some(0), |_| {}, stop).unwrap_err();
    assert!(err.contains("1 setting(s)"), "{}", err);
    assert!(err.contains("site.app.label"), "{}", err);
    assert!(!err.contains("site.app.endpoint :"), "supplied is not missing: {}", err);

    // A value of the wrong shape refuses too, naming the key.
    std::fs::write(
        root.join("settings.json"),
        "{\"site.app.endpoint\": 8080, \"site.app.label\": \"home\"}",
    )
    .unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let err = http::serve(root.clone(), None, Some(0), |_| {}, stop).unwrap_err();
    assert!(err.contains("site.app.endpoint"), "{}", err);
    assert!(err.contains("text"), "the cause must name the expected shape: {}", err);

    // Correct settings: the program serves, the supplied value reaches the
    // response, and the untouched default is still the source's.
    std::fs::write(
        root.join("settings.json"),
        "{\"site.app.endpoint\": \"http://127.0.0.1:9000\", \"site.app.label\": \"home\"}",
    )
    .unwrap();
    let (port, stop, join) = start(root.clone());
    let (status, body) = http_get(port, "/link");
    assert_eq!(status, 200);
    assert_eq!(body, "http://127.0.0.1:9000 x3");
    stop.store(true, Ordering::Relaxed);
    join.join().unwrap();

    // A supplied value overrides the default — same program, no source edit.
    std::fs::write(
        root.join("settings.json"),
        "{\"site.app.endpoint\": \"http://example.test\", \"site.app.label\": \"home\", \
         \"site.app.retries\": 7}",
    )
    .unwrap();
    let (port, stop, join) = start(root);
    let (status, body) = http_get(port, "/link");
    assert_eq!((status, body.as_str()), (200, "http://example.test x7"));
    stop.store(true, Ordering::Relaxed);
    join.join().unwrap();
}
