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

## The second edge (2026-07-27)

A proxy is not the only thing that can sit between a browser and this
origin. A **private mesh** is the other: the machine's mesh daemon joins a
network only its members can see, publishes a local port to them, and
proxies their connections in. `ashlar run --mesh` asks it to publish the
port the origin is already serving.

The decision above is unchanged, and that is the point — the binary does
not grow a mesh any more than it grew TLS. It does not speak the mesh's
protocol, hold its keys, or link its code (G1); it names a capability and
the machine's daemon answers, across the boundary ADR-0017 already built.
Two consequences follow from *this* ADR rather than that one:

- **The origin stays an origin.** A published site is reachable through
  loopback, exactly as it is behind nginx. Nothing about a request changes
  because it arrived over a mesh, including the `X-Forwarded-Proto` rule —
  a mesh hop is not TLS, and a site that needs `Secure` cookies still needs
  a terminating proxy in front.
- **Which edge is in force is deployment's.** `--mesh` is `--port`'s
  sibling: one says where this origin listens, the other says who can reach
  it, and neither is written in source (B5). A program cannot tell, and
  must not try.

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
