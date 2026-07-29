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
    // Unique per CALL, not per process. Tests in one binary run in parallel
    // threads, and two of them stage `pong`; keyed by pid alone both resolved
    // to the same directory, so one test's `remove_dir_all` could wipe the
    // tree the other was already serving from. That is a harness race whose
    // window moves with build profile — exactly the shape of a test that
    // passes alone and fails in a full release run.
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let seq = NEXT.fetch_add(1, Ordering::Relaxed);
    let dst = std::env::temp_dir().join(format!(
        "ashlar_ex_{}_{}_{}",
        name,
        std::process::id(),
        seq
    ));
    let _ = std::fs::remove_dir_all(&dst);
    std::fs::create_dir_all(&dst).unwrap();
    copy_tree(&src, &dst);
    // A project that vendored the mesh library reaches the node this machine
    // runs. The suite has none and must not touch one it finds, so every
    // staged copy is pointed at a socket that is not there — the site serves
    // its empty-roster state, and no test depends on what the machine running
    // it installed. The two tests that are ABOUT the mesh rebind afterwards.
    if dst.join("vendor/mesh/mesh.ash").is_file() {
        bind_mesh_to(&dst, &dst.join("no-node-here.sock"));
    }
    dst
}

/// Bind the `mesh` space to this toolchain's own worker, pointed at a socket
/// this test controls. Naming the socket is what stops the worker starting a
/// node of its own — so a machine with AllMyStuff installed runs these tests
/// without its real daemon being touched, and one without it runs them the
/// same way.
fn bind_mesh_to(dir: &std::path::Path, socket: &std::path::Path) {
    std::fs::write(
        dir.join("foreign.json"),
        format!(
            "{{\n  \"mesh\": {{ \"via\": \"worker\", \"run\": [{:?}, \"mesh\", \"worker\", {:?}] }}\n}}\n",
            env!("CARGO_BIN_EXE_ashlar"),
            socket.to_string_lossy()
        ),
    )
    .unwrap();
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
        let r = ashlar::http::serve(root, None, Some(0), move |port, _| tx.send(port).unwrap(), stop2);
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

/// Like `req`, but for a body that is not text. An icon is bytes, and
/// reading bytes into a String is how the first version of this assertion
/// failed.
fn req_bytes(port: u16, path: &str) -> (u16, String, Vec<u8>) {
    let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
    let text = format!("GET {} HTTP/1.1\r\nhost: t\r\ncontent-length: 0\r\n\r\n", path);
    s.write_all(text.as_bytes()).unwrap();
    let mut raw = Vec::new();
    s.read_to_end(&mut raw).unwrap();
    let split = raw
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .map(|i| i + 4)
        .unwrap_or(raw.len());
    let head = String::from_utf8_lossy(&raw[..split]).to_string();
    let status: u16 = head
        .split_whitespace()
        .nth(1)
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    (status, head, raw[split..].to_vec())
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

/// One request whose answer is not text. `req` reads a `String`, which a
/// JPEG is not — and an image served from a room is exactly the case worth
/// asserting on the bytes.

fn ws_read(s: &mut TcpStream) -> String {
    let mut head = [0u8; 2];
    s.read_exact(&mut head).unwrap();
    let mut len = (head[1] & 0x7f) as u64;
    if len == 126 {
        let mut ext = [0u8; 2];
        s.read_exact(&mut ext).unwrap();
        len = u16::from_be_bytes(ext) as u64;
    } else if len == 127 {
        // RFC 6455's 8-byte length. This arm was missing, and its absence
        // was invisible until an example rendered enough HTML to cross
        // 64KiB in one frame: the reader then took 127 as the length,
        // desynchronised, and every later frame was garbage — so a test
        // looking for a patch simply never found it, with no error. The
        // runtime broadcasts a patch set to every socket, so frame size
        // grows with the number of pages a test has opened, which is why
        // the boards with the most pages hit it first. t_g's copy of this
        // helper has always handled it.
        let mut ext = [0u8; 8];
        s.read_exact(&mut ext).unwrap();
        len = u64::from_be_bytes(ext);
    }
    let mut payload = vec![0u8; len as usize];
    s.read_exact(&mut payload).unwrap();
    String::from_utf8_lossy(&payload).to_string()
}

#[test]
fn t_examples_hello_serves() {
    let dir = staged("hello");
    let (port, stop, join) = start(dir.clone());
    // Returning text answers the request with text, and that is still the
    // smallest thing a route can do (§9.2).
    let (status, _, body) = req(port, "GET", "/text", None, None);
    assert_eq!((status, body.as_str()), (200, "hello from ashlar"));

    // The page counts whoever has it open, off the view lifecycle alone
    // (§9.4). One page, and it says so.
    let (status, _, html) = req(port, "GET", "/", None, None);
    assert_eq!(status, 200);
    assert!(html.contains("hello from ashlar"), "{}", html);
    let page_a = attr_of(&html, "data-ash-page").unwrap();
    let mut a = ws_open(port);
    ws_send(&mut a, &format!("{{\"page\":\"{}\"}}", page_a));
    std::thread::sleep(std::time::Duration::from_millis(80));

    // A second window arrives, and the FIRST one is told — nobody asked it
    // to look. That is reactivity on a shared `state` (§9.3), and the whole
    // of the bookkeeping is `start` and `stop`.
    let (_, _, html_b) = req(port, "GET", "/", None, None);
    let page_b = attr_of(&html_b, "data-ash-page").unwrap();
    let mut b = ws_open(port);
    ws_send(&mut b, &format!("{{\"page\":\"{}\"}}", page_b));
    let told = ws_expect(&mut a, "have this open", 8);
    assert!(
        told.contains("2 of you have this open"),
        "the first window learns of the second: {}",
        told
    );
    drop(b);
    drop(a);
    stop.store(true, Ordering::Relaxed);
    join.join().unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}

/// Every POST route in the whole corpus, fed a body the caller chose and the
/// program did not expect. None of them may answer with the runtime taking
/// the blame.
///
/// This is a CLASS test, and it exists because fixing one instance was not
/// enough twice running. `fields` (ADR-0026) landed with slate guarded and
/// left eleven other examples indexing `req.data` straight — including four
/// handling passwords — so a JSON array posted at them came back as
/// `500 internal: `.password` on a list`. The same shape as the favicon: the
/// capability closed in the language, still open in the corpus.
///
/// Enumerated from source rather than listed here, so a new example is
/// covered the day it is written and no list can go stale.
#[test]
fn t_examples_no_route_blames_the_runtime_for_the_callers_body() {
    // covers: A4, D3, G4
    let mut checked = 0;
    let mut failures: Vec<String> = Vec::new();
    let mut unreachable: Vec<String> = Vec::new();

    for example in example_names() {
        let routes = routes_of(&example);
        if routes.is_empty() {
            continue;
        }
        let dir = staged(&example);
        // A route behind an unbuilt shim answers 500 for every body and the
        // guard is never exercised. Build it, so ledger is swept like the
        // rest; if the toolchain cannot, the route is reported unreachable
        // rather than counted.
        if example == "ledger" {
            build_foreign_shim(&dir, "ledger.store", "ledger_store", "sqlite3");
        }
        let (port, stop, join) = start(dir.clone());

        // Routes behind `allow` answer before the handler runs. A session is
        // the difference between knowing the sweep is blind there and not
        // being blind: sign up if the example offers it, and sweep with it.
        let session = sign_up_anywhere(port);

        for route in &routes {
            // A captured segment takes any literal; give it one.
            let path = route
                .split('/')
                .map(|seg| {
                    if seg.starts_with('{') && seg.ends_with('}') { "probe" } else { seg }
                })
                .collect::<Vec<_>>()
                .join("/");

            // A well-formed object is the control. A route that fails the
            // same way for every body is broken for a reason that is not the
            // body — an unbuilt foreign shim, say — and that 500 belongs to
            // the deployment, not the caller. What this test forbids is a
            // route answering WORSE because of the body's shape.
            let (control_status, _, _) =
                req(port, "POST", &path, Some("{}"), session.as_deref());

            // A route the sweep cannot actually reach is NOT covered, and
            // silently counting it as covered is how this test came to pass
            // over two unguarded routes. `allow` answers 403 before the
            // handler runs, and a missing foreign shim answers 500 for every
            // body — either way the guard is never exercised, so say so.
            if control_status >= 500 {
                unreachable.push(format!(
                    "{} POST {} — every body answers {} (deployment, not the caller); guard unexercised",
                    example, path, control_status
                ));
                continue;
            }
            // An empty body is the commonest malformed request there is.
            for body in ["[1,2,3]", "42", "\"hello\"", "not json at all", "null", ""] {
                let (status, _, text) =
                    req(port, "POST", &path, Some(body), session.as_deref());
                checked += 1;
                if status >= 500 || text.contains("internal:") {
                    failures.push(format!(
                        "{} POST {} with `{}` -> {} {}\n    (a well-formed `{{}}` body gets {})",
                        example, path, body, status, text.trim(), control_status
                    ));
                }
            }
        }

        stop.store(true, Ordering::Relaxed);
        join.join().unwrap();
    }

    assert!(checked > 50, "the sweep found almost nothing to check ({})", checked);

    // Coverage this sweep does not have, named rather than counted. Silence
    // here would be the lie: the first version of this test passed while two
    // of the routes it existed to protect were invisible to it.
    if !unreachable.is_empty() {
        println!(
            "t_examples sweep — {} route(s) it cannot reach:\n  {}",
            unreachable.len(),
            unreachable.join("\n  ")
        );
    }
    assert!(
        failures.is_empty(),
        "{} of {} hostile bodies were answered by blaming the runtime.\n\
         A body the caller chose is the caller's fault: guard with \
         `fields(req.data) ?? fail(400, ...)`.\n\n{}",
        failures.len(),
        checked,
        failures.join("\n")
    );
}

/// Sign up on whichever route this example offers, and return the session
/// cookie. Without one, every `allow`-guarded route answers before its
/// handler and the guard behind it is never tested — which is how the first
/// version of this sweep passed over an unguarded `locker /api/keep`.
fn sign_up_anywhere(port: u16) -> Option<String> {
    let body = "{\"email\":\"sweep@probe.x\",\"password\":\"pw-for-the-sweep\",\"name\":\"sweep\"}";
    for path in ["/api/signup", "/signup", "/api/join", "/join"] {
        let (status, head, _) = req(port, "POST", path, Some(body), None);
        if status >= 400 {
            continue;
        }
        if let Some(c) = head
            .lines()
            .find(|l| l.to_ascii_lowercase().starts_with("set-cookie:"))
            .and_then(|l| l.split("ashsession=").nth(1))
            .map(|v| v.split(';').next().unwrap_or(v).trim().to_string())
        {
            if !c.is_empty() {
                return Some(c);
            }
        }
    }
    None
}

/// Every directory under `examples/` that holds a project.
fn example_names() -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(examples_root())
        .unwrap()
        .flatten()
        .filter(|e| e.path().is_dir())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|n| !n.starts_with('.'))
        .collect();
    names.sort();
    names
}

