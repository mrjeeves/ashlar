//! `ashlar run` (reference §9): a single-binary server, no dependencies.
//!
//! Architecture: one single-threaded event loop owns the whole runtime —
//! the evaluator is deliberately `!Send` (function values are `Rc`), so
//! requests, scheduled tasks, hot reload, and shutdown all interleave on
//! one thread via a non-blocking accept loop. Correct first; F1 governs
//! build latency, not request throughput.
//!
//! Transport invisibility (G2): HTTP requests and WebSocket `{path, data}`
//! envelopes dispatch through the same `dispatch()` — handlers cannot
//! observe the transport. The WebSocket handshake is hand-rolled
//! (SHA-1 + base64, RFC 6455) like everything else here.
//!
//! Hot reload (G3): source mtimes are polled; on change the project is
//! rebuilt and the state store carries over by full dotted name, so
//! `state`/`synced`/`stored` values survive an edit.
//!
//! Persistence (§9.3): `stored` values flush to `.ashlar-state.json` in
//! the project root whenever dirty, and load (with shape-agnostic JSON
//! decoding; the checker's startup validation is the shape gate) at boot.

use crate::eval::{from_json, to_json, to_text, Evaluator, Fault, V};
use crate::resolved::MergedValue;
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

// ---------------------------------------------------------------------------
// SHA-1 and base64 (for the RFC 6455 handshake; no external crates).
// ---------------------------------------------------------------------------

pub fn sha1(data: &[u8]) -> [u8; 20] {
    let mut h: [u32; 5] = [0x67452301, 0xEFCDAB89, 0x98BADCFE, 0x10325476, 0xC3D2E1F0];
    let ml = (data.len() as u64) * 8;
    let mut msg = data.to_vec();
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&ml.to_be_bytes());
    for chunk in msg.chunks(64) {
        let mut w = [0u32; 80];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([
                chunk[i * 4],
                chunk[i * 4 + 1],
                chunk[i * 4 + 2],
                chunk[i * 4 + 3],
            ]);
        }
        for i in 16..80 {
            w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
        }
        let (mut a, mut b, mut c, mut d, mut e) = (h[0], h[1], h[2], h[3], h[4]);
        for (i, &wi) in w.iter().enumerate() {
            let (f, k) = match i {
                0..=19 => ((b & c) | ((!b) & d), 0x5A827999u32),
                20..=39 => (b ^ c ^ d, 0x6ED9EBA1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1BBCDC),
                _ => (b ^ c ^ d, 0xCA62C1D6),
            };
            let tmp = a
                .rotate_left(5)
                .wrapping_add(f)
                .wrapping_add(e)
                .wrapping_add(k)
                .wrapping_add(wi);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = tmp;
        }
        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
    }
    let mut out = [0u8; 20];
    for (i, x) in h.iter().enumerate() {
        out[i * 4..i * 4 + 4].copy_from_slice(&x.to_be_bytes());
    }
    out
}

