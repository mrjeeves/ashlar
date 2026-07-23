//! The examples directory is showcase AND corpus, at two depths:
//!
//! 1. Every example checks with zero diagnostics and is canonically
//!    formatted — a broken example is a test failure, not a discovery.
//! 2. Every example RUNS: each one is copied to a temp dir, served on an
//!    ephemeral port, and driven through its real HTTP/WebSocket surface.
//!    The showcase is the regression suite.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};

fn examples_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples")
}

// -- static depth: compile-clean and canonical ------------------------------

#[test]
fn t_examples_all_check_clean() {
    let root = examples_root();
    let mut seen = 0;
    for entry in std::fs::read_dir(&root).expect("examples/ exists") {
        let dir = entry.unwrap().path();
        if !dir.is_dir() {
            continue;
        }
        seen += 1;
        let r = ashlar::check_project(&dir);
        assert!(
            r.diags.is_empty(),
            "example `{}` has diagnostics:\n{}",
            dir.display(),
            r.diags.iter().map(|d| d.human()).collect::<Vec<_>>().join("\n")
        );
        assert!(
            !r.program.parts.is_empty(),
            "example `{}` declares no parts",
            dir.display()
        );
    }
    assert!(seen >= 7, "expected the full example set, found {}", seen);
}

#[test]
fn t_examples_are_canonically_formatted() {
    let root = examples_root();
    for entry in std::fs::read_dir(&root).expect("examples/ exists") {
        let dir = entry.unwrap().path();
        if !dir.is_dir() {
            continue;
        }
        for file in ashlar::find_ash_files(&dir) {
            let src = std::fs::read_to_string(&file).unwrap();
            let rel = file.to_string_lossy().to_string();
            let formatted = ashlar::fmt::format_source(&rel, &src)
                .unwrap_or_else(|d| panic!("{} does not format: {:?}", rel, d));
            assert_eq!(
                formatted, src,
                "{} is not canonically formatted; run `ashlar fmt examples`",
                rel
            );
        }
    }
}

// -- runtime depth: every example served and driven -------------------------

