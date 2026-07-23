//! `ashlar run` (reference §9): a single-binary server, no dependencies.
//!
//! Architecture: one single-threaded event loop owns the whole runtime —
//! the evaluator is deliberately `!Send` (function values are `Rc`), so
//! requests, scheduled tasks, hot reload, and shutdown all interleave on
//! one thread. Nothing on the loop may ever block on one socket: requests
//! assemble in a pending list read without blocking (a socket that never
//! completes one — a browser's speculative preconnect, a stalled client —
//! is dropped at a deadline), and outbound WebSocket bytes queue per
//! connection and flush as each peer drains, with a cap that sheds
//! consumers who stopped reading. Correct first; F1 governs build
//! latency, not request throughput.
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

/// `application/x-www-form-urlencoded` -> a map of text values (§9.2):
/// `a=1&b=hi%20there` decodes keys and values with `+` and `%XX` rules.
fn decode_form(body: &str) -> V {
    fn unescape(s: &str) -> String {
        let bytes = s.as_bytes();
        let mut out = Vec::with_capacity(bytes.len());
        let mut i = 0;
        while i < bytes.len() {
            match bytes[i] {
                b'+' => {
                    out.push(b' ');
                    i += 1;
                }
                b'%' if i + 2 < bytes.len() => {
                    let hex = std::str::from_utf8(&bytes[i + 1..i + 3])
                        .ok()
                        .and_then(|h| u8::from_str_radix(h, 16).ok());
                    match hex {
                        Some(b) => {
                            out.push(b);
                            i += 3;
                        }
                        None => {
                            out.push(bytes[i]);
                            i += 1;
                        }
                    }
                }
                b => {
                    out.push(b);
                    i += 1;
                }
            }
        }
        String::from_utf8_lossy(&out).to_string()
    }
    let mut m = BTreeMap::new();
    for pair in body.split('&') {
        if pair.is_empty() {
            continue;
        }
        let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
        m.insert(unescape(k), V::Text(unescape(v)));
    }
    V::Map(m)
}

/// A request body larger than this is refused with 413 — the cap keeps
/// one client from holding megabytes of loop memory, and the refusal
/// names the limit instead of resetting the connection.
pub const BODY_CAP: usize = 16 << 20;

/// Hard bound on one socket's buffered request bytes (headers + body +
/// slack); the pump stops reading past it so a flood cannot grow a
/// buffer without limit inside a single tick.
const REQ_MAX: usize = BODY_CAP + (1 << 20) + 4096;

/// The state of one socket's buffered bytes as a request-in-progress.
/// `Incomplete` means keep the socket and read more; `Bad` means the
/// bytes can never become a request — drop the socket; `TooLarge`
/// deserves a 413 before the drop.
pub enum Parse {
    Incomplete,
    Bad,
    TooLarge,
    Ready(HttpRequest),
}