pub fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in data.chunks(3) {
        let b = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | (b[2] as u32);
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            T[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            T[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

// ---------------------------------------------------------------------------
// HTTP request/response plumbing.
// ---------------------------------------------------------------------------

pub struct HttpRequest {
    pub method: String,
    pub path: String,
    pub headers: BTreeMap<String, String>,
    pub body: Vec<u8>,
}

/// Read one HTTP/1.1 request. `None` on connection close or malformed
/// input (the connection is simply dropped; a server never panics).
pub fn read_request(stream: &mut TcpStream) -> Option<HttpRequest> {
    let mut buf: Vec<u8> = Vec::new();
    let mut tmp = [0u8; 4096];
    let header_end = loop {
        if let Some(i) = find_subslice(&buf, b"\r\n\r\n") {
            break i;
        }
        let n = stream.read(&mut tmp).ok()?;
        if n == 0 {
            return None;
        }
        buf.extend_from_slice(&tmp[..n]);
        if buf.len() > 1 << 20 {
            return None;
        }
    };
    let head = String::from_utf8_lossy(&buf[..header_end]).to_string();
    let mut lines = head.split("\r\n");
    let request_line = lines.next()?;
    let mut rl = request_line.split(' ');
    let method = rl.next()?.to_string();
    let target = rl.next()?.to_string();
    let path = target.split('?').next().unwrap_or("").to_string();
    let mut headers = BTreeMap::new();
    for line in lines {
        if let Some((k, v)) = line.split_once(':') {
            headers.insert(k.trim().to_lowercase(), v.trim().to_string());
        }
    }
    let content_length: usize = headers
        .get("content-length")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    let mut body = buf[header_end + 4..].to_vec();
    while body.len() < content_length {
        let n = stream.read(&mut tmp).ok()?;
        if n == 0 {
            break;
        }
        body.extend_from_slice(&tmp[..n]);
    }
    body.truncate(content_length);
    Some(HttpRequest {
        method,
        path,
        headers,
        body,
    })
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|w| w == needle)
}

pub fn write_response(
    stream: &mut TcpStream,
    status: u16,
    content_type: &str,
    extra_headers: &[(String, String)],
    body: &[u8],
) {
    let reason = match status {
        200 => "OK",
        302 => "Found",
        403 => "Forbidden",
        404 => "Not Found",
        _ => "Error",
    };
    let mut head = format!(
        "HTTP/1.1 {} {}\r\ncontent-type: {}\r\ncontent-length: {}\r\n",
        status,
        reason,
        content_type,
        body.len()
    );
    for (k, v) in extra_headers {
        head.push_str(&format!("{}: {}\r\n", k, v));
    }
    head.push_str("connection: close\r\n\r\n");
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(body);
}

// ---------------------------------------------------------------------------
// WebSocket frames (RFC 6455, text frames only — the envelope protocol).
// ---------------------------------------------------------------------------

pub fn ws_accept_key(client_key: &str) -> String {
    let magic = format!("{}258EAFA5-E914-47DA-95CA-C5AB0DC85B11", client_key);
    base64(&sha1(magic.as_bytes()))
}

/// Read one text frame (opcode 1). Returns `None` on close/error.
pub fn ws_read_text(stream: &mut TcpStream) -> Option<String> {
    loop {
        let mut hdr = [0u8; 2];
        stream.read_exact(&mut hdr).ok()?;
        let opcode = hdr[0] & 0x0F;
        let masked = hdr[1] & 0x80 != 0;
        let mut len = (hdr[1] & 0x7F) as u64;
        if len == 126 {
            let mut ext = [0u8; 2];
            stream.read_exact(&mut ext).ok()?;
            len = u16::from_be_bytes(ext) as u64;
        } else if len == 127 {
            let mut ext = [0u8; 8];
            stream.read_exact(&mut ext).ok()?;
            len = u64::from_be_bytes(ext);
        }
        if len > 1 << 20 {
            return None;
        }
        let mask = if masked {
            let mut m = [0u8; 4];
            stream.read_exact(&mut m).ok()?;
            Some(m)
        } else {
            None
        };
        let mut payload = vec![0u8; len as usize];
        stream.read_exact(&mut payload).ok()?;
        if let Some(m) = mask {
            for (i, b) in payload.iter_mut().enumerate() {
                *b ^= m[i % 4];
            }
        }
        match opcode {
            1 => return String::from_utf8(payload).ok(),
            8 => return None,                      // close
            9 => ws_write_frame(stream, 10, &payload), // ping -> pong
            _ => {}                                // pong/continuation ignored
        }
    }
}

pub fn ws_write_text(stream: &mut TcpStream, text: &str) {
    ws_write_frame(stream, 1, text.as_bytes());
}

fn ws_write_frame(stream: &mut TcpStream, opcode: u8, payload: &[u8]) {
    let mut frame = vec![0x80 | opcode];
    let len = payload.len();
    if len < 126 {
        frame.push(len as u8);
    } else if len < 1 << 16 {
        frame.push(126);
        frame.extend_from_slice(&(len as u16).to_be_bytes());
    } else {
        frame.push(127);
        frame.extend_from_slice(&(len as u64).to_be_bytes());
    }
    frame.extend_from_slice(payload);
    let _ = stream.write_all(&frame);
}

// ---------------------------------------------------------------------------
// Routing and dispatch (shared by HTTP and WebSocket — G2).
// ---------------------------------------------------------------------------

/// Match a request path against a route pattern; captures fill the map.
fn match_route(pattern: &str, path: &str) -> Option<BTreeMap<String, V>> {
    let ps: Vec<&str> = pattern.trim_matches('/').split('/').collect();
    let xs: Vec<&str> = path.trim_matches('/').split('/').collect();
    if ps.len() != xs.len() {
        return None;
    }
    let mut params = BTreeMap::new();
    for (p, x) in ps.iter().zip(&xs) {
        if p.starts_with('{') && p.ends_with('}') {
            params.insert(p[1..p.len() - 1].to_string(), V::Text((*x).to_string()));
        } else if p != x {
            return None;
        }
    }
    Some(params)
}

/// The routed parts of a program: (full name, pattern, pipe-reverse flag).
fn routes(ev: &Evaluator) -> Vec<(String, String, bool)> {
    let mut out = Vec::new();
    for (full, cp) in ev.composed.iter() {
        let Some(prop) = cp.props.get("route") else { continue };
        let pattern = match &prop.value {
            MergedValue::Single(r) => ev.program.files[r.file_idx].ast.parts[r.part_idx].props
                [r.prop_idx]
                .value
                .as_ref()
                .and_then(|e| match &e.expr {
                    crate::ast::Expr::Text(t) => Some(t.clone()),
                    _ => None,
                }),
            MergedValue::Literal(e) => match &e.expr {
                crate::ast::Expr::Text(t) => Some(t.clone()),
                _ => None,
            },
            _ => None,
        };
        let Some(pattern) = pattern else { continue };
        let reverse = cp
            .props
            .get("handle")
            .and_then(|p| p.kind)
            .map(|(_, r)| r)
            .unwrap_or(false);
        out.push((full.clone(), pattern, reverse));
    }
    out
}

/// One request through the routed program — identical for both transports.
pub fn dispatch(
    ev: &mut Evaluator,
    method: &str,
    path: &str,
    data: V,
    headers: &BTreeMap<String, String>,
) -> Result<V, Fault> {
    let table = routes(ev);
    let Some((part, params, reverse)) = table.iter().find_map(|(part, pattern, rev)| {
        match_route(pattern, path).map(|p| (part.clone(), p, *rev))
    }) else {
        return Err(Fault {
            status: 404,
            message: format!("no route matches `{}`.", path),
        });
    };

    let mut req = BTreeMap::new();
    req.insert("path".to_string(), V::Text(path.to_string()));
    req.insert("method".to_string(), V::Text(method.to_lowercase()));
    req.insert("params".to_string(), V::Map(params));
    req.insert("data".to_string(), data);
    req.insert(
        "headers".to_string(),
        V::Map(
            headers
                .iter()
                .map(|(k, v)| (k.clone(), V::Text(v.clone())))
                .collect(),
        ),
    );
    req.insert("user".to_string(), V::None);
    let req = V::Map(req);

    // The allow guard runs first (§9.6): false ends the request with 403.
    if ev
        .composed
        .get(&part)
        .map(|cp| cp.props.contains_key("allow"))
        .unwrap_or(false)
    {
        match ev.call_prop(&part, "allow", vec![req.clone()])? {
            V::Bool(true) => {}
            _ => {
                return Err(Fault {
                    status: 403,
                    message: "forbidden.".to_string(),
                })
            }
        }
    }

    ev.run_pipe(&part, "handle", reverse, req)
}

/// Render a handler's return value as an HTTP response (§9.2).
pub fn render_response(v: &V) -> (u16, String, Vec<u8>, Vec<(String, String)>) {
    if let V::Map(m) = v {
        if let Some(target) = m.get("__redirect") {
            return (
                302,
                "text/plain".to_string(),
                Vec::new(),
                vec![("location".to_string(), to_text(target))],
            );
        }
        if m.contains_key("__el") {
            let html = format!("<!doctype html>\n{}", render_el(v));
            return (200, "text/html".to_string(), html.into_bytes(), vec![]);
        }
    }
    match v {
        V::Text(s) => (200, "text/plain".to_string(), s.clone().into_bytes(), vec![]),
        other => (
            200,
            "application/json".to_string(),
            to_json(other).into_bytes(),
            vec![],
        ),
    }
}

fn render_el(v: &V) -> String {
    match v {
        V::Map(m) if m.contains_key("__el") => {
            let tag = m.get("__el").map(to_text).unwrap_or_default();
            let mut out = format!("<{}", tag);
            if let Some(V::Map(attrs)) = m.get("attrs") {
                for (k, val) in attrs {
                    match val {
                        V::Fn(_) => out.push_str(&format!(" data-on=\"{}\"", k)),
                        other => out.push_str(&format!(
                            " {}=\"{}\"",
                            k,
                            html_escape(&to_text(other))
                        )),
                    }
                }
            }
            out.push('>');
            if let Some(V::List(children)) = m.get("children") {
                for c in children {
                    out.push_str(&render_el(c));
                }
            }
            out.push_str(&format!("</{}>", tag));
            out
        }
        other => html_escape(&to_text(other)),
    }
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

// ---------------------------------------------------------------------------
// The serve loop.
// ---------------------------------------------------------------------------

/// Run the project at `root`. `override_port` (tests) wins over the
/// program's `port`; `ready` receives the bound port; `stop` ends the
/// loop from another thread.
pub fn serve(
    root: PathBuf,
    override_port: Option<u16>,
    ready: impl FnOnce(u16),
    stop: Arc<AtomicBool>,
) -> Result<(), String> {
    let mut carry: Option<BTreeMap<String, V>> = None;
    let mut ready = Some(ready);
    let mut listener: Option<TcpListener> = None;

    loop {
        let result = crate::check_project(&root);
        if result.has_errors() {
            return Err(result
                .diags
                .iter()
                .map(|d| d.human())
                .collect::<Vec<_>>()
                .join("\n"));
        }

        // The server root is the part that declares `port` (§9.1).
        let port_part = result
            .composed
            .iter()
            .find(|(_, cp)| cp.props.contains_key("port"))
            .map(|(full, _)| full.clone())
            .ok_or_else(|| "no part declares `port`; nothing to run.".to_string())?;

        let mut ev = Evaluator::new(&result.program, &result.composed);

        // Persistence: load stored values (shape validation happened in
        // the checker; unknown keys are ignored).
        let state_path = root.join(".ashlar-state.json");
        if carry.is_none() {
            if let Ok(text) = std::fs::read_to_string(&state_path) {
                if let Some(V::Map(m)) = from_json(&text) {
                    for (k, v) in m {
                        if ev.state.stored_keys.iter().any(|s| s == &k) {
                            ev.state.values.insert(k, v);
                        }
                    }
                }
            }
        }

        // Hot reload carry-over: state values survive by full name (G3).
        let is_reload = carry.is_some();
        ev.run_stack(&port_part, "start", false)
            .map_err(|f| format!("start failed: {}", f))?;
        if let Some(old) = carry.take() {
            for (k, v) in old {
                if ev.state.values.contains_key(&k) {
                    ev.state.values.insert(k, v);
                }
            }
        }
        if is_reload {
            eprintln!("reloaded");
        }

        let port = override_port
            .or_else(|| ev.prop_number(&port_part, "port").map(|n| n as u16))
            .unwrap_or(8080);

        if listener.is_none() {
            let l = TcpListener::bind(("127.0.0.1", port))
                .map_err(|e| format!("bind failed: {}", e))?;
            l.set_nonblocking(true)
                .map_err(|e| format!("nonblocking failed: {}", e))?;
            if let Some(r) = ready.take() {
                r(l.local_addr().map(|a| a.port()).unwrap_or(port));
            }
            listener = Some(l);
        }

        // Scheduled tasks (§9.7): parts with `every` + `run`.
        let mut schedule: Vec<(String, u64, std::time::Instant)> = Vec::new();
        for (full, cp) in result.composed.iter() {
            if let Some(prop) = cp.props.get("every") {
                let text = match &prop.value {
                    MergedValue::Single(r) => result.program.files[r.file_idx].ast.parts
                        [r.part_idx]
                        .props[r.prop_idx]
                        .value
                        .as_ref()
                        .and_then(|e| match &e.expr {
                            crate::ast::Expr::Text(t) => Some(t.clone()),
                            _ => None,
                        }),
                    _ => None,
                };
                if let Some(ms) = text.as_deref().and_then(duration_ms) {
                    schedule.push((
                        full.clone(),
                        ms,
                        std::time::Instant::now() + std::time::Duration::from_millis(ms),
                    ));
                }
            }
        }

        let mut last_mtime = source_mtime(&root);
        let mut last_scan = std::time::Instant::now();
        let exit = 'inner: loop {
            if stop.load(Ordering::Relaxed) {
                break 'inner Exit::Stop;
            }
            // Accept one connection if pending.
            match listener.as_ref().unwrap().accept() {
                Ok((mut conn, _)) => {
                    let _ = conn.set_nonblocking(false);
                    handle_conn(&mut ev, &mut conn);
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(std::time::Duration::from_millis(5));
                }
                Err(_) => {}
            }
            // Scheduled tasks due?
            let now = std::time::Instant::now();
            for (part, ms, due) in schedule.iter_mut() {
                if now >= *due {
                    if let Err(f) = ev.call_prop(part, "run", vec![]) {
                        eprintln!("task {} failed: {}", part, f);
                    }
                    *due = now + std::time::Duration::from_millis(*ms);
                }
            }
            // Flush stored state.
            if ev.state.dirty {
                flush_state(&state_path, &ev);
                ev.state.dirty = false;
            }
            // Drain logs.
            for line in ev.log.drain(..) {
                eprintln!("{}", line);
            }
            // Hot reload check (every ~500ms).
            if now.duration_since(last_scan).as_millis() > 500 {
                last_scan = now;
                let m = source_mtime(&root);
                if m != last_mtime {
                    last_mtime = m;
                    break 'inner Exit::Reload;
                }
            }
        };

        match exit {
            Exit::Stop => {
                // Shutdown (§9.1): stop stack reverse, then flush.
                let _ = ev.run_stack(&port_part, "stop", true);
                flush_state(&state_path, &ev);
                return Ok(());
            }
            Exit::Reload => {
                carry = Some(ev.state.values.clone());
                continue;
            }
        }
    }
}

enum Exit {
    Stop,
    Reload,
}

fn duration_ms(t: &str) -> Option<u64> {
    for (suffix, mult) in [("ms", 1u64), ("s", 1000), ("m", 60_000), ("h", 3_600_000), ("d", 86_400_000)] {
        if let Some(num) = t.strip_suffix(suffix) {
            if !num.is_empty() && num.chars().all(|c| c.is_ascii_digit()) {
                return num.parse::<u64>().ok().map(|n| n * mult);
            }
        }
    }
    None
}

fn source_mtime(root: &std::path::Path) -> u128 {
    crate::find_ash_files(root)
        .iter()
        .filter_map(|p| std::fs::metadata(p).ok())
        .filter_map(|m| m.modified().ok())
        .filter_map(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_nanos())
        .max()
        .unwrap_or(0)
}

fn flush_state(path: &std::path::Path, ev: &Evaluator) {
    let mut m = BTreeMap::new();
    for k in &ev.state.stored_keys {
        if let Some(v) = ev.state.values.get(k) {
            m.insert(k.clone(), v.clone());
        }
    }
    let _ = std::fs::write(path, to_json(&V::Map(m)));
}

/// Serve one connection: plain HTTP, or a WebSocket session speaking
/// `{path, data}` envelopes to the same routes (§9.2, G2).
fn handle_conn(ev: &mut Evaluator, conn: &mut TcpStream) {
    let Some(req) = read_request(conn) else { return };

    if req
        .headers
        .get("upgrade")
        .map(|u| u.eq_ignore_ascii_case("websocket"))
        .unwrap_or(false)
    {
        let key = req.headers.get("sec-websocket-key").cloned().unwrap_or_default();
        let accept = ws_accept_key(&key);
        let head = format!(
            "HTTP/1.1 101 Switching Protocols\r\nupgrade: websocket\r\nconnection: Upgrade\r\nsec-websocket-accept: {}\r\n\r\n",
            accept
        );
        let _ = conn.write_all(head.as_bytes());
        while let Some(text) = ws_read_text(conn) {
            let envelope = from_json(&text).unwrap_or(V::None);
            let (path, data, method) = match &envelope {
                V::Map(m) => (
                    m.get("path").map(to_text).unwrap_or_default(),
                    m.get("data").cloned().unwrap_or(V::None),
                    m.get("method").map(to_text).unwrap_or_else(|| "get".to_string()),
                ),
                _ => (String::new(), V::None, "get".to_string()),
            };
            let reply = match dispatch(ev, &method, &path, data, &req.headers) {
                Ok(v) => {
                    let mut m = BTreeMap::new();
                    m.insert("status".to_string(), V::Number(200.0));
                    m.insert("data".to_string(), v);
                    V::Map(m)
                }
                Err(f) => {
                    let mut m = BTreeMap::new();
                    m.insert("status".to_string(), V::Number(f.status as f64));
                    m.insert("error".to_string(), V::Text(f.message));
                    V::Map(m)
                }
            };
            ws_write_text(conn, &to_json(&reply));
        }
        return;
    }

    let data = if req.body.is_empty() {
        V::None
    } else {
        String::from_utf8(req.body.clone())
            .ok()
            .and_then(|s| from_json(&s))
            .unwrap_or(V::None)
    };
    match dispatch(ev, &req.method, &req.path, data, &req.headers) {
        Ok(v) => {
            let (status, ct, body, extra) = render_response(&v);
            write_response(conn, status, &ct, &extra, &body);
        }
        Err(f) => {
            let body = format!(
                "{{\"error\":{}}}",
                {
                    let mut s = String::new();
                    crate::diag::push_json_str(&mut s, &f.message);
                    s
                }
            );
            write_response(conn, f.status, "application/json", &[], body.as_bytes());
        }
    }
}