/// Copy an example into a temp dir (runtime writes state files; the
/// tree ships source only).
fn staged(name: &str) -> PathBuf {
    let src = examples_root().join(name);
    let dst = std::env::temp_dir().join(format!("ashlar_ex_{}_{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&dst);
    std::fs::create_dir_all(&dst).unwrap();
    for f in ashlar::find_ash_files(&src) {
        let rel = f.strip_prefix(&src).unwrap();
        if let Some(dir) = dst.join(rel).parent() {
            std::fs::create_dir_all(dir).unwrap();
        }
        std::fs::copy(&f, dst.join(rel)).unwrap();
    }
    dst
}

fn start(root: PathBuf) -> (u16, Arc<AtomicBool>, std::thread::JoinHandle<()>) {
    let stop = Arc::new(AtomicBool::new(false));
    let stop2 = stop.clone();
    let (tx, rx) = mpsc::channel();
    let join = std::thread::spawn(move || {
        let r = ashlar::http::serve(root, None, Some(0), move |port| tx.send(port).unwrap(), stop2);
        if let Err(e) = r {
            panic!("serve failed: {}", e);
        }
    });
    let port = rx.recv_timeout(std::time::Duration::from_secs(10)).unwrap();
    (port, stop, join)
}

fn req(port: u16, method: &str, path: &str, body: Option<&str>, cookie: Option<&str>) -> (u16, String, String) {
    let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
    let body = body.unwrap_or("");
    let cookie_line = cookie.map(|c| format!("cookie: ashsession={}\r\n", c)).unwrap_or_default();
    let text = format!(
        "{} {} HTTP/1.1\r\nhost: t\r\n{}content-length: {}\r\n\r\n{}",
        method, path, cookie_line, body.len(), body
    );
    s.write_all(text.as_bytes()).unwrap();
    let mut buf = String::new();
    s.read_to_string(&mut buf).unwrap();
    let status: u16 = buf.split_whitespace().nth(1).and_then(|v| v.parse().ok()).unwrap_or(0);
    let mut parts = buf.splitn(2, "\r\n\r\n");
    let head = parts.next().unwrap_or("").to_string();
    let body = parts.next().unwrap_or("").to_string();
    (status, head, body)
}

fn attr_of(html: &str, attr: &str) -> Option<String> {
    let marker = format!("{}=\"", attr);
    let start = html.find(&marker)? + marker.len();
    let end = html[start..].find('"')? + start;
    Some(html[start..end].to_string())
}

/// The handler id attached to the element wired for `kind`.
fn handler_for(html: &str, kind: &str) -> Option<String> {
    let marker = format!("data-ash-on=\"{}\"", kind);
    let at = html.find(&marker)?;
    attr_of(&html[at..], "data-ash-h")
}

/// The instance owning the handler wired for `kind`: the CLOSEST
/// enclosing `data-ash-instance`, exactly as the browser shim resolves
/// it with `.closest()` — nested views own their own handlers.
fn instance_for(html: &str, kind: &str) -> Option<String> {
    let marker = format!("data-ash-on=\"{}\"", kind);
    let at = html.find(&marker)?;
    let before = &html[..at];
    let inst_at = before.rfind("data-ash-instance=\"")?;
    attr_of(&before[inst_at..], "data-ash-instance")
}

/// WS payloads carry JSON-escaped HTML; unescape before attr searches.
fn unescape(s: &str) -> String {
    s.replace("\\\"", "\"")
}

fn ws_open(port: u16) -> TcpStream {
    let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
    let req = "GET / HTTP/1.1\r\nhost: t\r\nupgrade: websocket\r\nconnection: Upgrade\r\nsec-websocket-key: dGhlIHNhbXBsZSBub25jZQ==\r\nsec-websocket-version: 13\r\n\r\n";
    s.write_all(req.as_bytes()).unwrap();
    let mut buf = [0u8; 1024];
    let n = s.read(&mut buf).unwrap();
    assert!(String::from_utf8_lossy(&buf[..n]).contains("101"), "handshake");
    s
}

fn ws_send(s: &mut TcpStream, text: &str) {
    // Client frames are masked (RFC 6455); mask key zero keeps it simple.
    let payload = text.as_bytes();
    let mut frame = vec![0x81u8];
    if payload.len() < 126 {
        frame.push(0x80 | payload.len() as u8);
    } else {
        frame.push(0x80 | 126);
        frame.extend_from_slice(&(payload.len() as u16).to_be_bytes());
    }
    frame.extend_from_slice(&[0, 0, 0, 0]);
    frame.extend_from_slice(payload);
    s.write_all(&frame).unwrap();
}

fn ws_read(s: &mut TcpStream) -> String {
    let mut head = [0u8; 2];
    s.read_exact(&mut head).unwrap();
    let mut len = (head[1] & 0x7f) as usize;
    if len == 126 {
        let mut ext = [0u8; 2];
        s.read_exact(&mut ext).unwrap();
        len = u16::from_be_bytes(ext) as usize;
    }
    let mut payload = vec![0u8; len];
    s.read_exact(&mut payload).unwrap();
    String::from_utf8_lossy(&payload).to_string()
}

#[test]
fn t_examples_hello_serves() {
    let dir = staged("hello");
    let (port, stop, join) = start(dir.clone());
    let (status, _, body) = req(port, "GET", "/", None, None);
    assert_eq!((status, body.as_str()), (200, "hello from ashlar"));
    stop.store(true, Ordering::Relaxed);
    join.join().unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn t_examples_counter_clicks() {
    let dir = staged("counter");
    let (port, stop, join) = start(dir.clone());
    let (_, _, html) = req(port, "GET", "/", None, None);
    assert!(html.contains("clicks: 0"), "{}", html);
    let inst = instance_for(&html, "onclick").unwrap();
    let h = handler_for(&html, "onclick").unwrap();
    let mut ws = ws_open(port);
    ws_send(
        &mut ws,
        &format!("{{\"event\":{{\"instance\":\"{}\",\"h\":\"{}\",\"name\":\"onclick\"}}}}", inst, h),
    );
    let reply = ws_read(&mut ws);
    assert!(reply.contains("clicks: 1"), "{}", reply);
    stop.store(true, Ordering::Relaxed);
    join.join().unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn t_examples_chat_posts_persist_and_react() {
    let dir = staged("chat");
    let (port, stop, join) = start(dir.clone());
    let (status, _, body) =
        req(port, "POST", "/api/post", Some("{\"author\":\"m\",\"body\":\"first stone\"}"), None);
    assert_eq!((status, body.as_str()), (200, "ok"));
    let (_, _, list) = req(port, "GET", "/api/messages", None, None);
    assert!(list.contains("first stone"), "{}", list);
    let (_, _, page) = req(port, "GET", "/", None, None);
    assert!(page.contains("messages: 1"), "{}", page);

    // `stored` survives a restart (§9.3).
    stop.store(true, Ordering::Relaxed);
    join.join().unwrap();
    let (port2, stop2, join2) = start(dir.clone());
    let (_, _, list2) = req(port2, "GET", "/api/messages", None, None);
    assert!(list2.contains("first stone"), "restart lost stored state: {}", list2);
    stop2.store(true, Ordering::Relaxed);
    join2.join().unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn t_examples_todo_form_round_trip() {
    let dir = staged("todo");
    let (port, stop, join) = start(dir.clone());
    let (_, _, html) = req(port, "GET", "/", None, None);
    let inst = instance_for(&html, "oninput").unwrap();
    let typed = handler_for(&html, "oninput").unwrap();
    let mut ws = ws_open(port);
    ws_send(
        &mut ws,
        &format!(
            "{{\"event\":{{\"instance\":\"{}\",\"h\":\"{}\",\"name\":\"oninput\",\"value\":\"milk\"}}}}",
            inst, typed
        ),
    );
    let after_typing = unescape(&ws_read(&mut ws));
    // The patched form carries the fresh handler ids; submit through them.
    let submit = handler_for(&after_typing, "onsubmit").unwrap();
    ws_send(
        &mut ws,
        &format!("{{\"event\":{{\"instance\":\"{}\",\"h\":\"{}\",\"name\":\"onsubmit\"}}}}", inst, submit),
    );
    let after_submit = ws_read(&mut ws);
    assert!(after_submit.contains("todo: milk"), "{}", after_submit);
    stop.store(true, Ordering::Relaxed);
    join.join().unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn t_examples_diary_guards_by_session() {
    let dir = staged("diary");
    let (port, stop, join) = start(dir.clone());
    let (no_auth, _, _) = req(port, "GET", "/private", None, None);
    assert_eq!(no_auth, 403, "the allow guard must reject anonymous requests");

    let (status, head, _) = req(
        port,
        "POST",
        "/api/signup",
        Some("{\"email\":\"me@diary.x\",\"password\":\"pw\"}"),
        None,
    );
    assert_eq!(status, 200);
    let cookie = head
        .lines()
        .find(|l| l.to_ascii_lowercase().starts_with("set-cookie:"))
        .and_then(|l| l.split("ashsession=").nth(1))
        .map(|v| v.split(';').next().unwrap_or(v).trim().to_string())
        .expect("signup sets the session cookie");

    let (ok, _, body) = req(port, "GET", "/private", None, Some(&cookie));
    assert_eq!(ok, 200);
    assert!(body.contains("me@diary.x"), "{}", body);

    let (_, _, bye) = req(port, "GET", "/api/logout", None, Some(&cookie));
    assert_eq!(bye, "bye");
    let (after, _, _) = req(port, "GET", "/private", None, Some(&cookie));
    assert_eq!(after, 403, "logout must end the session server-side");

    stop.store(true, Ordering::Relaxed);
    join.join().unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn t_examples_press_merges_all_kinds() {
    let dir = staged("press");
    let (port, stop, join) = start(dir.clone());
    let (_, _, config) = req(port, "GET", "/api/config", None, None);
    assert!(config.contains("core") && config.contains("markdown"), "append: {}", config);
    assert!(config.contains("size") && config.contains("depth"), "deep: {}", config);
    let (_, _, rendered) =
        req(port, "POST", "/api/render", Some("{\"body\":\"hi\"}"), None);
    assert_eq!(rendered, "<p>hi</p>", "pipe layers must chain base-first");
    stop.store(true, Ordering::Relaxed);
    join.join().unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn t_examples_ticker_schedule_drives_state() {
    let dir = staged("ticker");
    let (port, stop, join) = start(dir.clone());
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let mut beats = 0.0;
    while std::time::Instant::now() < deadline {
        let (_, _, body) = req(port, "GET", "/api/beats", None, None);
        beats = body.trim().parse().unwrap_or(0.0);
        if beats > 0.0 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    assert!(beats > 0.0, "the schedule never fired");
    stop.store(true, Ordering::Relaxed);
    join.join().unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}