/// Try to assemble one complete request from buffered bytes, consuming
/// nothing. The event loop calls this after every non-blocking read, so
/// a socket that never completes a request (a browser's speculative
/// preconnect sends no bytes at all) costs the loop nothing.
pub fn parse_request(buf: &[u8]) -> Parse {
    let Some(header_end) = find_subslice(buf, b"\r\n\r\n") else {
        return if buf.len() > 1 << 20 { Parse::Bad } else { Parse::Incomplete };
    };
    let head = String::from_utf8_lossy(&buf[..header_end]).to_string();
    let mut lines = head.split("\r\n");
    let Some(request_line) = lines.next() else {
        return Parse::Bad;
    };
    let mut rl = request_line.split(' ');
    let (Some(method), Some(target)) = (rl.next(), rl.next()) else {
        return Parse::Bad;
    };
    let method = method.to_string();
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
    if content_length > BODY_CAP {
        return Parse::TooLarge;
    }
    let avail = &buf[header_end + 4..];
    if avail.len() < content_length {
        return Parse::Incomplete;
    }
    let mut body = avail.to_vec();
    body.truncate(content_length);
    Parse::Ready(HttpRequest {
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

/// Build one complete response as bytes. Writing is the caller's
/// problem — on the event loop nothing may block, so the bytes drain
/// through the closing list as the peer accepts them.
pub fn response_bytes(
    status: u16,
    content_type: &str,
    extra_headers: &[(String, String)],
    body: &[u8],
) -> Vec<u8> {
    let reason = match status {
        200 => "OK",
        302 => "Found",
        403 => "Forbidden",
        404 => "Not Found",
        413 => "Payload Too Large",
        _ => "Error",
    };
    // Every response is live output of the running program: a cached
    // page carries instance ids a restarted server no longer knows, and
    // heuristic caching (no validators are sent) makes browsers do
    // exactly that. `no-store` ends the whole class of stale-page
    // breakage.
    let mut head = format!(
        "HTTP/1.1 {} {}\r\ncontent-type: {}\r\ncontent-length: {}\r\ncache-control: no-store\r\n",
        status,
        reason,
        content_type,
        body.len()
    );
    for (k, v) in extra_headers {
        head.push_str(&format!("{}: {}\r\n", k, v));
    }
    head.push_str("connection: close\r\n\r\n");
    let mut out = head.into_bytes();
    out.extend_from_slice(body);
    out
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

/// Parse one complete frame out of a connection buffer, if present.
/// Returns (opcode, payload) and drains the consumed bytes.
pub fn ws_frame_from_buf(buf: &mut Vec<u8>) -> Option<(u8, Vec<u8>)> {
    if buf.len() < 2 {
        return None;
    }
    let opcode = buf[0] & 0x0F;
    let masked = buf[1] & 0x80 != 0;
    let mut len = (buf[1] & 0x7F) as u64;
    let mut off = 2usize;
    if len == 126 {
        if buf.len() < off + 2 {
            return None;
        }
        len = u16::from_be_bytes([buf[off], buf[off + 1]]) as u64;
        off += 2;
    } else if len == 127 {
        if buf.len() < off + 8 {
            return None;
        }
        let mut b = [0u8; 8];
        b.copy_from_slice(&buf[off..off + 8]);
        len = u64::from_be_bytes(b);
        off += 8;
    }
    if len > 1 << 20 {
        // Oversized frame: poison the connection by consuming everything.
        buf.clear();
        return Some((8, Vec::new()));
    }
    let mask = if masked {
        if buf.len() < off + 4 {
            return None;
        }
        let m = [buf[off], buf[off + 1], buf[off + 2], buf[off + 3]];
        off += 4;
        Some(m)
    } else {
        None
    };
    let total = off + len as usize;
    if buf.len() < total {
        return None;
    }
    let mut payload = buf[off..total].to_vec();
    if let Some(m) = mask {
        for (i, b) in payload.iter_mut().enumerate() {
            *b ^= m[i % 4];
        }
    }
    buf.drain(..total);
    Some((opcode, payload))
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
    req.insert("user".to_string(), ev.session_user());
    let req = V::Map(req);

    // A routed view part with no handler serves its page (§9.4): one
    // fresh instance per page load.
    let has_handle = ev
        .composed
        .get(&part)
        .map(|cp| cp.props.contains_key("handle"))
        .unwrap_or(false);
    let has_view = ev
        .composed
        .get(&part)
        .map(|cp| cp.props.contains_key("view"))
        .unwrap_or(false);
    if has_view && !has_handle {
        let id = ev.new_instance(&part, std::collections::BTreeMap::new())?;
        let mut m = std::collections::BTreeMap::new();
        m.insert("__view_instance".to_string(), V::Text(id));
        return Ok(V::Map(m));
    }

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

/// The browser side of §9.4, shipped as a constant: connect the socket,
/// forward events from `data-ash-on` elements, swap patched instances.
/// The browser runs no program code — only this transport shim.
const CLIENT_JS: &str = r#"(function(){
var ws=new WebSocket((location.protocol==='https:'?'wss://':'ws://')+location.host+'/');
var sent={},queue=[];
// Clicks before the socket opens must not vanish: queue and flush.
function send(o){var t=JSON.stringify(o);
 if(ws.readyState===1)ws.send(t);else queue.push(t);}
ws.onopen=function(){
 ws.send(JSON.stringify({page:document.body.getAttribute('data-ash-page')}));
 queue.forEach(function(t){ws.send(t)});queue=[];};
function fieldKey(inst,el){var box=document.querySelector('[data-ash-instance="'+inst+'"]');
 if(!box)return null;var all=box.querySelectorAll(el.tagName);
 return inst+':'+el.tagName+':'+Array.prototype.indexOf.call(all,el);}
function fire(kind,e){var t=e.target.closest('[data-ash-h]');
 if(!t||t.getAttribute('data-ash-on')!==kind)return;
 if(kind==='onsubmit')e.preventDefault();
 var v=(e.target&&'value'in e.target)?e.target.value:null;
 var inst=t.closest('[data-ash-instance]').getAttribute('data-ash-instance');
 if(v!==null){var k=fieldKey(inst,e.target);if(k)sent[k]=v;}
 send({event:{instance:inst,h:t.getAttribute('data-ash-h'),name:kind,value:v}});}
document.addEventListener('click',function(e){fire('onclick',e)});
document.addEventListener('input',function(e){fire('oninput',e)});
document.addEventListener('submit',function(e){fire('onsubmit',e)});
ws.onclose=function(){
 (function again(){fetch('/',{cache:'no-store'}).then(function(){location.reload();})
  .catch(function(){setTimeout(again,400);});})();};
// A patch must not eat the user's focus, caret, or typing still in
// flight: remember the focused field (by tag + index inside the
// instance), swap, then restore. The server's value wins only when it
// DIFFERS from what we last sent (an intentional change, e.g. a
// cleared draft); an echo of our own event keeps the live value.
function apply(inst,html){
 var n=document.querySelector('[data-ash-instance="'+inst+'"]');
 if(!n)return;
 var f=document.activeElement,idx=-1,tag='',val=null,s=0,en=0;
 if(f&&n.contains(f)&&('value'in f)){
  tag=f.tagName;var all=n.querySelectorAll(tag);
  idx=Array.prototype.indexOf.call(all,f);
  val=f.value;s=f.selectionStart;en=f.selectionEnd;}
 n.outerHTML=html;
 if(idx>=0){
  var n2=document.querySelector('[data-ash-instance="'+inst+'"]');
  var f2=n2&&n2.querySelectorAll(tag)[idx];
  if(f2){var k=inst+':'+tag+':'+idx;
   if(sent[k]!==undefined&&f2.value===sent[k]){f2.value=val;}
   else{delete sent[k];s=en=f2.value.length;}
   f2.focus();try{f2.setSelectionRange(s,en);}catch(_){}}}}
// Replacing a node mid-gesture kills the gesture: while a pointer is
// held (a drag on a slider, a press on a button), patches to THAT
// element's instance defer — latest wins — and flush on release.
// Everything else keeps updating live.
var held=null,deferred={};
document.addEventListener('pointerdown',function(e){held=e.target;});
function release(){held=null;
 for(var id in deferred){apply(id,deferred[id]);}
 deferred={};}
document.addEventListener('pointerup',release);
document.addEventListener('pointercancel',release);
ws.onmessage=function(m){var d=JSON.parse(m.data);
 if(d.error&&/no (instance|handler)/.test(d.error)){ws.close();return;}
 if(!d.patches)return;
 d.patches.forEach(function(p){
  var n=document.querySelector('[data-ash-instance="'+p.instance+'"]');
  if(!n)return;
  if(held&&n.contains(held)){deferred[p.instance]=p.html;return;}
  apply(p.instance,p.html);
 });};
})();"#;

/// Render a handler's return value as an HTTP response (§9.2).
/// The `<link>` for the root's declared stylesheet (§9.4), or empty when
/// none is declared. The href is `/<name>.css` — a location the runtime
/// derives, never one written in source.
fn style_link(ev: &Evaluator) -> String {
    match &ev.style_name {
        Some(name) => format!(
            "<link rel=\"stylesheet\" href=\"/{}.css\">",
            html_escape(name)
        ),
        None => String::new(),
    }
}

pub fn render_response(
    ev: &mut Evaluator,
    v: &V,
) -> Result<(u16, String, Vec<u8>, Vec<(String, String)>), Fault> {
    if let V::Map(m) = v {
        if let Some(target) = m.get("__redirect") {
            return Ok((
                302,
                "text/plain".to_string(),
                Vec::new(),
                vec![("location".to_string(), to_text(target))],
            ));
        }
        if let Some(V::Text(id)) = m.get("__view_instance") {
            let inner = render_instance(ev, id)?;
            let page = ev.current_page.clone().unwrap_or_default();
            let html = format!(
                "<!doctype html>\n<html><head><meta charset=\"utf-8\">{}</head><body data-ash-page=\"{}\">{}<script>{}</script></body></html>",
                style_link(ev), page, inner, CLIENT_JS
            );
            return Ok((200, "text/html".to_string(), html.into_bytes(), vec![]));
        }
        if m.contains_key("__el") {
            let html = format!("<!doctype html>\n{}", render_el(ev, v, "")?);
            return Ok((200, "text/html".to_string(), html.into_bytes(), vec![]));
        }
    }
    Ok(match v {
        V::Text(s) => (200, "text/plain".to_string(), s.clone().into_bytes(), vec![]),
        other => (
            200,
            "application/json".to_string(),
            to_json(other).into_bytes(),
            vec![],
        ),
    })
}

/// Render one live instance: clear its handler registry, run `view`,
/// wrap the result with its instance marker (§9.4).
pub fn render_instance(ev: &mut Evaluator, id: &str) -> Result<String, Fault> {
    let stale: Vec<(String, String)> = ev
        .handlers
        .keys()
        .filter(|(i, _)| i == id)
        .cloned()
        .collect();
    for k in stale {
        ev.handlers.remove(&k);
    }
    // Handler ids are the render POSITION within this instance, so the
    // same view shape mints the same ids every render — an event from a
    // not-yet-patched page still names the right handler. Rotating ids
    // made every interactive burst race its own re-render.
    ev.render_handler_seq = 0;
    ev.begin_render(id);
    let v = ev.call_instance_prop(id, "view", vec![]);
    ev.end_render();
    let v = v?;
    // The instance IS its view's root element: the marker is stamped onto
    // that element, not a wrapper div around it. A wrapper would sit
    // between a layout container and its child views, so `display: grid`
    // on a parent would see the wrappers instead of the views — the kind
    // of surprise an agent would never guess. A view whose root is not a
    // single element still gets a wrapper, so patching always has one
    // node to swap.
    let stamped = stamp_instance(v, id);
    render_el(ev, &stamped, id)
}

/// Put `data-ash-instance` on the view's own root element when it has
/// one; otherwise wrap the result so the instance is still a single
/// patchable node.
fn stamp_instance(v: V, id: &str) -> V {
    if let V::Map(mut m) = v {
        if m.contains_key("__el") {
            let mut attrs = match m.remove("attrs") {
                Some(V::Map(a)) => a,
                _ => BTreeMap::new(),
            };
            attrs.insert("data-ash-instance".to_string(), V::Text(id.to_string()));
            m.insert("attrs".to_string(), V::Map(attrs));
            return V::Map(m);
        }
        return wrap_instance(V::Map(m), id);
    }
    wrap_instance(v, id)
}

fn wrap_instance(inner: V, id: &str) -> V {
    let mut attrs = BTreeMap::new();
    attrs.insert("data-ash-instance".to_string(), V::Text(id.to_string()));
    let mut wrap = BTreeMap::new();
    wrap.insert("__el".to_string(), V::Text("div".to_string()));
    wrap.insert("attrs".to_string(), V::Map(attrs));
    wrap.insert("children".to_string(), V::List(vec![inner]));
    V::Map(wrap)
}

/// Render an element tree. Function-valued attrs register as event
/// handlers scoped to `instance`; nested view instances recurse.
fn render_el(ev: &mut Evaluator, v: &V, instance: &str) -> Result<String, Fault> {
    match v {
        V::Map(m) if m.contains_key("__view_instance") => {
            if let Some(V::Text(id)) = m.get("__view_instance") {
                let id = id.clone();
                render_instance(ev, &id)
            } else {
                Ok(String::new())
            }
        }
        V::Map(m) if m.contains_key("__el") => {
            let tag = m.get("__el").map(to_text).unwrap_or_default();
            let mut out = format!("<{}", tag);
            if let Some(V::Map(attrs)) = m.get("attrs") {
                for (k, val) in attrs {
                    match val {
                        V::Fn(_) => {
                            ev.render_handler_seq += 1;
                            let hid = format!("h{}", ev.render_handler_seq);
                            ev.handlers
                                .insert((instance.to_string(), hid.clone()), val.clone());
                            out.push_str(&format!(
                                " data-ash-on=\"{}\" data-ash-h=\"{}\"",
                                html_escape(k),
                                hid
                            ));
                        }
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
                    out.push_str(&render_el(ev, c, instance)?);
                }
            }
            out.push_str(&format!("</{}>", tag));
            Ok(out)
        }
        other => Ok(html_escape(&to_text(other))),
    }
}

/// Run one browser event (§9.4): find the handler, run it in its
/// instance, re-render every instance whose state changed.
pub fn dispatch_event(
    ev: &mut Evaluator,
    instance: &str,
    hid: &str,
    name: &str,
    value: V,
) -> Result<Vec<(String, String)>, Fault> {
    let Some(f) = ev.handlers.get(&(instance.to_string(), hid.to_string())).cloned() else {
        return Err(Fault {
            status: 404,
            message: format!("no handler `{}` on instance `{}`.", hid, instance),
        });
    };
    let arity = match &f {
        V::Fn(fv) => fv.params.len(),
        _ => 0,
    };
    let args = if arity == 0 {
        vec![]
    } else {
        let mut data = std::collections::BTreeMap::new();
        data.insert("value".to_string(), value);
        let mut event = std::collections::BTreeMap::new();
        event.insert("name".to_string(), V::Text(name.to_string()));
        event.insert("data".to_string(), V::Map(data));
        vec![V::Map(event)]
    };
    ev.dirty_instances.clear();
    ev.call_with_instance(f, args, Some(instance.to_string()))?;
    let mut dirty: Vec<String> = std::mem::take(&mut ev.dirty_instances);
    if !dirty.contains(&instance.to_string()) {
        // The event's own instance re-renders even on a no-op, so the
        // client always converges with server state.
        dirty.push(instance.to_string());
    }
    let mut patches = Vec::new();
    for id in dirty {
        if ev.instances.contains_key(&id) {
            let html = render_instance(ev, &id)?;
            patches.push((id, html));
        }
    }
    Ok(patches)
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
    root_part: Option<String>,
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

        // The server root is the part that declares `port` (§9.1): the
        // named one, or the program's SINGLE candidate — anything else
        // errors listing the candidates.
        let candidates: Vec<String> = result
            .composed
            .iter()
            .filter(|(_, cp)| cp.props.contains_key("port"))
            .map(|(full, _)| full.clone())
            .collect();
        let port_part = match &root_part {
            Some(name) => {
                if candidates.iter().any(|c| c == name) {
                    name.clone()
                } else if result.composed.contains_key(name) {
                    return Err(format!(
                        "`{}` declares no `port`; it cannot be a server root.",
                        name
                    ));
                } else {
                    return Err(format!("`{}` is not a part in this program.", name));
                }
            }
            None => match candidates.len() {
                0 => return Err("no part declares `port`; nothing to run.".to_string()),
                1 => candidates[0].clone(),
                _ => {
                    return Err(format!(
                        "more than one part declares `port`; name one: {}.",
                        candidates
                            .iter()
                            .map(|c| format!("`ashlar run {}`", c))
                            .collect::<Vec<_>>()
                            .join(", ")
                    ))
                }
            },
        };

        let mut ev = Evaluator::new(&result.program, &result.composed);
        ev.foreign_root = Some(root.clone());

        // §9.4: the server root may name a stylesheet. It resolves like
        // `files` and `foreign` — a name in source, a location the build
        // finds under `assets/`, and a loud error if the named sheet is
        // missing. The runtime links it into every served page's head so
        // `class` names in views bind to its rules by name.
        if let Some(cp) = result.composed.get(&port_part) {
            if let Some(prop) = cp.props.get("style") {
                let name = match &prop.value {
                    MergedValue::Single(r) => result.program.files[r.file_idx].ast.parts
                        [r.part_idx]
                        .props[r.prop_idx]
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
                if let Some(name) = name {
                    let sheet = root.join("assets").join(format!("{}.css", name));
                    if !sheet.is_file() {
                        return Err(format!(
                            "`{}` declares `style = \"{}\"` but assets/{}.css does not exist.",
                            port_part, name, name
                        ));
                    }
                    ev.style_name = Some(name);
                }
            }
        }

        // Persistence: load stored values (shape validation happened in
        // the checker; unknown keys are ignored).
        let state_path = root.join(".ashlar-state.json");
        if carry.is_none() {
            if let Ok(text) = std::fs::read_to_string(&state_path) {
                if let Some(V::Map(m)) = from_json(&text) {
                    for (k, v) in m {
                        if k == "__users" {
                            if let V::Map(users) = v {
                                for (email, u) in users {
                                    if let V::Map(u) = u {
                                        let id = u.get("id").map(to_text).unwrap_or_default();
                                        let hash = u.get("hash").map(to_text).unwrap_or_default();
                                        ev.users.insert(email, (id, hash));
                                    }
                                }
                            }
                        } else if ev.state.stored_keys.iter().any(|s| s == &k) {
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
        let mut ws_conns: Vec<WsConn> = Vec::new();
        let mut pending: Vec<PendingConn> = Vec::new();
        let mut closing: Vec<CloseConn> = Vec::new();
        let exit = 'inner: loop {
            if stop.load(Ordering::Relaxed) {
                break 'inner Exit::Stop;
            }
            // Accept every waiting connection; each joins the pending
            // list and reads without ever blocking the loop. Browsers
            // open sockets speculatively and send nothing on them — one
            // blocking read here once froze the whole runtime.
            let mut accepted = false;
            loop {
                match listener.as_ref().unwrap().accept() {
                    Ok((conn, _)) => {
                        let _ = conn.set_nonblocking(true);
                        pending.push(PendingConn {
                            stream: conn,
                            buf: Vec::new(),
                            deadline: std::time::Instant::now()
                                + std::time::Duration::from_secs(10),
                        });
                        accepted = true;
                    }
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                    Err(_) => break,
                }
            }
            if !accepted {
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            let mut tmp = [0u8; 4096];
            // Pump pending sockets: read what has arrived, answer every
            // complete request, drop the closed, the malformed, and the
            // overdue (a socket that sent nothing for 10s is a
            // speculative preconnect or a scan, not a request).
            let now = std::time::Instant::now();
            let mut p = 0;
            while p < pending.len() {
                let mut drop_pending = now >= pending[p].deadline;
                let mut eof = false;
                if !drop_pending {
                    loop {
                        if pending[p].buf.len() > REQ_MAX {
                            break;
                        }
                        match pending[p].stream.read(&mut tmp) {
                            Ok(0) => {
                                eof = true;
                                break;
                            }
                            Ok(n) => {
                                pending[p].buf.extend_from_slice(&tmp[..n]);
                                // Progress is liveness: a slow uploader
                                // that keeps sending keeps its socket;
                                // only silence runs out the deadline.
                                pending[p].deadline =
                                    now + std::time::Duration::from_secs(10);
                            }
                            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                            Err(_) => {
                                drop_pending = true;
                                break;
                            }
                        }
                    }
                }
                if !drop_pending {
                    match parse_request(&pending[p].buf) {
                        Parse::Ready(req) => {
                            let conn = pending.remove(p).stream;
                            match handle_request(&mut ev, &root, conn, req) {
                                Handled::Ws(ws) => ws_conns.push(ws),
                                Handled::Close(c) => closing.push(c),
                                Handled::Done => {}
                            }
                            ev.current_page = None;
                            continue;
                        }
                        Parse::TooLarge => {
                            // Refusal is still a correction: name the
                            // limit instead of resetting the socket.
                            let conn = pending.remove(p).stream;
                            let msg = format!(
                                "request body exceeds the {} MiB limit; send less per request",
                                BODY_CAP >> 20
                            );
                            let resp =
                                response_bytes(413, "text/plain", &[], msg.as_bytes());
                            if let Handled::Close(c) = finish(conn, resp) {
                                closing.push(c);
                            }
                            continue;
                        }
                        Parse::Bad => drop_pending = true,
                        Parse::Incomplete => {
                            drop_pending = eof || pending[p].buf.len() > REQ_MAX;
                        }
                    }
                }
                if drop_pending {
                    pending.remove(p);
                } else {
                    p += 1;
                }
            }
            // Drain responses still leaving: whatever each socket
            // accepts now, never more than the deadline allows.
            let mut c = 0;
            while c < closing.len() {
                let cc = &mut closing[c];
                let mut done = now >= cc.deadline;
                if !done {
                    while !cc.out.is_empty() {
                        match cc.stream.write(&cc.out) {
                            Ok(0) => break,
                            Ok(n) => {
                                cc.out.drain(..n);
                            }
                            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                            Err(ref e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                            Err(_) => {
                                cc.out.clear();
                                break;
                            }
                        }
                    }
                    done = cc.out.is_empty();
                }
                if done {
                    closing.remove(c);
                } else {
                    c += 1;
                }
            }
            // Poll live sockets: read available bytes, process complete
            // frames, drop closed connections. An open socket never
            // blocks the loop (§9.4's protocol depends on this).
            let mut i = 0;
            while i < ws_conns.len() {
                let mut drop_conn = false;
                let mut replies: Vec<(String, Vec<(String, String)>)> = Vec::new();
                loop {
                    match ws_conns[i].stream.read(&mut tmp) {
                        Ok(0) => {
                            drop_conn = true;
                            break;
                        }
                        Ok(n) => ws_conns[i].buf.extend_from_slice(&tmp[..n]),
                        Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                        Err(_) => {
                            drop_conn = true;
                            break;
                        }
                    }
                }
                while let Some((opcode, payload)) = ws_frame_from_buf(&mut ws_conns[i].buf) {
                    match opcode {
                        1 => {
                            if let Ok(text) = String::from_utf8(payload) {
                                // The shim's hello binds this socket to
                                // its page (§9.5); it needs no reply.
                                if let Some(V::Map(m)) = from_json(&text) {
                                    if let Some(V::Text(p)) = m.get("page") {
                                        ws_conns[i].page = Some(p.clone());
                                        continue;
                                    }
                                }
                                let headers = ws_conns[i].headers.clone();
                                let session = ws_conns[i].session.clone();
                                // Instances created while handling this
                                // event (re-renders) belong to the
                                // socket's page.
                                ev.current_page = ws_conns[i].page.clone();
                                let (reply, patches) =
                                    process_ws_text(&mut ev, &text, &headers, &session);
                                ev.current_page = None;
                                replies.push((reply, patches));
                            }
                        }
                        8 => {
                            drop_conn = true;
                        }
                        9 => {
                            ws_enqueue_frame(&mut ws_conns[i], 0x0A, &payload);
                        }
                        _ => {}
                    }
                }
                for (reply, patches) in replies {
                    ws_enqueue(&mut ws_conns[i], &reply);
                    // View patches broadcast to every OTHER live socket
                    // (§9.4: every view that read a changed property).
                    if !patches.is_empty() {
                        let msg = patches_json(&patches);
                        for (j, other) in ws_conns.iter_mut().enumerate() {
                            if j != i {
                                ws_enqueue(other, &msg);
                            }
                        }
                    }
                }
                if drop_conn {
                    // The socket's page unmounts with it: stop stacks run,
                    // then instances, handlers, dependency edges, and
                    // their channel subscriptions go (§9.5).
                    if let Some(page) = ws_conns[i].page.clone() {
                        ev.unmount_page(&page);
                    }
                    ws_conns.remove(i);
                } else {
                    i += 1;
                }
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
            // Drain spawned tasks (§9.7): they run between requests.
            while let Some(f) = {
                let next = if ev.spawn_queue.is_empty() { None } else { Some(ev.spawn_queue.remove(0)) };
                next
            } {
                if let Err(fault) = ev.call(f, vec![]) {
                    eprintln!("spawned task failed: {}", fault);
                }
            }
            // Instances dirtied outside an event (schedules, spawned
            // tasks) re-render and broadcast.
            if !ev.dirty_instances.is_empty() {
                let dirty: Vec<String> = std::mem::take(&mut ev.dirty_instances);
                let mut patches = Vec::new();
                for id in dirty {
                    if ev.instances.contains_key(&id) {
                        if let Ok(html) = render_instance(&mut ev, &id) {
                            patches.push((id, html));
                        }
                    }
                }
                if !patches.is_empty() {
                    let msg = patches_json(&patches);
                    for conn in ws_conns.iter_mut() {
                        ws_enqueue(conn, &msg);
                    }
                }
            }
            // Flush queued outbound frames. A peer that stopped draining
            // (a suspended laptop, a half-open socket) is dropped rather
            // than ever stalling the loop; its page unmounts like any
            // other close.
            let mut f = 0;
            while f < ws_conns.len() {
                if ws_flush(&mut ws_conns[f]) {
                    f += 1;
                } else {
                    if let Some(page) = ws_conns[f].page.clone() {
                        ev.unmount_page(&page);
                    }
                    ws_conns.remove(f);
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
    // Accounts persist alongside stored values (§9.6).
    let mut users = BTreeMap::new();
    for (email, (id, hash)) in &ev.users {
        let mut u = BTreeMap::new();
        u.insert("id".to_string(), V::Text(id.clone()));
        u.insert("hash".to_string(), V::Text(hash.clone()));
        users.insert(email.clone(), V::Map(u));
    }
    m.insert("__users".to_string(), V::Map(users));
    let _ = std::fs::write(path, to_json(&V::Map(m)));
}

/// Static file parts (§9.8): route is a prefix; `files` names a
/// directory under `assets/`. Returns the full response bytes when a
/// file part claims the path; the caller drains them without blocking.
fn try_serve_files(ev: &mut Evaluator, root: &std::path::Path, path: &str) -> Option<Vec<u8>> {
    for (full, cp) in ev.composed.iter() {
        let (Some(route_prop), Some(files_prop)) = (cp.props.get("route"), cp.props.get("files"))
        else {
            continue;
        };
        let text_of = |prop: &crate::resolved::ComposedProp| -> Option<String> {
            match &prop.value {
                MergedValue::Single(r) => ev.program.files[r.file_idx].ast.parts[r.part_idx]
                    .props[r.prop_idx]
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
            }
        };
        let (Some(prefix), Some(dir)) = (text_of(route_prop), text_of(files_prop)) else {
            continue;
        };
        let Some(rest) = path.strip_prefix(prefix.trim_end_matches('/')) else {
            continue;
        };
        let rest = rest.trim_start_matches('/');
        if rest.is_empty() || rest.split('/').any(|s| s == "..") {
            return Some(response_bytes(404, "text/plain", &[], b"not found"));
        }
        let file = root.join("assets").join(&dir).join(rest);
        let resp = match std::fs::read(&file) {
            Ok(bytes) => {
                let ct = match file.extension().and_then(|e| e.to_str()) {
                    Some("html") => "text/html",
                    Some("css") => "text/css",
                    Some("js") => "text/javascript",
                    Some("json") => "application/json",
                    Some("png") => "image/png",
                    Some("svg") => "image/svg+xml",
                    _ => "application/octet-stream",
                };
                response_bytes(200, ct, &[], &bytes)
            }
            Err(_) => response_bytes(404, "text/plain", &[], b"not found"),
        };
        let _ = full;
        return Some(resp);
    }
    None
}

fn session_from_headers(headers: &BTreeMap<String, String>) -> Option<String> {
    headers.get("cookie").and_then(|c| {
        c.split(';').find_map(|kv| {
            let kv = kv.trim();
            kv.strip_prefix("ashsession=").map(|v| v.to_string())
        })
    })
}

/// A live WebSocket connection multiplexed on the event loop: the loop
/// polls its buffer for complete frames and drains its outbound queue as
/// the peer accepts bytes, so an open socket never blocks other
/// requests, schedules, or reloads — in either direction.
pub struct WsConn {
    pub stream: TcpStream,
    pub buf: Vec<u8>,
    /// Outbound bytes not yet accepted by the socket; `ws_flush` drains
    /// this every tick and sheds the peer when it stops draining.
    pub out: Vec<u8>,
    /// Last moment the peer accepted bytes (or the queue was empty).
    /// A queue that stays untouched past `WS_STALL` marks a dead peer;
    /// judging by time-without-progress, not queue size, keeps a healthy
    /// client that just received one huge patch from being shed.
    pub last_drain: std::time::Instant,
    pub session: Option<String>,
    pub headers: BTreeMap<String, String>,
    /// The page render this socket belongs to (the shim announces it on
    /// open); its instances unmount when the socket closes (§9.5).
    pub page: Option<String>,
}

/// An accepted socket that has not yet delivered a complete request.
/// The event loop reads it without blocking; one that stops making
/// progress is dropped at its deadline instead of stalling everyone.
struct PendingConn {
    stream: TcpStream,
    buf: Vec<u8>,
    deadline: std::time::Instant,
}

/// A socket whose response is still draining. The loop pushes the rest
/// out as the peer accepts bytes, then closes; a peer that trickles
/// (or stops) is cut at the deadline. This is what keeps a slow reader
/// downloading a big file from ever pausing the runtime.
struct CloseConn {
    stream: TcpStream,
    out: Vec<u8>,
    deadline: std::time::Instant,
}

/// What one complete request turned into.
enum Handled {
    /// An upgraded live socket for the loop to poll.
    Ws(WsConn),
    /// A response that did not fit the socket buffer; drain and close.
    Close(CloseConn),
    /// Fully answered (or unanswerable); the socket closes now.
    Done,
}

/// Push a finished response at the socket without blocking: whatever
/// the kernel accepts now goes out now, and the remainder — if any —
/// parks on the closing list.
fn finish(mut conn: TcpStream, bytes: Vec<u8>) -> Handled {
    let mut off = 0usize;
    while off < bytes.len() {
        match conn.write(&bytes[off..]) {
            Ok(0) => return Handled::Done,
            Ok(n) => off += n,
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                return Handled::Close(CloseConn {
                    stream: conn,
                    out: bytes[off..].to_vec(),
                    deadline: std::time::Instant::now() + std::time::Duration::from_secs(30),
                });
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(_) => return Handled::Done,
        }
    }
    Handled::Done
}

/// Serve one complete request. An HTTP request is answered in place; a
/// WebSocket upgrade completes its handshake and returns the connection
/// for the event loop to poll.
fn handle_request(
    ev: &mut Evaluator,
    root: &std::path::Path,
    mut conn: TcpStream,
    req: HttpRequest,
) -> Handled {
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
        // A fresh socket's send buffer is empty; this small head cannot
        // block or short-write.
        let _ = conn.write_all(head.as_bytes());
        return Handled::Ws(WsConn {
            session: session_from_headers(&req.headers),
            headers: req.headers,
            stream: conn,
            buf: Vec::new(),
            out: Vec::new(),
            last_drain: std::time::Instant::now(),
            page: None,
        });
    }

    // Every HTTP dispatch is a page context: instances created while
    // handling it (el in the route handler, nested views in the render)
    // belong to this page and unmount when its socket closes (§9.5).
    ev.begin_page();

    // The declared stylesheet (§9.4) serves ahead of everything: its
    // path is `/<name>.css`, resolved to `assets/<name>.css`, and the
    // document head links exactly this.
    if req.method == "GET" {
        if let Some(name) = ev.style_name.clone() {
            if req.path == format!("/{}.css", name) {
                let file = root.join("assets").join(format!("{}.css", name));
                let resp = match std::fs::read(&file) {
                    Ok(bytes) => response_bytes(200, "text/css", &[], &bytes),
                    Err(_) => response_bytes(404, "text/plain", &[], b"not found"),
                };
                return finish(conn, resp);
            }
        }
    }

    // Static files (§9.8) match by route prefix before dynamic dispatch.
    if req.method == "GET" {
        if let Some(resp) = try_serve_files(ev, root, &req.path) {
            return finish(conn, resp);
        }
    }

    // §9.2: `data` is the decoded JSON or form body, `none` when absent.
    let form = req
        .headers
        .get("content-type")
        .map(|c| c.starts_with("application/x-www-form-urlencoded"))
        .unwrap_or(false);
    let data = if req.body.is_empty() {
        V::None
    } else if form {
        String::from_utf8(req.body.clone())
            .ok()
            .map(|s| decode_form(&s))
            .unwrap_or(V::None)
    } else {
        String::from_utf8(req.body.clone())
            .ok()
            .and_then(|s| from_json(&s))
            .unwrap_or(V::None)
    };
    // Session context in, cookie intent out (§9.6).
    ev.current_session = session_from_headers(&req.headers);
    ev.pending_cookie = None;
    let resp = match dispatch(ev, &req.method, &req.path, data, &req.headers) {
        Ok(v) => match render_response(ev, &v) {
            Ok((status, ct, body, mut extra)) => {
                if let Some(tok) = ev.pending_cookie.take() {
                    let cookie = if tok.is_empty() {
                        "ashsession=; Path=/; Max-Age=0".to_string()
                    } else {
                        format!("ashsession={}; Path=/; HttpOnly", tok)
                    };
                    extra.push(("set-cookie".to_string(), cookie));
                }
                response_bytes(status, &ct, &extra, &body)
            }
            Err(f) => response_bytes(f.status, "text/plain", &[], f.message.as_bytes()),
        },
        Err(f) => {
            let body = format!(
                "{{\"error\":{}}}",
                {
                    let mut s = String::new();
                    crate::diag::push_json_str(&mut s, &f.message);
                    s
                }
            );
            response_bytes(f.status, "application/json", &[], body.as_bytes())
        }
    };
    finish(conn, resp)
}

/// Process one text envelope from a live socket; the reply JSON goes
/// back on the same connection, and view patches broadcast to everyone.
fn process_ws_text(
    ev: &mut Evaluator,
    text: &str,
    headers: &BTreeMap<String, String>,
    session: &Option<String>,
) -> (String, Vec<(String, String)>) {
    ev.current_session = session.clone();
    let envelope = from_json(text).unwrap_or(V::None);
    if let V::Map(m) = &envelope {
        if let Some(V::Map(e)) = m.get("event") {
            let instance = e.get("instance").map(to_text).unwrap_or_default();
            let hid = e.get("h").map(to_text).unwrap_or_default();
            let name = e.get("name").map(to_text).unwrap_or_default();
            let value = e.get("value").cloned().unwrap_or(V::None);
            return match dispatch_event(ev, &instance, &hid, &name, value) {
                Ok(patches) => (patches_json(&patches), patches),
                Err(f) => {
                    let mut m = BTreeMap::new();
                    m.insert("status".to_string(), V::Number(f.status as f64));
                    m.insert("error".to_string(), V::Text(f.message));
                    (to_json(&V::Map(m)), Vec::new())
                }
            };
        }
    }
    let (path, data, method) = match &envelope {
        V::Map(m) => (
            m.get("path").map(to_text).unwrap_or_default(),
            m.get("data").cloned().unwrap_or(V::None),
            m.get("method").map(to_text).unwrap_or_else(|| "get".to_string()),
        ),
        _ => (String::new(), V::None, "get".to_string()),
    };
    let reply = match dispatch(ev, &method, &path, data, headers) {
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
    (to_json(&reply), Vec::new())
}

fn patches_json(patches: &[(String, String)]) -> String {
    let list = V::List(
        patches
            .iter()
            .map(|(id, html)| {
                let mut p = BTreeMap::new();
                p.insert("instance".to_string(), V::Text(id.clone()));
                p.insert("html".to_string(), V::Text(html.clone()));
                V::Map(p)
            })
            .collect(),
    );
    let mut m = BTreeMap::new();
    m.insert("patches".to_string(), list);
    to_json(&V::Map(m))
}

/// How long a non-empty outbound queue may sit with the peer accepting
/// nothing before the peer is judged dead and shed. Time without
/// progress — not queue size — is the test, so one huge patch to a
/// healthy client never gets it disconnected.
const WS_STALL: std::time::Duration = std::time::Duration::from_secs(5);

/// Absolute outbound queue bound: a runaway backlog is shed at once so
/// one dead peer can never hold this much of the server's memory.
const WS_OUT_MAX: usize = 64 << 20;

/// Frame a payload and queue it on `conn`; bytes leave in `ws_flush`.
/// Queueing never blocks, so a broadcast to a stalled peer costs the
/// loop nothing — the old direct write here retried a full socket
/// forever, freezing every client because one stopped reading.
fn ws_enqueue_frame(conn: &mut WsConn, opcode: u8, payload: &[u8]) {
    conn.out.push(0x80 | opcode);
    let len = payload.len();
    if len < 126 {
        conn.out.push(len as u8);
    } else if len < 1 << 16 {
        conn.out.push(126);
        conn.out.extend_from_slice(&(len as u16).to_be_bytes());
    } else {
        conn.out.push(127);
        conn.out.extend_from_slice(&(len as u64).to_be_bytes());
    }
    conn.out.extend_from_slice(payload);
}

fn ws_enqueue(conn: &mut WsConn, text: &str) {
    ws_enqueue_frame(conn, 0x01, text.as_bytes());
}

/// Write as much queued outbound data as the socket accepts right now.
/// `false` means the connection is dead — closed, stalled past
/// `WS_STALL` with bytes waiting, or `WS_OUT_MAX` behind — drop it.
fn ws_flush(conn: &mut WsConn) -> bool {
    while !conn.out.is_empty() {
        match conn.stream.write(&conn.out) {
            Ok(0) => return false,
            Ok(n) => {
                conn.out.drain(..n);
                conn.last_drain = std::time::Instant::now();
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
            Err(ref e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(_) => return false,
        }
    }
    if conn.out.is_empty() {
        conn.last_drain = std::time::Instant::now();
        return true;
    }
    conn.out.len() <= WS_OUT_MAX && conn.last_drain.elapsed() < WS_STALL
}
