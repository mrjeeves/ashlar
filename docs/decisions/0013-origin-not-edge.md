# ADR-0013: Ashlar is an origin; TLS and modern HTTP live at the edge

Date: 2026-07-23

Status: accepted

## Context

The runtime is a single zero-dependency binary speaking hand-rolled
HTTP/1.1 on one event loop. Real deployments want TLS, and often want
HTTP/2 or HTTP/3 for browsers. The question is whether any of that
belongs *in* the binary, and G1's single-binary / zero-dependency rule
plus the "only `unsafe` is the dlopen boundary" discipline sharply
constrain the answer.

## Decision

**The Ashlar binary is an origin server. TLS, HTTP/2, HTTP/3, and QUIC
are terminated at a reverse proxy in front of it (nginx, Caddy, a cloud
load balancer), which speaks the modern protocols to browsers and plain
HTTP/1.1 to the origin.** The binary grows only the small, correct
pieces needed to sit behind such a proxy honestly.

Why not in the binary:

- **TLS.** Hand-rolling TLS 1.3, X.509, and the AEAD/curve primitives
  under a zero-crate rule would be a security catastrophe — exactly the
  crypto one must never hand-roll, and against the spirit of confining
  `unsafe` to one audited boundary. (The auth hashing the runtime *does*
  hand-roll — PBKDF2-HMAC-SHA1 — is a bounded, testable primitive; a full
  TLS stack is not.)
- **HTTP/2 / HTTP/3 / QUIC.** These are best spoken at the edge anyway:
  the proxy gives browsers h2/h3 while the origin stays HTTP/1.1. The win
  in-binary would be small — an Ashlar page is one HTML document, one
  stylesheet, and one long-lived WebSocket, so h2 multiplexing buys
  little and h3's loss-resilience is a mobile nicety, not a need — and
  the cost (HPACK, QUIC, congestion control, a second TLS stack) is
  enormous.

What the binary does carry, so it sits behind a proxy correctly:

- **`X-Forwarded-Proto` awareness → `Secure` cookies.** The origin sees
  plain HTTP even when the browser is on HTTPS; the proxy reports the
  real scheme in `X-Forwarded-Proto`. The session cookie is `HttpOnly`
  and `SameSite=Lax` always, and gains `Secure` when that header says
  `https`, so it never rides a plaintext hop (reference §9.6).
- **Atomic state writes.** `stored` state flushes to a sibling temp file
  that is then renamed over the live one — atomic on a single
  filesystem — so a crash mid-flush leaves the whole old file or the
  whole new one, never a truncated `.ashlar-state.json`.

## Consequences

- The origin stays tiny, zero-dependency, and free of the largest
  correctness-and-security surface a server can have, while apps still
  get HTTPS and modern HTTP in production. "Latest and most compatible"
  is achieved at the edge, which is where it belongs.
- The embedded JSON key-value store (ADR-0007, name-keyed `stored`
  values) remains deliberate, not a placeholder: it is the simplest
  thing that satisfies persistence under zero-deps. A log-structured or
  embedded store is warranted only if a real durability-at-scale
  requirement appears, and would be hand-rolled in-tree if so — recorded
  here as a known, bounded frontier rather than built ahead of need.
- Presentation and computation already cross named boundaries out of the
  language (a stylesheet, ADR-0010; `foreign`, §9.10). Transport security
  is the same shape one level down: a boundary the deployment owns, with
  the origin naming just enough (the forwarded scheme) to stay correct
  across it.
