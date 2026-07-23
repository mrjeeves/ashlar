# Examples

Each directory is a complete Ashlar project: run one with
`ashlar run examples/<name>` and open `http://127.0.0.1:8080`. Every
example is held to two depths by `crates/ashlar/tests/t_examples.rs`:
it must compile with zero diagnostics in canonical format, AND it is
served on a real port and driven through its HTTP/WebSocket surface on
every test run. If it's here, it builds — and it works.

## hello

The smallest server: one part declares the `port`, one part owns a
route. Two parts, no ceremony.

## counter

The live view protocol (§9.4) in one file: a `view` part with
per-instance `state`, instantiated with `el`, its `onclick` handler
running server-side over the built-in socket. The browser runs no
program code — open two windows and click.

## todo

Forms over the socket: `oninput` mirrors the field into per-instance
state (`e.data.value`), `onsubmit` commits it, and the patched HTML
comes back down the same socket. The whole app is one view part.

## chat

The composition story in four files:

- `data.ash` — a data shape (`Message`), a `stored` map that survives
  restarts, and a `pipe` property (`prepare`).
- `api.ash` — routes, a `start stack`, JSON request handling, and the
  §9.6 auth builtins (`signup`/`login`).
- `audit.ash` — a separate space LAYERS the store and the app: its
  `prepare` pipe layer runs after the base's (use order is composition
  order), and its `start` stack joins the boot sequence. No base file
  was edited.
- `ui.ash` — the full interface: a compose form (name + message over
  `oninput`/`onsubmit`), a feed sorted by send time, and a live counter.
  Any post — this client's form, another client's, or the HTTP API —
  re-renders every connected feed (§9.3 reactivity). The suite drives
  it with two concurrent socket clients.

## diary

Sessions end to end (§9.6): signup/login/logout routes, the `allow`
guard turning anonymous requests into 403s before `handle` runs, and
`req.user!` proven safe inside the guard. The test drives the full
lifecycle including the server-side session ending on logout.

## press

All the merge kinds in one part, layered from a second space without
editing the first (§4): `append` joins the tag lists, `deep` merges the
limit maps one level, `pipe` chains the render base-first, and paired
`stack` / `stack reverse` properties boot in use order and tear down
derived-first.

## ticker

Server-driven reactivity (§9.7 + §9.3): a scheduled part's `run` bumps
a `synced` counter on an `every` interval, and every connected view
that read it re-renders — no user event anywhere in the loop.