/// The `route = "..."` literals an example declares. Parsed from source
/// because a hand-kept list is a list that goes stale.
fn routes_of(example: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![examples_root().join(example)];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().and_then(|x| x.to_str()) == Some("ash") {
                let src = std::fs::read_to_string(&p).unwrap_or_default();
                for line in src.lines() {
                    let t = line.trim();
                    if let Some(rest) = t.strip_prefix("route = \"") {
                        if let Some(end) = rest.find('"') {
                            out.push(rest[..end].to_string());
                        }
                    }
                }
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

#[test]
fn t_examples_counter_clicks() {
    let dir = staged("counter");
    let (port, stop, join) = start(dir.clone());
    let (_, _, html) = req(port, "GET", "/", None, None);
    assert!(html.contains("this window: 0"), "{}", html);
    assert!(html.contains("everyone: 0"), "{}", html);
    // The smallest example shows the smallest form of §9.8: one file, one
    // absolute path. A browser asks for it whether or not anyone declared it.
    let (istatus, _, ibody) = req_bytes(port, "/favicon.ico");
    assert_eq!(istatus, 200, "the page every browser loads asks for this");
    assert_eq!(&ibody[..4], b"\x00\x00\x01\x00", "a real ICO");
    let (inst, h) = event_target(&html, "onclick", 0).unwrap();
    let mut ws = ws_open(port);
    ws_send(
        &mut ws,
        &format!("{{\"event\":{{\"instance\":\"{}\",\"h\":\"{}\",\"name\":\"onclick\"}}}}", inst, h),
    );
    let reply = ws_read(&mut ws);
    assert!(reply.contains("this window: 1"), "{}", reply);
    assert!(
        !reply.contains("everyone"),
        "only the view that read the changed value re-renders — the shared \
         button read nothing that moved, so it is not even in the patch: {}",
        reply
    );

    // The other button is the same keyword on a part nothing instantiates,
    // so its `state` is one value for the program — and a second window is
    // told about this one's click without asking (§9.3).
    let (_, _, html_b) = req(port, "GET", "/", None, None);
    let page_b = attr_of(&html_b, "data-ash-page").unwrap();
    let mut b = ws_open(port);
    ws_send(&mut b, &format!("{{\"page\":\"{}\"}}", page_b));
    std::thread::sleep(std::time::Duration::from_millis(80));
    let (inst, h) = event_target(&html, "onclick", 1).unwrap();
    ws_send(
        &mut ws,
        &format!("{{\"event\":{{\"instance\":\"{}\",\"h\":\"{}\",\"name\":\"onclick\"}}}}", inst, h),
    );
    let shared = ws_expect(&mut b, "everyone: 1", 8);
    assert!(
        shared.contains("everyone: 1") && !shared.contains("this window"),
        "the shared count crosses windows, and the per-instance one is not \
         touched by anybody else's click: {}",
        shared
    );
    drop(b);
    stop.store(true, Ordering::Relaxed);
    join.join().unwrap();
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

    // The DIRECTORY form of `files` (§9.8), which had no corpus site at all
    // until an adversarial read pointed out that the reference's own first
    // example was undefended. The single-file form is proved in slate.
    let (dstatus, _, doc) = req(port, "GET", "/docs/notes.md", None, None);
    assert_eq!(dstatus, 200, "a directory mounts under its route: {}", doc);
    assert!(doc.contains("directory form"), "{}", doc);
    let (dbare, _, _) = req(port, "GET", "/docs", None, None);
    assert_eq!(dbare, 404, "the directory itself is not a file");
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
    assert!(html.contains(">finished<"), "both lanes are on the board: {}", html);
    let page_id = attr_of(&html, "data-ash-page").unwrap();
    let mut ws = ws_open(port);
    ws_send(&mut ws, &format!("{{\"page\":\"{}\"}}", page_id));
    std::thread::sleep(std::time::Duration::from_millis(80));

    let (status, _, accepted) =
        req(port, "POST", "/api/jobs", Some("{\"brief\":\"cut release\"}"), None);
    assert_eq!(status, 200);
    assert!(accepted.contains("cut release"), "{}", accepted);

    let pushed = ws_expect(&mut ws, "cut release", 6);
    assert!(pushed.contains("waiting: 0"), "the worker drained it: {}", pushed);
    assert!(
        pushed.contains("jobname\">cut release<"),
        "and the finished brief is on the board nobody asked to refresh: {}",
        pushed
    );
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
            eprintln!(
                "SKIP: cannot build foreign shim `{}` (needs a Rust toolchain + lib{}):\n{}",
                space, link, why
            );
            // Linking needs the DEVELOPMENT package, not the runtime library
            // most systems already ship — `cannot find -lsqlite3` is what that
            // looks like, and a skip note that does not name the fix is a
            // shrug. Same correction the showcase launchers print.
            if why.contains(&format!("-l{}", link)) {
                eprintln!(
                    "      install lib{}'s development package and re-run:\n\
                     \x20       Debian/Ubuntu   sudo apt install lib{}-dev\n\
                     \x20       Fedora/RHEL     sudo dnf install {}-devel\n\
                     \x20       Arch            sudo pacman -S {}",
                    link, link, link, link
                );
            }
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

    // Reactive SQL (ADR-0014): a board holding only a socket is patched live
    // by ANOTHER client's write — no request of its own. `record` `writes
    // Entry` and the board `reads Entry`, so the foreign store joins the §9.3
    // dependency graph exactly like `stored`. (Run after the restart checks so
    // the persisted total above is still ada + bob.)
    let page_id = attr_of(&page2, "data-ash-page").unwrap();
    let mut ws = ws_open(port2);
    ws_send(&mut ws, &format!("{{\"page\":\"{}\"}}", page_id));
    std::thread::sleep(std::time::Duration::from_millis(80));
    let (s3, _, _) =
        req(port2, "POST", "/add", Some("{\"who\":\"cy\",\"note\":\"tea\",\"amount\":2}"), None);
    assert_eq!(s3, 302);
    let patch = ws_expect(&mut ws, "cy: tea ($2)", 10);
    assert!(
        patch.contains("total: $12.5"),
        "the reactive patch re-reads the SQL SUM, not a cached value: {}",
        patch
    );
    drop(ws);

    stop2.store(true, Ordering::Relaxed);
    join2.join().unwrap();
    let _ = std::fs::remove_file(&db);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn t_examples_abacus_computes_through_a_python_worker() {
    // The worker transport (ADR-0017): a capability implemented in Python and
    // reached over JSON Lines — no shared library, no C ABI, no compiler in
    // the project at all. The answer is shape-checked against `Summary` at
    // the boundary like any other foreign return.
    if std::process::Command::new("python3").arg("-V").output().is_err() {
        eprintln!("SKIP: t_examples_abacus needs python3 (the worker's language)");
        return;
    }
    let dir = staged("abacus");
    let (port, stop, join) = start(dir.clone());

    // The page renders figures computed by Python's `statistics` module.
    let (status, _, page) = req(port, "GET", "/", None, None);
    assert_eq!(status, 200);
    assert!(page.contains("mean 5"), "the worker's mean reaches the view: {}", page);
    assert!(page.contains("median 4.5"), "{}", page);
    assert!(page.contains("spread 2"), "{}", page);

    // Same capability over HTTP, same worker (§9.2).
    let (status, _, body) =
        req(port, "POST", "/api/summary", Some("{\"entry\":\"10 20 30\"}"), None);
    assert_eq!(status, 200);
    assert!(body.contains("\"mean\":20"), "{}", body);
    assert!(body.contains("\"spread\":8.165"), "the stdev crosses the boundary: {}", body);

    // Typing re-runs the worker over the socket and patches the figures.
    let (_, _, html) = req(port, "GET", "/", None, None);
    let (inst, typed) = event_target(&html, "oninput", 0).unwrap();
    let mut ws = ws_open(port);
    ws_send(
        &mut ws,
        &format!(
            "{{\"event\":{{\"instance\":\"{}\",\"h\":\"{}\",\"name\":\"oninput\",\"value\":\"1 2 3 4\"}}}}",
            inst, typed
        ),
    );
    let patch = ws_expect(&mut ws, "mean 2.5", 6);
    assert!(patch.contains("median 2.5"), "the worker answers over the socket: {}", patch);
    drop(ws);

    stop.store(true, Ordering::Relaxed);
    join.join().unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn t_examples_enclave_vendors_the_mesh_library_verbatim() {
    // covers: G5
    // There is no registry, so a dependency is code copied into the tree
    // (`ashlar vendor`). A copy that drifts from its source is the version
    // skew a registry exists to manage and this language refuses to have, so
    // the two are compared byte for byte rather than trusted.
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut compared = 0;
    for file in ["mesh.ash"] {
        let lib = std::fs::read(root.join("lib/mesh").join(file))
            .unwrap_or_else(|e| panic!("lib/mesh/{} is missing: {}", file, e));
        let vendored = std::fs::read(root.join("examples/enclave/vendor/mesh").join(file))
            .unwrap_or_else(|e| panic!("examples/enclave/vendor/mesh/{} is missing: {}", file, e));
        assert_eq!(
            lib,
            vendored,
            "examples/enclave/vendor/mesh/{} has drifted from lib/mesh/{} — re-vendor it",
            file,
            file
        );
        compared += 1;
    }
    assert_eq!(compared, 1, "the mesh library is one space, vendored whole");
}

/// The mesh node's control socket, without the daemon behind it.
///
/// This is deliberately NOT a stand-in for the `mesh` space: the space is
/// bound to the shipped worker (`ashlar mesh worker`), and this answers the
/// socket that worker drives. So the adapter under test is the one that ships,
/// its wire framing is exercised against a real socket, and the only thing
/// faked is the network — which is the one part a single machine cannot have.
#[cfg(unix)]
mod fake_node {
    use ashlar::eval::{from_json, to_json, V};
    use std::collections::BTreeMap;
    use std::io::{Read, Write};
    use std::os::unix::net::UnixListener;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};

    #[derive(Default)]
    pub struct State {
        /// What the machine's owner called this node.
        pub name: String,
        /// id, label, status — the roster the daemon would answer.
        pub peers: Vec<(String, String, String)>,
        /// node, label, port — what presence says each peer serves.
        pub sites: Vec<(String, String, u16)>,
        pub exposed: BTreeMap<String, String>,
        pub networks: Vec<String>,
        /// The base name of every file this node was asked to offer.
        pub minted: Vec<String>,
        /// Every token this node was asked to fetch.
        pub fetched: Vec<String>,
        /// Where the last registered download sink writes.
        pub landed: String,
        /// Whether this machine has shared its camera with anyone.
        pub granted: bool,
        /// Every grant id this node was asked to record.
        pub grants: Vec<String>,
        /// How many camera batches this node has handed over.
        pub polls: u32,
        /// `kind|room|text` for everything this node was asked to transmit.
        pub sent: Vec<String>,
        /// Every command this node was asked to run. The rename regression is
        /// asserted from here: a site must join a mesh without ever touching
        /// the identity its owner named.
        pub asked: Vec<String>,
    }

    pub struct Fake {
        pub state: Arc<Mutex<State>>,
        pub socket: std::path::PathBuf,
        /// Connections that asked to be told when something moves. The real
        /// node holds these for its GUI; the worker is one more subscriber.
        watchers: Arc<Mutex<Vec<std::os::unix::net::UnixStream>>>,
        stop: Arc<AtomicBool>,
    }

    impl Fake {
        /// Say the session moved, the way the node does when presence
        /// changes. Nothing polls: this is what the page follows.
        pub fn announce(&self, event: &str) {
            self.emit(event, V::Map(BTreeMap::new()))
        }

        /// One peer's line arriving, exactly as the node hands it to its own
        /// clients: `{from, message}` on `allmystuff://room`.
        pub fn hears(&self, from: &str, room: &str, said: &str) {
            self.emit(
                "allmystuff://room",
                map(&[
                    ("from", text(from)),
                    (
                        "message",
                        map(&[
                            ("room", text(room)),
                            ("kind", text("chat")),
                            ("text", text(said)),
                        ]),
                    ),
                ]),
            )
        }

        /// A member restating what it offers the room.
        pub fn hears_shared(&self, from: &str, room: &str, token: &str, name: &str) {
            self.emit(
                "allmystuff://room",
                map(&[
                    ("from", text(from)),
                    (
                        "message",
                        map(&[
                            ("room", text(room)),
                            ("kind", text("share_list")),
                            (
                                "files",
                                V::List(vec![map(&[
                                    ("token", text(token)),
                                    ("name", text(name)),
                                    ("size", V::Number(3.0)),
                                ])]),
                            ),
                        ]),
                    ),
                ]),
            )
        }

        fn emit(&self, event: &str, payload: V) {
            let body = to_json(&map(&[
                ("kind", text("emit")),
                ("event", text(event)),
                ("payload", payload),
            ]));
            let bytes = body.as_bytes();
            let len = (bytes.len() as u32) + 1;
            let mut held = self.watchers.lock().unwrap();
            held.retain_mut(|w| {
                w.write_all(&len.to_be_bytes())
                    .and_then(|_| w.write_all(&[2u8]))
                    .and_then(|_| w.write_all(bytes))
                    .and_then(|_| w.flush())
                    .is_ok()
            });
        }
    }

    impl Drop for Fake {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::Relaxed);
            let _ = std::fs::remove_file(&self.socket);
        }
    }

    /// The socket lives outside the project tree, and short: a `sun_path` is
    /// 108 bytes on Linux and a temp path under a project directory can spend
    /// them.
    pub fn start() -> Fake {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let socket = std::env::temp_dir().join(format!(
            "ashlar_node_{}_{}.sock",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_file(&socket);
        let listener = UnixListener::bind(&socket).unwrap();
        listener.set_nonblocking(true).unwrap();
        let state = Arc::new(Mutex::new(State::default()));
        let stop = Arc::new(AtomicBool::new(false));
        let watchers: Arc<Mutex<Vec<std::os::unix::net::UnixStream>>> =
            Arc::new(Mutex::new(Vec::new()));
        let (s, t, w) = (state.clone(), stop.clone(), watchers.clone());
        std::thread::spawn(move || {
            while !t.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        stream.set_nonblocking(false).unwrap();
                        let Some(payload) = read_frame(&mut stream) else {
                            continue;
                        };
                        let ack = |stream: &mut std::os::unix::net::UnixStream, body: String| {
                            let bytes = body.as_bytes();
                            let len = (bytes.len() as u32) + 1;
                            let _ = stream
                                .write_all(&len.to_be_bytes())
                                .and_then(|_| stream.write_all(&[0u8]))
                                .and_then(|_| stream.write_all(bytes))
                                .and_then(|_| stream.flush());
                        };
                        // A subscriber keeps its connection: the node streams
                        // events down it until one side goes away.
                        if matches!(
                            ashlar::meshd::at(&from_json(&payload).unwrap_or(V::None), "cmd"),
                            Some(V::Text(ref c)) if c == "__subscribe_events"
                        ) {
                            ack(
                                &mut stream,
                                to_json(&map(&[("ok", V::Bool(true)), ("result", V::None)])),
                            );
                            w.lock().unwrap().push(stream);
                            continue;
                        }
                        // A `*_poll` answers with a raw batch under tag 1,
                        // never JSON — the node keeps media bytes out of its
                        // JSON lane, and a double that answered otherwise
                        // would hide exactly the bug that cost a day here.
                        if matches!(
                            ashlar::meshd::at(&from_json(&payload).unwrap_or(V::None), "cmd"),
                            Some(V::Text(ref c)) if c == "video_poll"
                        ) {
                            // A camera that never stops would flood the
                            // socket for the rest of the test; a real one
                            // stops too, and an empty batch is how it says so.
                            let sent = {
                                let mut st = s.lock().unwrap();
                                st.polls += 1;
                                st.polls
                            };
                            if sent > 2 {
                                let _ = stream
                                    .write_all(&1u32.to_be_bytes())
                                    .and_then(|_| stream.write_all(&[1u8]))
                                    .and_then(|_| stream.flush());
                                continue;
                            }
                            let jpeg: Vec<u8> = vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, 0xFF, 0xD9];
                            let frame = to_json(&map(&[
                                ("t", text("video")),
                                ("seq", V::Number(7.0)),
                                ("jpeg", text(&b64(&jpeg))),
                            ]));
                            let mut batch = (frame.len() as u32).to_le_bytes().to_vec();
                            batch.extend_from_slice(frame.as_bytes());
                            let len = (batch.len() as u32) + 1;
                            let _ = stream
                                .write_all(&len.to_be_bytes())
                                .and_then(|_| stream.write_all(&[1u8]))
                                .and_then(|_| stream.write_all(&batch))
                                .and_then(|_| stream.flush());
                            continue;
                        }
                        let answered = handle(&s, &payload);
                        // A fetch's chunks stream to disk and never reach a
                        // poll queue; the node says it landed with an event.
                        let finished = matches!(
                            ashlar::meshd::at(&from_json(&payload).unwrap_or(V::None), "cmd"),
                            Some(V::Text(ref c)) if c == "file_send"
                        );
                        ack(&mut stream, to_json(&answered));
                        if finished {
                            let landed = s.lock().unwrap().landed.clone();
                            let body = to_json(&map(&[
                                ("kind", text("emit")),
                                ("event", text("allmystuff://file-saved")),
                                (
                                    "payload",
                                    map(&[
                                        ("route", text("route-1")),
                                        ("req", V::Number(1.0)),
                                        ("path", text(&landed)),
                                    ]),
                                ),
                            ]));
                            let bytes = body.as_bytes();
                            let len = (bytes.len() as u32) + 1;
                            let mut held = w.lock().unwrap();
                            held.retain_mut(|c| {
                                c.write_all(&len.to_be_bytes())
                                    .and_then(|_| c.write_all(&[2u8]))
                                    .and_then(|_| c.write_all(bytes))
                                    .and_then(|_| c.flush())
                                    .is_ok()
                            });
                        }
                    }
                    Err(_) => std::thread::sleep(std::time::Duration::from_millis(5)),
                }
            }
        });
        Fake {
            state,
            socket,
            watchers,
            stop,
        }
    }

    fn read_frame(stream: &mut std::os::unix::net::UnixStream) -> Option<String> {
        let mut head = [0u8; 4];
        stream.read_exact(&mut head).ok()?;
        let len = u32::from_be_bytes(head) as usize;
        let mut body = vec![0u8; len];
        stream.read_exact(&mut body).ok()?;
        String::from_utf8(body[1..].to_vec()).ok()
    }

    fn map(pairs: &[(&str, V)]) -> V {
        V::Map(
            pairs
                .iter()
                .map(|(k, v)| (k.to_string(), v.clone()))
                .collect(),
        )
    }

    fn text(s: &str) -> V {
        V::Text(s.to_string())
    }

    /// Base64, the encoding the node's JSON control channel uses for bytes.
    fn b64(bytes: &[u8]) -> String {
        const A: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut out = String::new();
        for group in bytes.chunks(3) {
            let b = [group[0], *group.get(1).unwrap_or(&0), *group.get(2).unwrap_or(&0)];
            let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
            for i in 0..4 {
                if i <= group.len() {
                    out.push(A[((n >> (18 - 6 * i)) & 63) as usize] as char);
                } else {
                    out.push('=');
                }
            }
        }
        out
    }

    fn handle(state: &Mutex<State>, payload: &str) -> V {
        let call = from_json(payload).unwrap_or(V::None);
        let cmd = match ashlar::meshd::at(&call, "cmd") {
            Some(V::Text(c)) => c,
            _ => return map(&[("ok", V::Bool(false)), ("error", text("no cmd"))]),
        };
        let args = ashlar::meshd::at(&call, "args").unwrap_or(V::None);
        let mut st = state.lock().unwrap();
        st.asked.push(cmd.clone());
        let result = match cmd.as_str() {
            "mesh_identity" => map(&[("device_id", text("me-A1B2C")), ("label", text(&st.name))]),
            "mesh_peers" => V::List(
                st.peers
                    .iter()
                    .map(|(id, label, status)| {
                        map(&[
                            ("device_id", text(id)),
                            ("label", text(label)),
                            ("status", text(status)),
                        ])
                    })
                    .collect(),
            ),
            "mesh_networks" => V::List(
                st.networks
                    .iter()
                    .map(|n| map(&[("network_id", text(n))]))
                    .collect(),
            ),
            "mesh_network_add" => {
                if let Some(V::Text(id)) = ashlar::meshd::at(&args, "config")
                    .and_then(|c| ashlar::meshd::at(&c, "network_id"))
                {
                    st.networks.push(id);
                }
                V::None
            }
            "site_exposed" => V::Map(
                st.exposed
                    .iter()
                    .map(|(k, v)| (k.clone(), text(v)))
                    .collect(),
            ),
            "site_set_exposed" => {
                st.exposed.clear();
                if let Some(V::Map(m)) = ashlar::meshd::at(&args, "exposed") {
                    for (k, v) in m {
                        if let V::Text(label) = v {
                            st.exposed.insert(k, label);
                        }
                    }
                }
                V::None
            }
            // What a member sends to the room. The node routes it; every
            // recipient's own node emits it back out as an event.
            "room_send" => {
                st.sent.push(format!(
                    "{}|{}|{}",
                    ashlar::meshd::at(&args, "message")
                        .and_then(|m| ashlar::meshd::at(&m, "kind"))
                        .and_then(|k| match k {
                            V::Text(t) => Some(t),
                            _ => None,
                        })
                        .unwrap_or_default(),
                    ashlar::meshd::at(&args, "message")
                        .map(|m| ashlar::meshd::at(&m, "room"))
                        .and_then(|r| match r {
                            Some(V::Text(t)) => Some(t),
                            _ => None,
                        })
                        .unwrap_or_default(),
                    ashlar::meshd::at(&args, "message")
                        .map(|m| ashlar::meshd::at(&m, "text"))
                        .and_then(|t| match t {
                            Some(V::Text(t)) => Some(t),
                            _ => None,
                        })
                        .unwrap_or_default(),
                ));
                V::None
            }
            // The room's own file lane: a token whose allow-list is the
            // members, and one request — fetch it — checked per call. No
            // share, no grant, nothing durable.
            "room_share_files" => {
                let paths = match ashlar::meshd::at(&args, "paths") {
                    Some(V::List(p)) => p,
                    _ => vec![],
                };
                V::List(
                    paths
                        .iter()
                        .enumerate()
                        .map(|(i, p)| {
                            let named = ashlar::meshd::as_text(p);
                            let base =
                                named.rsplit('/').next().unwrap_or(&named).to_string();
                            st.minted.push(base.clone());
                            map(&[
                                ("token", text(&format!("tok{}", i + 1))),
                                ("name", text(&base)),
                                ("size", V::Number(3.0)),
                            ])
                        })
                        .collect(),
                )
            }
            "connect_route" => {
                // A media route from somebody who has not shared is refused,
                // and the node's sentence is the one the page shows.
                if matches!(ashlar::meshd::at(&args, "media"), Some(V::Text(ref m)) if m == "video")
                    && !st.granted
                {
                    return map(&[
                        ("ok", V::Bool(false)),
                        (
                            "error",
                            text(
                                "not authorized: capturing this device's screen, camera, \
                                 or microphone needs owner/fleet or a share",
                            ),
                        ),
                    ]);
                }
                text("route-1")
            }
            "share_grant" => {
                st.granted = true;
                st.grants.push(
                    ashlar::meshd::at(&args, "grant")
                        .map(|g| match ashlar::meshd::at(&g, "id") {
                            Some(V::Text(id)) => id,
                            _ => String::new(),
                        })
                        .unwrap_or_default(),
                );
                V::None
            }
            "video_watch" => V::Number(1.0),
            "disconnect_route" => V::None,
            "file_download" => {
                // The node writes a fetch into Downloads and answers where it
                // landed. This test's "Downloads" is a temp file.
                let landing = std::env::temp_dir()
                    .join(format!("ashlar_fetched_{}.txt", std::process::id()));
                std::fs::write(&landing, b"hi").unwrap();
                st.landed = landing.to_string_lossy().into_owned();
                text(&st.landed)
            }
            "file_send" => {
                st.fetched.push(
                    ashlar::meshd::at(&args, "event")
                        .and_then(|e| ashlar::meshd::at(&e, "token"))
                        .and_then(|t| match t {
                            V::Text(t) => Some(t),
                            _ => None,
                        })
                        .unwrap_or_default(),
                );
                V::None
            }
            "site_mappings" => V::List(vec![]),
            // The node binds a local port and proxies it over the mesh; the
            // number is the node's to choose, which is why source never has it.
            "site_map" => map(&[("localPort", V::Number(47001.0))]),
            "session_snapshot" => map(&[(
                "peers",
                V::List(
                    st.sites
                        .iter()
                        .map(|(node, label, port)| {
                            map(&[
                                ("node", text(node)),
                                ("label", text(node)),
                                (
                                    "sites",
                                    V::List(vec![map(&[
                                        ("label", text(label)),
                                        ("port", V::Number(*port as f64)),
                                    ])]),
                                ),
                            ])
                        })
                        .collect(),
                ),
            )]),
            _ => {
                return map(&[
                    ("ok", V::Bool(false)),
                    ("error", text(&format!("no such cmd: {}", cmd))),
                ])
            }
        };
        let wrapped = match cmd.as_str() {
            "mesh_peers" => map(&[("peers", result)]),
            "mesh_networks" => map(&[("networks", result)]),
            _ => result,
        };
        map(&[("ok", V::Bool(true)), ("result", wrapped)])
    }
}

