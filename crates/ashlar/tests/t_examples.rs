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
    assert!(seen >= 10, "expected the full example set, found {}", seen);
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

/// Copy an example into a temp dir (runtime writes state files; the tree
/// ships source only). The whole project copies — `.ash` and any
/// `assets/` (a declared stylesheet must be present or the server
/// refuses to start), minus runtime artifacts.
fn staged(name: &str) -> PathBuf {
    let src = examples_root().join(name);
    let dst = std::env::temp_dir().join(format!("ashlar_ex_{}_{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&dst);
    std::fs::create_dir_all(&dst).unwrap();
    copy_tree(&src, &dst);
    dst
}

fn copy_tree(src: &std::path::Path, dst: &std::path::Path) {
    for entry in std::fs::read_dir(src).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        if name == ".ashlar-state.json" || name == "ashlar.manifest" || name.starts_with('.') {
            continue;
        }
        let target = dst.join(&name);
        if path.is_dir() {
            std::fs::create_dir_all(&target).unwrap();
            copy_tree(&path, &target);
        } else {
            std::fs::copy(&path, &target).unwrap();
        }
    }
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

/// The instance owning the `nth` handler wired for `kind`, resolved
/// exactly as the browser shim does with `.closest('[data-ash-instance]')`:
/// the handler element ITSELF if it carries the marker (a view whose root
/// is the interactive element — a bare button, a link), otherwise the
/// nearest ancestor. A sibling instance that closed before the element
/// must not win, so this walks real tag nesting (the renderer closes
/// every element explicitly).
fn event_target(html: &str, kind: &str, nth: usize) -> Option<(String, String)> {
    let marker = format!("data-ash-on=\"{}\"", kind);
    let mut at = 0;
    for _ in 0..=nth {
        at = html[at..].find(&marker)? + at + marker.len();
    }
    let h = attr_of(&html[at..], "data-ash-h")?;
    let open_at = html[..at].rfind('<')?;
    // The handler element's own opening tag may carry the instance
    // marker (stamped onto a view's root element) — `.closest` starts
    // at the element, so check it first.
    let self_gt = html[open_at..].find('>').map(|p| p + open_at)?;
    if let Some(id) = attr_of(&html[open_at..=self_gt], "data-ash-instance") {
        return Some((id, h));
    }
    // Otherwise walk the tags before it for the nearest open ancestor.
    let mut stack: Vec<Option<String>> = Vec::new();
    let mut i = 0;
    while i < open_at {
        let Some(lt) = html[i..open_at].find('<').map(|p| p + i) else {
            break;
        };
        let Some(gt) = html[lt..].find('>').map(|p| p + lt) else {
            break;
        };
        let tag = &html[lt..=gt];
        if tag.starts_with("</") {
            stack.pop();
        } else if !tag.starts_with("<!") {
            stack.push(attr_of(tag, "data-ash-instance"));
        }
        i = gt + 1;
    }
    let instance = stack.iter().rev().find_map(|s| s.clone())?;
    Some((instance, h))
}

/// WS payloads carry JSON-escaped HTML; unescape before attr searches.
fn unescape(s: &str) -> String {
    s.replace("\\\"", "\"")
}

/// Read frames until one contains `needle` (the runtime broadcasts every
/// patch set; clients filter by instance id, so a watcher may see other
/// pages' patches first).
fn ws_expect(s: &mut TcpStream, needle: &str, max_frames: usize) -> String {
    let mut last = String::new();
    for _ in 0..max_frames {
        last = unescape(&ws_read(s));
        if last.contains(needle) {
            return last;
        }
    }
    panic!("no frame contained `{}`; last was: {}", needle, last);
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
    let (inst, h) = event_target(&html, "onclick", 0).unwrap();
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
    assert!(page.contains("m: first stone"), "the feed must render rows: {}", page);

    // Drive the compose form as client A while client B watches: name,
    // message, submit — B's feed re-renders from A's post (§9.3).
    let (_, _, html_a) = req(port, "GET", "/", None, None);
    let page_a = attr_of(&html_a, "data-ash-page").unwrap();
    let (_, _, html_b) = req(port, "GET", "/", None, None);
    let page_b = attr_of(&html_b, "data-ash-page").unwrap();
    let mut a = ws_open(port);
    let mut b = ws_open(port);
    ws_send(&mut a, &format!("{{\"page\":\"{}\"}}", page_a));
    ws_send(&mut b, &format!("{{\"page\":\"{}\"}}", page_b));
    std::thread::sleep(std::time::Duration::from_millis(80));

    let (inst, named) = event_target(&html_a, "oninput", 0).unwrap();
    ws_send(&mut a, &format!("{{\"event\":{{\"instance\":\"{}\",\"h\":\"{}\",\"name\":\"oninput\",\"value\":\"ada\"}}}}", inst, named));
    let after_name = unescape(&ws_read(&mut a));
    let (_, typed) = event_target(&after_name, "oninput", 1).unwrap();
    ws_send(&mut a, &format!("{{\"event\":{{\"instance\":\"{}\",\"h\":\"{}\",\"name\":\"oninput\",\"value\":\"hello stone\"}}}}", inst, typed));
    let after_draft = unescape(&ws_read(&mut a));
    let (_, submit) = event_target(&after_draft, "onsubmit", 0).unwrap();
    ws_send(&mut a, &format!("{{\"event\":{{\"instance\":\"{}\",\"h\":\"{}\",\"name\":\"onsubmit\"}}}}", inst, submit));
    let posted = ws_expect(&mut a, "ada: hello stone", 5);
    assert!(posted.contains("ada: hello stone"), "{}", posted);
    ws_expect(&mut b, "ada: hello stone", 8);
    drop(a);
    drop(b);

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
    let (inst, typed) = event_target(&html, "oninput", 0).unwrap();
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
    let (_, submit) = event_target(&after_typing, "onsubmit", 0).unwrap();
    ws_send(
        &mut ws,
        &format!("{{\"event\":{{\"instance\":\"{}\",\"h\":\"{}\",\"name\":\"onsubmit\"}}}}", inst, submit),
    );
    let after_submit = ws_read(&mut ws);
    assert!(after_submit.contains(">milk<"), "the committed item renders as a list row: {}", after_submit);
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

    // The `/` view is a login gate for anonymous visitors and the reader
    // for a signed-in one — identity crossing from the request into the
    // view (§9.4).
    let (anon_home, _, gate) = req(port, "GET", "/", None, None);
    assert_eq!(anon_home, 200);
    assert!(gate.contains("create an account"), "anonymous sees the gate: {}", gate);
    let (auth_home, _, reader) = req(port, "GET", "/", None, Some(&cookie));
    assert_eq!(auth_home, 200);
    assert!(reader.contains("me@diary.x"), "the reader greets the member: {}", reader);

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

    // The `/` view runs the composed pipe live: the default draft renders
    // base-first then the markdown layer, right in the page (§9.4).
    let (home, _, studio) = req(port, "GET", "/", None, None);
    assert_eq!(home, 200);
    assert!(studio.contains("&lt;p&gt;hello&lt;/p&gt;"), "the composed pipe renders in the view: {}", studio);

    stop.store(true, Ordering::Relaxed);
    join.join().unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn t_examples_poll_channel_feeds_instances() {
    let dir = staged("poll");
    let (port, stop, join) = start(dir.clone());

    // HTTP surface: vote, then list.
    let (status, _, body) =
        req(port, "POST", "/api/vote", Some("{\"option\":\"granite\"}"), None);
    assert_eq!((status, body.as_str()), (200, "ok"));
    let (_, _, votes) = req(port, "GET", "/api/votes", None, None);
    assert!(votes.contains("granite"), "{}", votes);

    // A fresh page reads the shared tally, but `latest` is per-instance:
    // votes cast before the instance existed are not replayed into it.
    let (_, _, html) = req(port, "GET", "/", None, None);
    assert!(html.contains("granite 1"), "{}", html);
    assert!(html.contains("last vote: none yet"), "{}", html);

    // Register the page's socket, then click the first button (granite).
    let page_id = attr_of(&html, "data-ash-page").unwrap();
    let mut ws = ws_open(port);
    ws_send(&mut ws, &format!("{{\"page\":\"{}\"}}", page_id));
    std::thread::sleep(std::time::Duration::from_millis(80));
    let (inst, pick) = event_target(&html, "onclick", 0).unwrap();
    ws_send(
        &mut ws,
        &format!("{{\"event\":{{\"instance\":\"{}\",\"h\":\"{}\",\"name\":\"onclick\"}}}}", inst, pick),
    );
    let clicked = ws_expect(&mut ws, "last vote: granite", 5);
    assert!(clicked.contains("granite 2"), "tally must re-render with the vote: {}", clicked);

    // An HTTP vote reaches the view through the channel alone: `latest`
    // is per-instance state no code in this request assigns, so a patch
    // carrying it can only be the instance's subscription firing (§9.5).
    let (_, _, ok2) = req(port, "POST", "/api/vote", Some("{\"option\":\"marble\"}"), None);
    assert_eq!(ok2, "ok");
    let pushed = ws_expect(&mut ws, "last vote: marble", 8);
    assert!(pushed.contains("marble 1"), "{}", pushed);

    stop.store(true, Ordering::Relaxed);
    join.join().unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn t_examples_pong_syncs_across_two_windows() {
    // Two pages, two sockets: A starting the game must flip B's button
    // and animate B's ball; B pausing must reach A. The whole game is
    // one shared state — windows are just observers with sliders.
    let dir = staged("pong");
    let (port, stop, join) = start(dir.clone());

    let (_, _, html_a) = req(port, "GET", "/", None, None);
    let page_a = attr_of(&html_a, "data-ash-page").unwrap();
    let (_, _, html_b) = req(port, "GET", "/", None, None);
    let page_b = attr_of(&html_b, "data-ash-page").unwrap();
    let mut a = ws_open(port);
    let mut b = ws_open(port);
    ws_send(&mut a, &format!("{{\"page\":\"{}\"}}", page_a));
    ws_send(&mut b, &format!("{{\"page\":\"{}\"}}", page_b));
    std::thread::sleep(std::time::Duration::from_millis(80));

    // A starts the game.
    let (ainst, aflip) = event_target(&html_a, "onclick", 0).unwrap();
    ws_send(
        &mut a,
        &format!("{{\"event\":{{\"instance\":\"{}\",\"h\":\"{}\",\"name\":\"onclick\"}}}}", ainst, aflip),
    );
    // B's switch instance must receive its own 'pause' patch, and B's
    // field must receive moving-ball patches from the schedule.
    let (binst_switch, bflip) = event_target(&html_b, "onclick", 0).unwrap();
    let b_flip_patch = ws_expect(&mut b, &binst_switch, 12);
    assert!(b_flip_patch.contains("pause"), "{}", b_flip_patch);
    let one = ws_expect(&mut b, "border-radius", 30);
    let two = ws_expect(&mut b, "border-radius", 30);
    assert_ne!(one, two, "B's ball must animate from A's start");

    // B pauses; A must see 'start' again on ITS switch instance.
    ws_send(
        &mut b,
        &format!("{{\"event\":{{\"instance\":\"{}\",\"h\":\"{}\",\"name\":\"onclick\"}}}}", binst_switch, bflip),
    );
    let (ainst_switch, _) = event_target(&html_a, "onclick", 0).unwrap();
    // A's socket carries its own earlier reply and 20fps ball frames;
    // drain until the frame that patches A's switch back to "start".
    let mut seen = String::new();
    let mut flipped = false;
    for _ in 0..200 {
        seen = unescape(&ws_read(&mut a));
        if seen.contains(&ainst_switch) && seen.contains(">start<") {
            flipped = true;
            break;
        }
    }
    assert!(flipped, "A must see B's pause; last frame: {}", seen);
    let _ = ainst;

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

#[test]
fn t_examples_pong_plays() {
    let dir = staged("pong");
    let (port, stop, join) = start(dir.clone());
    let (_, _, html) = req(port, "GET", "/", None, None);
    assert!(html.contains("pong —"), "{}", html);
    assert!(html.contains("type=\"range\""), "{}", html);

    // Paused at serve position.
    let (_, _, s0) = req(port, "GET", "/api/state", None, None);
    assert!(s0.contains("\"running\":false") && s0.contains("\"x\":195"), "{}", s0);

    // Steer the left paddle with a slider event.
    let (inst, steer) = event_target(&html, "oninput", 0).unwrap();
    let mut ws = ws_open(port);
    ws_send(
        &mut ws,
        &format!("{{\"event\":{{\"instance\":\"{}\",\"h\":\"{}\",\"name\":\"oninput\",\"value\":\"40\"}}}}", inst, steer),
    );
    let _ = ws_read(&mut ws);
    let (_, _, s1) = req(port, "GET", "/api/state", None, None);
    assert!(s1.contains("\"pl\":40"), "the slider must steer the paddle: {}", s1);

    // Start: the schedule drives the ball; pause: it stops.
    let (binst, flip) = event_target(&html, "onclick", 0).unwrap();
    ws_send(
        &mut ws,
        &format!("{{\"event\":{{\"instance\":\"{}\",\"h\":\"{}\",\"name\":\"onclick\"}}}}", binst, flip),
    );
    let _ = ws_read(&mut ws);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let mut moved = false;
    while std::time::Instant::now() < deadline {
        let (_, _, s) = req(port, "GET", "/api/state", None, None);
        if s.contains("\"running\":true") && !s.contains("\"x\":195") {
            moved = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(60));
    }
    assert!(moved, "the ball must move while running");

    // Pause through the fresh button handler (the switch re-rendered).
    let (_, _, page2) = req(port, "GET", "/", None, None);
    let (binst2, flip2) = event_target(&page2, "onclick", 0).unwrap();
    ws_send(
        &mut ws,
        &format!("{{\"event\":{{\"instance\":\"{}\",\"h\":\"{}\",\"name\":\"onclick\"}}}}", binst2, flip2),
    );
    let _ = ws_read(&mut ws);
    std::thread::sleep(std::time::Duration::from_millis(150));
    let (_, _, a) = req(port, "GET", "/api/state", None, None);
    std::thread::sleep(std::time::Duration::from_millis(200));
    let (_, _, b) = req(port, "GET", "/api/state", None, None);
    assert!(a.contains("\"running\":false"), "{}", a);
    assert_eq!(a, b, "paused means the ball holds still");

    stop.store(true, Ordering::Relaxed);
    join.join().unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn t_examples_foundry_background_work_patches_view() {
    let dir = staged("foundry");
    let (port, stop, join) = start(dir.clone());

    let (_, _, html) = req(port, "GET", "/", None, None);
    assert!(html.contains("waiting: 0"), "{}", html);
    assert!(html.contains("finished: "), "{}", html);
    let page_id = attr_of(&html, "data-ash-page").unwrap();
    let mut ws = ws_open(port);
    ws_send(&mut ws, &format!("{{\"page\":\"{}\"}}", page_id));
    std::thread::sleep(std::time::Duration::from_millis(80));

    let (status, _, accepted) =
        req(port, "POST", "/api/jobs", Some("{\"brief\":\"cut release\"}"), None);
    assert_eq!(status, 200);
    assert!(accepted.contains("cut release"), "{}", accepted);

    let pushed = ws_expect(&mut ws, "finished: cut release", 6);
    assert!(pushed.contains("waiting: 0"), "{}", pushed);
    let (_, _, state) = req(port, "GET", "/api/status", None, None);
    assert!(state.contains("cut release"), "{}", state);

    stop.store(true, Ordering::Relaxed);
    join.join().unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn t_examples_guardrails_layers_typed_policies() {
    let dir = staged("guardrails");
    let (port, stop, join) = start(dir.clone());

    let (ok, _, clean) =
        req(port, "POST", "/api/review", Some("{\"body\":\"ship it\"}"), None);
    assert_eq!(ok, 200);
    assert!(clean.contains("\"allowed\":true"), "{}", clean);

    let (_, _, blocked) =
        req(port, "POST", "/api/review", Some("{\"body\":\"share the secret\"}"), None);
    assert!(blocked.contains("\"allowed\":false"), "{}", blocked);
    assert!(blocked.contains("contains secret"), "{}", blocked);

    let (_, _, layered) = req(
        port,
        "POST",
        "/api/review",
        Some("{\"body\":\"this secret is much too long to pass\"}"),
        None,
    );
    assert!(layered.contains("over 24 characters"), "{}", layered);
    assert!(layered.contains("contains secret"), "{}", layered);

    // The `/` view runs the composed policy pipe live: the default draft
    // trips both layered checks, decided right in the page (§9.4).
    let (home, _, checker) = req(port, "GET", "/", None, None);
    assert_eq!(home, 200);
    assert!(checker.contains("blocked"), "the view shows the composed decision: {}", checker);
    assert!(
        checker.contains("over 24 characters") && checker.contains("contains secret"),
        "both layered policies decide in the view: {}",
        checker
    );

    stop.store(true, Ordering::Relaxed);
    join.join().unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}

/// Pull the session token out of a Set-Cookie header.
fn cookie_of(head: &str) -> String {
    head.lines()
        .find(|l| l.to_ascii_lowercase().starts_with("set-cookie:"))
        .and_then(|l| l.split("ashsession=").nth(1))
        .map(|v| v.split(';').next().unwrap_or(v).trim().to_string())
        .expect("a session cookie")
}

#[test]
fn t_examples_commons_is_a_live_team_chat() {
    // The flagship: auth, a live cross-client feed, an independently
    // owned moderation layer, cross-space @mentions over a channel, and
    // presence driven by the mount/unmount lifecycle — one product
    // exercising the whole language. Handlers are transport-invisible, so
    // the test posts JSON where a browser posts a form; same routes.
    let dir = staged("commons");
    let (port, stop, join) = start(dir.clone());

    // Two people sign up; each gets a session (§9.6).
    let (s1, h1, _) = req(port, "POST", "/api/signup",
        Some("{\"name\":\"Ada\",\"email\":\"ada@team.dev\",\"password\":\"stone\"}"), None);
    assert_eq!(s1, 302, "signup redirects");
    let ada = cookie_of(&h1);
    let (_, h2, _) = req(port, "POST", "/api/signup",
        Some("{\"name\":\"Bob\",\"email\":\"bob@team.dev\",\"password\":\"slate\"}"), None);
    let bob = cookie_of(&h2);

    // The gate is what a logged-out visitor sees; the shell is what a
    // member sees, with their name resolved from their id — the request
    // identity crossing into the view (§9.4).
    let (_, _, anon) = req(port, "GET", "/", None, None);
    assert!(anon.contains("class=\"gate\""), "logged-out sees the gate");
    assert!(anon.contains("/commons.css"), "the declared stylesheet is linked into the head");
    let (_, _, shell_a) = req(port, "GET", "/", None, Some(&ada));
    assert!(shell_a.contains("class=\"sidebar\""), "a member sees the shell");
    assert!(shell_a.contains("Ada"), "the shell greets the member by name");
    assert!(shell_a.contains("general"), "the seeded room is listed");

    // The stylesheet serves as a real asset at the linked path.
    let (css_status, css_head, css_body) = req(port, "GET", "/commons.css", None, None);
    assert_eq!(css_status, 200);
    assert!(css_head.to_ascii_lowercase().contains("text/css"), "{}", css_head);
    assert!(css_body.contains(".sidebar"), "the sheet is the real CSS");

    // Both open the general room; each render mounts a presence probe and
    // a notice tray, and binds a live socket to its page.
    let (_, _, room_a) = req(port, "GET", "/c/general", None, Some(&ada));
    let (_, _, room_b) = req(port, "GET", "/c/general", None, Some(&bob));
    let page_a = attr_of(&room_a, "data-ash-page").unwrap();
    let page_b = attr_of(&room_b, "data-ash-page").unwrap();
    let mut ws_a = ws_open(port);
    let mut ws_b = ws_open(port);
    ws_send(&mut ws_a, &format!("{{\"page\":\"{}\"}}", page_a));
    ws_send(&mut ws_b, &format!("{{\"page\":\"{}\"}}", page_b));
    std::thread::sleep(std::time::Duration::from_millis(80));

    // Presence: Ada's sidebar now lists Bob as online (his page mounted,
    // his socket is live). The lobby has no message feed, so his name can
    // only come from the online list.
    let (_, _, lobby) = req(port, "GET", "/", None, Some(&ada));
    assert!(lobby.contains("Bob"), "presence: Bob shows online in Ada's sidebar:\n{}", lobby);

    // Bob composes a message that trips two independently owned spaces at
    // once: it @mentions Ada, and it contains a redacted word.
    let (binst, typed) = event_target(&room_b, "oninput", 0).unwrap();
    let (sinst, send) = event_target(&room_b, "onsubmit", 0).unwrap();
    ws_send(&mut ws_b, &format!(
        "{{\"event\":{{\"instance\":\"{}\",\"h\":\"{}\",\"name\":\"oninput\",\"value\":\"hey @Ada check the spoiler\"}}}}",
        binst, typed));
    let _ = ws_read(&mut ws_b);
    ws_send(&mut ws_b, &format!(
        "{{\"event\":{{\"instance\":\"{}\",\"h\":\"{}\",\"name\":\"onsubmit\"}}}}",
        sinst, send));

    // One event, three reactions reach Ada in one broadcast: her feed
    // re-renders with Bob's post (cross-client reactivity on `stored`),
    // the body is redacted (commons.moderation's `prepare` layer ran),
    // and a mention toast appears (commons.mentions published to Ada's
    // channel, her notice tray was subscribed — two spaces meeting at a
    // channel name, §9.5).
    let frame = ws_expect(&mut ws_a, "mentioned you", 12);
    assert!(frame.contains("[redacted]"), "moderation must redact the body: {}", frame);
    assert!(!frame.contains("spoiler"), "the raw word must not survive: {}", frame);
    assert!(frame.contains("Bob mentioned you"), "the mention names the sender: {}", frame);

    // Bob sees his own message land too.
    let mine = ws_expect(&mut ws_b, "[redacted]", 6);
    assert!(mine.contains("check the"), "{}", mine);

    // Presence departs with the socket: Bob closing his page unmounts it,
    // the stop stack runs, and Ada's sidebar drops him.
    drop(ws_b);
    std::thread::sleep(std::time::Duration::from_millis(120));
    let (_, _, lobby2) = req(port, "GET", "/", None, Some(&ada));
    assert!(!lobby2.contains("Bob"), "presence: Bob departs when his socket closes:\n{}", lobby2);

    drop(ws_a);
    stop.store(true, Ordering::Relaxed);
    join.join().unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}

/// Build an example's foreign shim into a host library beside its source.
/// Returns false (with a loud note) when the toolchain or libsqlite3 is
/// absent — a SQLite integration cannot be tested without SQLite, so the
/// caller skips rather than fail an unrelated machine's whole suite.
fn build_foreign_shim(dir: &std::path::Path, space: &str, crate_name: &str, link: &str) -> bool {
    let src = dir.join("foreign").join(format!("{}.rs", space));
    let so = dir.join("foreign").join(format!("{}.so", space));
    let out = std::process::Command::new("rustc")
        .args(["--edition", "2021", "--crate-name", crate_name, "--crate-type", "cdylib", "-l", link, "-o"])
        .arg(&so)
        .arg(&src)
        .output();
    match out {
        Ok(o) if o.status.success() && so.exists() => true,
        other => {
            let why = other
                .map(|o| String::from_utf8_lossy(&o.stderr).into_owned())
                .unwrap_or_else(|e| e.to_string());
            eprintln!("SKIP: cannot build foreign shim `{}` (needs a Rust toolchain + lib{}):\n{}", space, link, why);
            false
        }
    }
}

#[test]
fn t_examples_ledger_persists_to_sqlite() {
    // The datastore is a REAL SQLite database file, reached across the
    // `foreign` boundary (§9.10) — the first example to exercise foreign.
    // The shim is a std-only Rust cdylib linking the system libsqlite3; the
    // SQL lives there, never in Ashlar source (ADR-0014). No Ashlar runtime
    // change: this rides the boundary that already exists.
    let dir = staged("ledger");
    if !build_foreign_shim(&dir, "ledger.store", "ledger_store", "sqlite3") {
        let _ = std::fs::remove_dir_all(&dir);
        return;
    }

    // The shim's datastore path is a deployment fact, not source (B5): it
    // reads ASHLAR_LEDGER_DB, else a per-process temp file. Unset here, so
    // it takes the fallback — start from a clean file.
    let db = std::env::temp_dir().join(format!("ashlar-ledger-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&db);

    let (port, stop, join) = start(dir.clone());

    // Two entries through one handler; a client posts JSON where a browser
    // posts a form (transport-invisible, §9.2).
    let (s1, _, _) = req(port, "POST", "/add", Some("{\"who\":\"ada\",\"note\":\"coffee\",\"amount\":4.5}"), None);
    assert_eq!(s1, 302, "the add handler redirects back to the board");
    let (s2, _, _) = req(port, "POST", "/add", Some("{\"who\":\"bob\",\"note\":\"bagels\",\"amount\":6}"), None);
    assert_eq!(s2, 302);

    // The board renders straight from SQLite: both rows newest-first, and
    // the running total, which SQL sums inside the shim.
    let (_, _, page) = req(port, "GET", "/", None, None);
    assert!(page.contains("ada: coffee ($4.5)"), "row read back from SQLite: {}", page);
    assert!(page.contains("bob: bagels ($6)"), "row read back from SQLite: {}", page);
    assert!(page.contains("total: $10.5"), "the SQL SUM crosses the boundary: {}", page);
    assert!(
        page.find("bob").unwrap() < page.find("ada").unwrap(),
        "newest first (ORDER BY id DESC): {}",
        page
    );

    // The file on disk is a genuine SQLite database, not an Ashlar blob.
    let bytes = std::fs::read(&db).expect("the SQLite file exists");
    assert!(bytes.starts_with(b"SQLite format 3\0"), "a real SQLite database file");

    // Restart: a fresh evaluator holds none of these entries in memory, so
    // their surviving proves they were read back from the database — the
    // datastore genuinely lives outside the program.
    stop.store(true, Ordering::Relaxed);
    join.join().unwrap();
    let (port2, stop2, join2) = start(dir.clone());
    let (_, _, page2) = req(port2, "GET", "/", None, None);
    assert!(page2.contains("ada: coffee ($4.5)"), "restart lost the SQLite data: {}", page2);
    assert!(page2.contains("total: $10.5"), "{}", page2);

    stop2.store(true, Ordering::Relaxed);
    join2.join().unwrap();
    let _ = std::fs::remove_file(&db);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn t_examples_locker_scopes_storage_per_user() {
    // `owned stored` gives each signed-in user their own isolated, persisted
    // data (ADR-0015). Proven here: anonymous access is refused, two users
    // never see each other's notes, and the data survives a restart keyed by
    // the persisted account id.
    let dir = staged("locker");
    let (port, stop, join) = start(dir.clone());

    // Anonymous cannot reach owned storage — the `allow` guard rejects it
    // before the read would even fault.
    let (anon, _, _) = req(port, "GET", "/api/notes", None, None);
    assert_eq!(anon, 403, "anonymous is refused the owned read");

    // Two users sign up; each gets a session.
    let (_, ha, _) = req(port, "POST", "/api/signup",
        Some("{\"email\":\"ada@keep.x\",\"password\":\"p\"}"), None);
    let ada = cookie_of(&ha);
    let (_, hb, _) = req(port, "POST", "/api/signup",
        Some("{\"email\":\"bob@keep.x\",\"password\":\"p\"}"), None);
    let bob = cookie_of(&hb);

    // Each keeps a different note in their own locker.
    req(port, "POST", "/api/keep", Some("{\"note\":\"ada-secret\"}"), Some(&ada));
    req(port, "POST", "/api/keep", Some("{\"note\":\"bob-secret\"}"), Some(&bob));

    // Each sees ONLY their own — the owned isolation, by construction.
    let (_, _, an) = req(port, "GET", "/api/notes", None, Some(&ada));
    assert!(an.contains("ada-secret") && !an.contains("bob-secret"),
        "ada sees only her own notes: {}", an);
    let (_, _, bn) = req(port, "GET", "/api/notes", None, Some(&bob));
    assert!(bn.contains("bob-secret") && !bn.contains("ada-secret"),
        "bob sees only his own notes: {}", bn);

    // The `/` view: a gate for anonymous, the live board for a member —
    // whose owned notes render right in the page, isolated (§9.3).
    let (anon_home, _, gate) = req(port, "GET", "/", None, None);
    assert_eq!(anon_home, 200);
    assert!(gate.contains("class=\"stack\""), "anonymous sees the gate: {}", gate);
    let (_, _, board) = req(port, "GET", "/", None, Some(&ada));
    assert!(board.contains("ada-secret") && !board.contains("bob-secret"),
        "the board renders only this user's owned notes: {}", board);

    // owned stored survives a restart. Sessions do not persist, so log in
    // again — the account (and its stable id) does, and the notes keyed by
    // that id come back, still isolated.
    stop.store(true, Ordering::Relaxed);
    join.join().unwrap();
    let (port2, stop2, join2) = start(dir.clone());
    let (_, h, _) = req(port2, "POST", "/api/login",
        Some("{\"email\":\"ada@keep.x\",\"password\":\"p\"}"), None);
    let ada2 = cookie_of(&h);
    let (_, _, a2) = req(port2, "GET", "/api/notes", None, Some(&ada2));
    assert!(a2.contains("ada-secret") && !a2.contains("bob-secret"),
        "restart kept ada's owned notes, still isolated: {}", a2);

    stop2.store(true, Ordering::Relaxed);
    join2.join().unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}
