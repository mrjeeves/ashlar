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