#[test]
fn t_examples_enclave_serves_where_there_is_no_mesh() {
    // covers: B5, G4
    // A machine with no mesh node is an ordinary machine — the app is closed,
    // it was never installed, or (WSL) the node is on the other side of the
    // kernel boundary. The site must still serve, and must say which state it
    // is in rather than showing an empty roster that looks like a lonely one.
    //
    // This is the regression for a build that faulted on every mesh read: the
    // enclave's `start` stack called `arrive()`, the fault propagated, and the
    // whole example refused to come up — "1 of 18 did not start".
    let dir = staged("enclave");
    // A socket that is not there, named — so nothing is spawned and the
    // outcome does not depend on what the machine running the suite installed.
    bind_mesh_to(&dir, &dir.join("no-node-here.sock"));
    let (port, stop, join) = start(dir.clone());

    let (status, _, html) = req(port, "GET", "/", None, None);
    assert_eq!(status, 200, "the site serves without a mesh: {}", html);
    assert!(
        html.contains("no mesh here: nothing is listening"),
        "the room says why it is empty, not just that it is: {}",
        html
    );
    assert!(
        html.contains("nothing is listening at"),
        "and the panel carries the correction, in the page: {}",
        html
    );
    // The one deliberate publish still fails loudly: `run --mesh` printed a
    // promise, and a site nobody can reach reported as published would be the
    // quiet-wrong this language refuses.
    let mut link = ashlar::mesh::Link::new();
    let refused = link
        .publish(&dir, port, "enclave", "enclave.app")
        .unwrap_err();
    assert!(refused.contains("nothing is listening"), "{}", refused);
    let report = link.report(&dir);
    assert!(!report.ok(), "`ashlar mesh` is the question, so it is told");
    assert_eq!(
        report.problems.len(),
        1,
        "one absent node, one problem: {:?}",
        report.problems
    );

    stop.store(true, Ordering::Relaxed);
    join.join().unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}

#[cfg(unix)]
#[test]
fn t_examples_enclave_shows_who_else_is_on_the_mesh() {
    // covers: B5, G4
    // The mesh is a capability, not a builtin: one `foreign` space reached
    // across the one boundary (§9.10), bound to the worker this toolchain
    // ships. What is faked here is the mesh NODE's socket, not the space — so
    // the adapter, its wire, and the vendored library are all the shipped ones
    // and only the network is stood in for.
    let dir = staged("enclave");
    let node = fake_node::start();
    node.state.lock().unwrap().name = "chris's laptop".to_string();
    bind_mesh_to(&dir, &node.socket);
    let (port, stop, join) = start(dir.clone());

    // An empty mesh says so. A roster that renders nothing when it knows
    // nothing is indistinguishable from one that is broken.
    let (status, _, html) = req(port, "GET", "/", None, None);
    assert_eq!(status, 200);
    assert!(
        html.contains("nobody else here yet"),
        "the empty room says so: {}",
        html
    );
    assert!(
        html.contains("Nobody has said anything."),
        "rather than rendering blank: {}",
        html
    );

    // The mesh this app is on is the one its OWN source layered onto the
    // vendored setting — not the shared default the library ships with.
    assert!(
        html.contains(">enclave<"),
        "the panel names the app's own mesh, from the layered setting: {}",
        html
    );

    // The machine's name is the machine's. The panel shows what the NODE
    // says it is called, and joining the mesh did not change it — an earlier
    // build set the identity label from the app's `label` setting, renaming
    // its owner's node on every mesh that node was on.
    assert!(
        html.contains("title=\"chris&#x27;s laptop\"") || html.contains("title=\"chris's laptop\""),
        "you are in your own room, under the name your machine's owner gave it: {}",
        html
    );
    {
        let st = node.state.lock().unwrap();
        assert!(
            !st.asked.iter().any(|c| c == "mesh_identity_set_label"),
            "arriving on a mesh must never rename the node: {:?}",
            st.asked
        );
        assert!(
            st.networks.iter().any(|n| n == "enclave"),
            "it joins the mesh the program named, and that is all it writes: {:?}",
            st.networks
        );
    }

    let (status, _, peers) = req(port, "GET", "/api/peers", None, None);
    assert_eq!(status, 200);
    assert_eq!(peers.trim(), "[]", "no peers, over HTTP too");

    // Someone arrives.
    let page_id = attr_of(&html, "data-ash-page").unwrap();
    let mut ws = ws_open(port);
    ws_send(&mut ws, &format!("{{\"page\":\"{}\"}}", page_id));
    std::thread::sleep(std::time::Duration::from_millis(80));
    {
        let mut st = node.state.lock().unwrap();
        st.peers = vec![
            ("n1".to_string(), "ada".to_string(), "active".to_string()),
            ("n2".to_string(), "grace".to_string(), "offline".to_string()),
        ];
        // Presence carries the display form of an id (`pubkey-SUFFIX`) while
        // the roster answers the bare key; a site whose peer is on this mesh
        // must survive that difference.
        st.sites = vec![("n1-7F3A2".to_string(), "ada's pad".to_string(), 8080)];
    }
    // …and the node says so, down the connection it holds open. Nothing in
    // this program polls: the worker is subscribed, the push names the
    // collection, and every view that read the roster is dirtied by it.
    node.announce("allmystuff://session");

    // Nobody polled — not the browser, not the program. The node pushed, the
    // worker named the collection, and every view that read it re-rendered
    // and patched over the socket (§9.3, §9.10). The library used to carry a
    // three-second schedule for this, which was both late and, on a quiet
    // mesh, entirely wasted work.
    let patch = ws_expect(&mut ws, "ada", 8);
    assert!(patch.contains("grace"), "the whole roster patches, not one row: {}", patch);
    assert!(
        patch.contains("who-face who-here"),
        "presence is visible: a connected peer carries the live class: {}",
        patch
    );
    // Somebody says something. The mesh IS the room — everyone holding its id
    // is in it — so the id every member computes is derived from the mesh's
    // name, with no host to mint one or be offline.
    let room = ashlar::meshd::room_of("enclave");
    node.hears("n1", &room, "anyone about?");
    let patch = ws_expect(&mut ws, "anyone about?", 8);
    assert!(
        patch.contains("class=\"said\""),
        "a line arriving patches the conversation, unasked: {}",
        patch
    );
    // A line for another room on the same mesh is somebody else's traffic.
    node.hears("n1", "ashlar:elsewhere", "not for this app");
    std::thread::sleep(std::time::Duration::from_millis(200));

    // And this side can answer. Type into the box, submit it, and what the
    // node was asked to transmit is what the person typed — to the room the
    // mesh's name derives, not to a room somebody had to be given.
    let (_, _, page) = req(port, "GET", "/", None, None);
    // The room's first `oninput` is the conversation's own filter, which
    // sends nothing anywhere; the second is the line you talk on.
    let (inst, typed) = event_target(&page, "oninput", 1).unwrap();
    ws_send(
        &mut ws,
        &format!(
            "{{\"event\":{{\"instance\":\"{}\",\"h\":\"{}\",\"name\":\"oninput\",\"value\":\"here\"}}}}",
            inst, typed
        ),
    );
    let after = ws_expect(&mut ws, "here", 8);
    let (inst, submit) = event_target(&after, "onsubmit", 0).unwrap();
    ws_send(
        &mut ws,
        &format!(
            "{{\"event\":{{\"instance\":\"{}\",\"h\":\"{}\",\"name\":\"onsubmit\"}}}}",
            inst, submit
        ),
    );
    let mine = ws_expect(&mut ws, "line mine", 8);
    assert!(
        mine.contains("line mine"),
        "your own words read as yours, not as everyone else's: {}",
        mine
    );
    assert!(
        mine.contains("when\">now<"),
        "and carry when they were said: {}",
        mine
    );
    // The room noticed the arrivals itself. Nobody sent those: every member
    // computes them from the roster it already watches, so they cost no
    // traffic and no two members can disagree about who was there.
    assert!(
        mine.contains("<p class=\"notice\">ada joined</p>"),
        "an arrival is a notice, not a message: {}",
        mine
    );
    assert!(
        !mine.contains("grace joined"),
        "and only for somebody who is actually here — grace is offline: {}",
        mine
    );
    // A member puts a file up. The room's file lane is NOT the share
    // subsystem: the token's allow-list is the members, so nothing durable is
    // granted and nothing has to be revoked.
    node.hears_shared("n1", &room, "tok9", "notes.txt");
    let shelf = ws_expect(&mut ws, "notes.txt", 8);
    assert!(
        shelf.contains("drop-name"),
        "an offer arrives as a push and patches the shelf: {}",
        shelf
    );
    // Nothing is fetched until somebody asks — an offer is a list, not a
    // transfer. Ask, and the bytes land under the site's own assets, where a
    // `files` part serves them like anything else.
    let (inst, get) = event_target(&shelf, "onclick", 0).unwrap();
    ws_send(
        &mut ws,
        &format!(
            "{{\"event\":{{\"instance\":\"{}\",\"h\":\"{}\",\"name\":\"onclick\"}}}}",
            inst, get
        ),
    );
    let after = ws_expect(&mut ws, "/room/notes.txt", 8);
    assert!(
        after.contains("<a href=\"/room/notes.txt\""),
        "a fetched file is an ordinary link on this site: {}",
        after
    );
    {
        let st = node.state.lock().unwrap();
        assert_eq!(st.fetched, vec!["tok9".to_string()], "one fetch, by token");
    }
    let (status, _, body) = req(port, "GET", "/room/notes.txt", None, None);
    assert_eq!((status, body.trim()), (200, "hi"), "and it serves");

    drop(ws);
    {
        let st = node.state.lock().unwrap();
        assert_eq!(
            st.sent,
            vec![format!("chat|{}|here", room)],
            "one line, to the room the mesh's own name derives"
        );
    }

    let (_, _, page) = req(port, "GET", "/", None, None);
    assert!(
        page.contains("anyone about?"),
        "what was heard is on the page: {}",
        page
    );
    assert!(
        !page.contains("not for this app"),
        "another room's traffic is not this program's to show: {}",
        page
    );

    // The same roster over HTTP: one handler, two transports (G2).
    let (_, _, peers) = req(port, "GET", "/api/peers", None, None);
    assert!(peers.contains("\"label\":\"ada\""), "{}", peers);
    assert!(peers.contains("\"here\":false"), "presence crosses as it is: {}", peers);

    // What `ashlar run --mesh` does, against the same binding: publish the
    // port this origin is serving, ask what the mesh now says, and take it
    // back off. The site is published to a mesh named at RUN time — the
    // program said nothing about it, which is the whole of B5 here.
    let mut link = ashlar::mesh::Link::new();
    let landed = link
        .publish(&dir, port, "enclave", "enclave.app")
        .expect("the mesh answers `expose`");
    assert_eq!(landed.network, "enclave");
    assert!(
        landed.line().contains("mesh `enclave`"),
        "the line a runner reads names the mesh: {}",
        landed.line()
    );
    let report = link.report(&dir);
    assert!(report.ok(), "the mesh answered every question: {:?}", report.problems);
    assert!(
        report.facts.iter().any(|(k, v)| k == "published" && v == "enclave.app"),
        "the published site is reported back: {:?}",
        report.facts
    );
    link.withdraw(&dir, port).expect("the mesh answers `unexpose`");
    let after = link.report(&dir);
    assert!(
        after.facts.iter().any(|(k, v)| k == "published" && v == "nothing"),
        "and taking it back off is visible to the same link: {:?}",
        after.facts
    );

    stop.store(true, Ordering::Relaxed);
    join.join().unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn t_examples_locker_scopes_storage_per_user() {
    // `peruser stored` gives each signed-in user their own isolated, persisted
    // data (ADR-0015). Proven here: anonymous access is refused, two users
    // never see each other's notes, and the data survives a restart keyed by
    // the persisted account id.
    let dir = staged("locker");
    let (port, stop, join) = start(dir.clone());

    // Anonymous cannot reach peruser storage — the `allow` guard rejects it
    // before the read would even fault.
    let (anon, _, _) = req(port, "GET", "/api/notes", None, None);
    assert_eq!(anon, 403, "anonymous is refused the peruser read");

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

    // Each sees ONLY their own — the peruser isolation, by construction.
    let (_, _, an) = req(port, "GET", "/api/notes", None, Some(&ada));
    assert!(an.contains("ada-secret") && !an.contains("bob-secret"),
        "ada sees only her own notes: {}", an);
    let (_, _, bn) = req(port, "GET", "/api/notes", None, Some(&bob));
    assert!(bn.contains("bob-secret") && !bn.contains("ada-secret"),
        "bob sees only his own notes: {}", bn);

    // The `/` view: a gate for anonymous, the live board for a member —
    // whose peruser notes render right in the page, isolated (§9.3).
    let (anon_home, _, gate) = req(port, "GET", "/", None, None);
    assert_eq!(anon_home, 200);
    assert!(gate.contains("class=\"stack\""), "anonymous sees the gate: {}", gate);
    let (_, _, board) = req(port, "GET", "/", None, Some(&ada));
    assert!(board.contains("ada-secret") && !board.contains("bob-secret"),
        "the board renders only this user's peruser notes: {}", board);

    // peruser stored survives a restart. Sessions do not persist, so log in
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
        "restart kept ada's peruser notes, still isolated: {}", a2);

    stop2.store(true, Ordering::Relaxed);
    join2.join().unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn t_examples_slate_merges_two_people_typing_at_once() {
    // A shared pad, and the one problem that makes a pad real software:
    // two people typing at the same time. The client sends the whole
    // field, so the naive server takes the last snapshot it received and
    // the other person's work disappears with no error anywhere. This
    // test is the proof that it does not.
    let dir = staged("slate");
    let (port, stop, join) = start(dir.clone());

    // Two browsers open the same pad. No login, no invite: the URL is it.
    let (status, _, page_a) = req(port, "GET", "/p/welcome", None, None);
    assert_eq!(status, 200);
    // The absolute path a program does not choose (§9.8). `files` naming one
    // file is the only way to answer it without taking `/` from the page.
    let (rstatus, rheaders, rbody) = req(port, "GET", "/robots.txt", None, None);
    assert_eq!(rstatus, 200, "a deployed program must be able to answer /robots.txt");
    assert!(rbody.contains("User-agent"), "{}", rbody);
    assert!(
        rheaders.to_lowercase().contains("content-type: text/plain"),
        "served as text, not octet-stream: {}",
        rheaders
    );
    let (rbelow, _, _) = req(port, "GET", "/robots.txt/x", None, None);
    assert_eq!(rbelow, 404, "one file answers one path and nothing below it");

    // The path every browser asks for on every page load. T-BROWSER's first
    // run failed here: the capability had landed and no example served one,
    // so a console error survived the finding that was meant to close it.
    let (istatus, iheaders, ibody) = req_bytes(port, "/favicon.ico");
    assert_eq!(istatus, 200, "a browser asks for this unprompted, every load");
    assert!(
        iheaders.to_lowercase().contains("content-type: image/x-icon"),
        "served as an icon: {}",
        iheaders
    );
    assert_eq!(&ibody[..4], b"\x00\x00\x01\x00", "and it is a real ICO, not a placeholder");

    // The pad names its own tab (§9.4). Found by driving this example with a
    // real browser: every page it served had a blank title, and nothing in
    // the reference said a view could set one — though a view always could.
    assert!(
        page_a.contains("<title>welcome to slate · slate</title>"),
        "the pad must name its tab after itself: {}",
        page_a
    );
    let (_, _, page_b) = req(port, "GET", "/p/welcome", None, None);
    let id_a = attr_of(&page_a, "data-ash-page").unwrap();
    let id_b = attr_of(&page_b, "data-ash-page").unwrap();
    let mut a = ws_open(port);
    let mut b = ws_open(port);
    ws_send(&mut a, &format!("{{\"page\":\"{}\"}}", id_a));
    ws_send(&mut b, &format!("{{\"page\":\"{}\"}}", id_b));
    std::thread::sleep(std::time::Duration::from_millis(100));

    // Presence: each page named itself off the stone list on arrival, and
    // A's roster carries both.
    let (_, _, seen) = req(port, "GET", "/api/pad/welcome", None, None);
    assert!(seen.contains("granite") || seen.contains("basalt"), "the roster names its visitors: {}", seen);
    assert!(seen.contains("\"here\":[\""), "{}", seen);

    let (inst_a, key_a) = event_target(&page_a, "oninput", 0).unwrap();
    let (inst_b, key_b) = event_target(&page_b, "oninput", 0).unwrap();
    let typing = |s: &mut TcpStream, inst: &str, h: &str, value: &str| {
        ws_send(
            s,
            &format!(
                "{{\"event\":{{\"instance\":\"{}\",\"h\":\"{}\",\"name\":\"oninput\",\"value\":\"{}\"}}}}",
                inst, h, value
            ),
        );
    };

    // A writes the first three lines. B's page is patched with them —
    // live co-editing, with no editor library and no client code: the
    // whole browser side is the runtime's shim (§9.4).
    typing(&mut a, &inst_a, &key_a, "alpha\\nbeta\\ngamma");
    ws_expect(&mut b, "gamma", 20);

    // A changes the first line. B is patched again...
    typing(&mut a, &inst_a, &key_a, "ALPHA\\nbeta\\ngamma");
    ws_expect(&mut b, "ALPHA", 20);

    // ...and now B's keystroke arrives carrying the text B had BEFORE
    // that patch — a finger that came down while the patch was in
    // flight. Taken at face value it silently undoes A's line, which is
    // the failure a shared editor may never have. The pad recognises a
    // line it held one version ago and keeps A's, while B's own change on
    // the third line still lands.
    typing(&mut b, &inst_b, &key_b, "alpha\\nbeta\\nGAMMA");
    std::thread::sleep(std::time::Duration::from_millis(150));
    let (_, _, crossed) = req(port, "GET", "/api/pad/welcome", None, None);
    assert!(
        crossed.contains("ALPHA") && crossed.contains("GAMMA"),
        "a keystroke that crossed a patch must not revert the other writer: {}",
        crossed
    );
    assert!(
        crossed.contains("\"clashes\":0"),
        "and being a step behind is not a conflict — crying wolf at every \
         fast typist is its own failure: {}",
        crossed
    );

    // True concurrency, stated exactly: two clients that each say what
    // they were editing. This is the path a script, an integration, or a
    // second editor takes, and it is where "at the same time" is a fact
    // rather than a race the test has to win.
    let edit = |base: &str, body: &str, who: &str| {
        req(
            port,
            "POST",
            "/api/edit/welcome",
            Some(&format!(
                "{{\"base\":\"{}\",\"body\":\"{}\",\"who\":\"{}\"}}",
                base, body, who
            )),
            None,
        )
    };
    // The pad currently reads ALPHA/beta/GAMMA, and that is the text both
    // of these clients say they were editing.
    let held = "ALPHA\\nbeta\\nGAMMA";
    edit(held, "ALPHA\\nfrom-ada\\nGAMMA", "ada");
    edit(held, "ALPHA\\nbeta\\nGAMMA-2", "bob");
    let (_, _, both) = req(port, "GET", "/api/pad/welcome", None, None);
    assert!(
        both.contains("from-ada") && both.contains("GAMMA-2"),
        "simultaneous edits to different lines both survive: {}",
        both
    );
    assert!(both.contains("\"clashes\":0"), "different lines are not a conflict: {}", both);

    // Same base, SAME line: a real disagreement. One lands, the other is
    // refused and told — never dropped in silence.
    let now_held = "ALPHA\\nfrom-ada\\nGAMMA-2";
    edit(now_held, "ALPHA\\nada-again\\nGAMMA-2", "ada");
    edit(now_held, "ALPHA\\nbob-instead\\nGAMMA-2", "bob");
    let (_, _, after) = req(port, "GET", "/api/pad/welcome", None, None);
    assert!(after.contains("ada-again"), "the first edit stands: {}", after);
    assert!(!after.contains("bob-instead"), "the second did not overwrite it: {}", after);
    assert!(after.contains("\"clashes\":1"), "the conflict is counted: {}", after);
    let told = ws_expect(&mut a, "same line", 30);
    assert!(
        told.contains("the copy already on the pad won"),
        "and every page on the pad hears which way it went: {}",
        told
    );

    // A deliberate rewrite one step later must still land — the rule that
    // protects a lagging writer must not freeze the text.
    edit("ALPHA\\nada-again\\nGAMMA-2", "ALPHA\\nrewritten\\nGAMMA-2", "cy");
    let (_, _, rewritten) = req(port, "GET", "/api/pad/welcome", None, None);
    assert!(rewritten.contains("rewritten"), "an ordinary later edit lands: {}", rewritten);

    // A layered policy refuses an edit the pad will not hold, on the same
    // seam the history layer uses — and the refusal reaches the writer
    // rather than being swallowed.
    let huge = "x".repeat(20_001);
    let (refused, _, why) = req(
        port,
        "POST",
        "/api/edit/welcome",
        Some(&format!("{{\"base\":\"\",\"body\":\"{}\",\"who\":\"a script\"}}", huge)),
        None,
    );
    assert_eq!(refused, 409, "the size policy refuses: {}", why);
    assert!(why.contains("20000 characters"), "and says what the limit is: {}", why);

    // What the outside world sends when it is not a browser. Everything
    // arriving from outside is `data` (§5), and the shortest idiom that
    // type-checks — `text(req.data["body"] ?? "")` — happily accepts a
    // number and writes it to the pad, so the route guards instead.
    let bad: &[(&str, u16)] = &[
        ("not json at all", 400),
        ("{\"base\":\"\"}", 400),
    ];
    for (body, want) in bad {
        let (got, _, _) = req(port, "POST", "/api/edit/welcome", Some(body), None);
        assert_eq!(got, *want, "malformed edit `{}` must be refused, not written", body);
    }
    // The shape that used to have no guard: valid JSON, but not an object.
    // `data` is a union, and nothing answered "is this a map", so the first
    // index faulted and the caller's choice was reported as the server's
    // fault — a 500 whose message began `internal:`. `fields` asks it now
    // (ADR-0026), so the refusal is the route's own 400 and the message is
    // one the route wrote.
    for body in ["[1,2,3]", "42", "\"hello\""] {
        let (status, _, seen) = req(port, "POST", "/api/edit/welcome", Some(body), None);
        assert_eq!(status, 400, "a non-object JSON body is the caller's fault: {}", seen);
        assert!(
            !seen.contains("internal:"),
            "and the runtime does not take the blame for it: {}",
            seen
        );
        assert!(seen.contains("JSON object"), "the refusal says what was wanted: {}", seen);
    }

    // Presence departs with the socket, like any other unmount (§9.5).
    drop(b);
    std::thread::sleep(std::time::Duration::from_millis(150));
    let (_, _, alone) = req(port, "GET", "/api/pad/welcome", None, None);
    let here = alone
        .split("\"here\":[")
        .nth(1)
        .and_then(|rest| rest.split(']').next())
        .unwrap_or("");
    assert_eq!(
        here.matches('"').count() / 2,
        1,
        "one page left, one remains: {}",
        alone
    );

    // Making a pad is a native form post: no client code involved, and the
    // title becomes an address a person can read out loud.
    let (made, head, _) = req(port, "POST", "/new", Some("{\"title\":\"Sprint Notes\"}"), None);
    assert_eq!(made, 302);
    assert!(
        head.contains("/p/sprint-notes"),
        "the title becomes a slug: {}",
        head
    );
    let (_, _, fresh) = req(port, "GET", "/api/pad/sprint-notes", None, None);
    assert!(fresh.contains("\"body\":\"\""), "a new pad starts empty: {}", fresh);

    // The pads outlive the process; presence does not, and neither should.
    drop(a);
    stop.store(true, Ordering::Relaxed);
    join.join().unwrap();
    let (port2, stop2, join2) = start(dir.clone());
    let (_, _, kept) = req(port2, "GET", "/api/pad/welcome", None, None);
    assert!(kept.contains("rewritten"), "stored pads survive a restart: {}", kept);
    assert!(kept.contains("\"here\":[]"), "presence does not: {}", kept);
    stop2.store(true, Ordering::Relaxed);
    join2.join().unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}

/// Parse `name -> port` pairs out of a showcase launcher or the gallery page.
/// Each of the three files states the map in its own syntax; this reduces them
/// to the same set so they can be compared.
fn showcase_map(text: &str) -> std::collections::BTreeMap<String, u16> {
    let mut out = std::collections::BTreeMap::new();
    for line in text.lines() {
        let l = line.trim();
        // serve.sh:    "counter:8081"
        // serve.ps1:   @{ name = 'counter';    port = 8081 }
        // index.html:  { name: "counter", port: 8081, blurb: "..." },
        let (name, rest) = if l.starts_with('"') && l.contains(':') && !l.contains('=') {
            let inner = l.trim_matches('"');
            match inner.split_once(':') {
                Some((n, p)) => (n.to_string(), p.to_string()),
                None => continue,
            }
        } else if let Some(i) = l.find("name") {
            let after = &l[i + 4..];
            let name: String = after
                .chars()
                .skip_while(|c| !matches!(c, '\'' | '"'))
                .skip(1)
                .take_while(|c| !matches!(c, '\'' | '"'))
                .collect();
            match l.find("port") {
                Some(j) => (name, l[j + 4..].to_string()),
                None => continue,
            }
        } else {
            continue;
        };
        // The FIRST digit run only: a blurb like "A 20fps game" sits on the
        // same line in index.html and would otherwise join the port.
        let port: String = rest
            .chars()
            .skip_while(|c| !c.is_ascii_digit())
            .take_while(|c| c.is_ascii_digit())
            .collect();
        if name.is_empty() || port.len() != 4 {
            continue;
        }
        if let Ok(p) = port.parse::<u16>() {
            out.insert(name, p);
        }
    }
    out
}

/// The gallery's own view of the world: name -> port, read out of the URLs in
/// `examples/gallery/settings.json`. This is deployment data, not source — the
/// program that renders it contains no address at all (B5).
fn gallery_map(text: &str) -> std::collections::BTreeMap<String, u16> {
    let mut out = std::collections::BTreeMap::new();
    let mut name: Option<String> = None;
    for chunk in text.split('"') {
        // Entries are `{ "name": "counter", ..., "url": "http://127.0.0.1:8081" }`,
        // so a quoted run either IS the name value or IS the url value; both
        // arrive in order within one object.
        if let Some(rest) = chunk.strip_prefix("http://127.0.0.1:") {
            let port: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
            if let (Some(n), Ok(p)) = (name.take(), port.parse::<u16>()) {
                out.insert(n, p);
            }
            continue;
        }
        // A bare identifier-looking run is a candidate name; the url that
        // follows within the same object claims it.
        if !chunk.is_empty()
            && chunk.chars().all(|c| c.is_ascii_lowercase())
            && chunk.len() > 2
            && chunk != "name"
            && chunk != "blurb"
            && chunk != "url"
            && chunk != "label"
            && chunk != "sites"
        {
            name = Some(chunk.to_string());
        }
    }
    out
}

#[test]
fn t_examples_showcase_launchers_agree_on_every_port() {
    // The showcase states its name->port map three times — serve.sh, serve.ps1,
    // and the gallery's own settings.json — because each has to be readable on
    // its own. Three copies of a fact is a drift hazard, so this is the test
    // that makes drift impossible instead of a comment asking nicely.
    //
    // The two launchers agree exactly. The gallery's settings list everything
    // it FRAMES, which is every example except the gallery itself: a page
    // cannot be an item in its own sidebar.
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let read = |rel: &str| {
        std::fs::read_to_string(root.join(rel)).unwrap_or_else(|e| panic!("{}: {}", rel, e))
    };
    let sh = showcase_map(&read("showcase/serve.sh"));
    let ps = showcase_map(&read("showcase/serve.ps1"));
    let gallery = gallery_map(&read("examples/gallery/settings.json"));

    assert!(!sh.is_empty(), "parsed no name:port pairs out of serve.sh");
    assert!(!gallery.is_empty(), "parsed no name->url pairs out of the gallery settings");
    assert_eq!(sh, ps, "showcase/serve.sh and serve.ps1 disagree about ports");

    let mut framed = sh.clone();
    framed.remove("gallery");
    assert_eq!(
        framed, gallery,
        "the gallery's settings.json and the launchers disagree about where the \
         examples serve — an operator would get dead frames"
    );
    assert!(
        sh.contains_key("gallery"),
        "the launchers must start the gallery itself, or there is nothing to open"
    );

    // Every launched name is a real example, and every example is launched —
    // so a new example cannot quietly stay out of the gallery.
    let mut on_disk: Vec<String> = Vec::new();
    for entry in std::fs::read_dir(examples_root()).expect("examples/").flatten() {
        if entry.path().is_dir() {
            on_disk.push(entry.file_name().to_string_lossy().to_string());
        }
    }
    on_disk.sort();
    let launched: Vec<String> = sh.keys().cloned().collect();
    assert_eq!(
        launched, on_disk,
        "the showcase must launch exactly the examples that exist"
    );

    // Ports are distinct, or two examples fight over one socket.
    let mut ports: Vec<u16> = sh.values().copied().collect();
    ports.sort();
    let before = ports.len();
    ports.dedup();
    assert_eq!(before, ports.len(), "two showcase examples share a port");
}

#[test]
fn t_examples_gallery_frames_a_chosen_example() {
    // The showcase is an Ashlar program now. It could not be one before
    // settings existed: a page of links needs addresses, B5 forbade a location
    // in source, and `std` has no file I/O — so the gallery lived as
    // hand-written HTML opened over a local file URL, outside the language it
    // was advertising. This test is the proof that it came inside.
    let dir = staged("gallery");
    let (port, stop, join) = start(dir.clone());
    let (status, _, html) = req(port, "GET", "/", None, None);
    assert_eq!(status, 200);

    // Every example the settings name is on the page, under its heading —
    // and the lead is on the stage above them all.
    for name in [
        "counter", "todo", "poll", "ticker", "pong", "foundry", "press", "guardrails", "diary",
        "locker", "ledger", "abacus", "enclave", "commons", "slate", "hello",
    ] {
        assert!(
            html.contains(&format!(">{}<", name)),
            "the page is missing `{}`: {}",
            name,
            html
        );
    }
    assert!(
        html.contains("Realtime") && html.contains("Flagship"),
        "section headings come from the settings too: {}",
        html
    );
    assert!(
        html.contains("class=\"stage-name\">enclave<"),
        "the lead example is a setting, and it is on the stage: {}",
        html
    );

    // Every frame on the page is a real address, and every address on the
    // page is one deployment supplied. This is the load-bearing pair: the
    // gallery renders locations it was HANDED, and holds none of its own.
    // (It used to assert that no address appeared before a click, which was
    // the same claim about a page that framed one example at a time. This
    // one frames them all, so the claim is made about where they came from.)
    let settings = std::fs::read_to_string(dir.join("settings.json")).unwrap();
    let mut framed = 0;
    for port_n in 8081..=8097u16 {
        let addr = format!("http://127.0.0.1:{}", port_n);
        assert_eq!(
            html.contains(&addr),
            settings.contains(&addr),
            "`{}` is on the page but not in the settings, or the other way about",
            addr
        );
        if html.contains(&addr) {
            framed += 1;
        }
    }
    assert_eq!(framed, 16, "every example except the gallery itself is framed");

    // Click the first tile over the socket. The handler is an inline function
    // in the button's attrs (§9.4) closing over the mapped Site, so this also
    // exercises E024's call-argument case.
    let (inst, h) = event_target(&html, "onclick", 0).unwrap();
    let mut ws = ws_open(port);
    ws_send(
        &mut ws,
        &format!("{{\"event\":{{\"instance\":\"{}\",\"h\":\"{}\",\"name\":\"onclick\"}}}}", inst, h),
    );
    let reply = unescape(&ws_expect(&mut ws, "stage-frame", 8));
    assert!(
        reply.contains("class=\"stage-frame\" src=\"http://127.0.0.1:8094\" title=\"hello\""),
        "the stage must move to the clicked example's deployed address: {}",
        reply
    );
    assert!(reply.contains("tile on"), "the promoted tile marks itself: {}", reply);
    assert!(
        reply.contains("ashlar run examples/hello"),
        "the stage names the command for whatever is on it: {}",
        reply
    );
    assert!(
        reply.contains("back to enclave"),
        "and how to put the lead back: {}",
        reply
    );

    // And the address is nowhere in the source that rendered it.
    let src = std::fs::read_to_string(dir.join("gallery.ash")).unwrap();
    assert!(
        !src.contains("127.0.0.1") && !src.contains("http"),
        "gallery.ash must contain no location — that is the entire point of it"
    );

    stop.store(true, Ordering::Relaxed);
    join.join().unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}
