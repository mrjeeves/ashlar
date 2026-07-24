# Examples

Each directory is a complete Ashlar project: run one with
`ashlar run examples/<name>` and open `http://127.0.0.1:8080`. Every
example is held to two depths by `crates/ashlar/tests/t_examples.rs`:
it must compile with zero diagnostics in canonical format, AND it is
served on a real port and driven through its HTTP/WebSocket surface on
every test run. If it's here, it builds — and it works.

Every example wears the same restrained dark skin — one house palette,
declared per project as `assets/<name>.css` and bound by `class` name
(§9.4, ADR-0016). To flip through them all at once, run `./showcase/serve.sh`
(it starts each example on its own port) and open `showcase/index.html`:
a sidebar of all fourteen with live frames you can swap with a click or the
arrow keys.

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
lifecycle including the server-side session ending on logout. The `/` page
is a login gate for visitors and a private reader for members — the request
identity crossing into the view.

## press

All the merge kinds in one part, layered from a second space without
editing the first (§4): `append` joins the tag lists, `deep` merges the
limit maps one level, `pipe` chains the render base-first, and paired
`stack` / `stack reverse` properties boot in use order and tear down
derived-first. The `/` page is a live window onto that composed pipe: type
text and the output — base first, then the markdown layer — updates as you go.

## poll

Channels (§9.5), placed honestly: votes are `stored` state, so
reactivity alone keeps every tally live — the channel carries what
state doesn't, the ephemeral "last vote" ticker. Each board instance
subscribes in its `start stack` (the subscription dies with the
instance) and keeps a per-instance `latest`: a fresh page joins at
"none yet" no matter how many votes came before it. The test proves
the push arrives through the channel alone — an HTTP vote patches a
connected view whose `latest` no code in that request assigns.

## ticker

Server-driven reactivity (§9.7 + §9.3): a scheduled part's `run` bumps
a `state` counter on an `every` interval, and every connected view
that read it re-renders — no user event anywhere in the loop. The page
shows the beat count as a live, ticking number.

## pong

A real-time game with zero client code: a 20fps `every` schedule advances
the ball server-side, sliders steer the paddles over `oninput`, and both
players' pages re-render from the same shared `state`. Each control is
its own view instance, so the field's twenty-patches-a-second never
replace a slider mid-drag. The play field is placed with inline geometry
(those pixel coordinates are game state, not appearance); the chrome
around it is class-bound. Open it in two windows and play.

## foundry

Background work joined directly to a live interface (§9.7 + §9.4). A
POST queues a brief and returns while it is still waiting; `spawn` runs
the worker between requests, and the worker's state change patches every
connected board. The API, worker, and UI coordinate through one named
part, with no client application code or job-runner dependency. The board
carries a compose form, so you can queue a brief from the page and watch
it finish, live.

## guardrails

A typed policy pipeline assembled by the use graph. The core space owns
the route and `Decision` shape; two other spaces independently layer
length and content checks onto `Gate.review`. Their order is declared by
`use`, every layer must preserve the pipe's shape, and neither policy
edits the core or the other policy — the composition model applied to
work that separate agents can safely own. The `/` page runs the whole
composed policy live: type a message and the verdict — allowed, or blocked
with each layer's reason — decides as you type.

## commons

The flagship: a complete team chat that exercises the whole language as
one product. Native-form signup and login set a session cookie with zero
client code (§9.6); the request identity crosses into the views as `el`
fields. Rooms live at their own URLs, messages post live over the socket
and re-render every viewer's feed (§9.4), and **presence** is driven by
the view lifecycle — a page mounting arrives, its socket closing departs
(§9.5), so the online list is live with no heartbeat. Two independently
owned spaces layer the shared store without editing it: `commons.moderation`
redacts on the `prepare` seam, `commons.mentions` scans on the `announce`
seam and pings mentioned people over a per-user channel the notice tray
subscribes to by name. Appearance is bound by name: the root declares
`style = "commons"`, and the views carry `class` names that meet the
served `assets/commons.css` by name — no style string anywhere (§9.4).

## ledger

The datastore is a real **SQLite database file**, reached across the
`foreign` boundary (§9.10) — the one example that leaves the language for
its data. `data.ash` names the operations (`record`, `recent`, `total`)
and shape-checks every returned row against the `Entry` data shape; the
SQL lives entirely in `foreign/ledger.store.rs`, a std-only Rust `cdylib`
that links the system `libsqlite3` over the C ABI. SQL is the persistence
peer of CSS: **named in Ashlar, defined outside it** — no query string and
no connection string ever appears in source (B5; the shim reads
`ASHLAR_LEDGER_DB`, a deployment fact). The board reads the ledger with
`recent` and `total` — both declared `reads Entry` — while `record` is
`writes Entry`, so the SQLite store is **reactive** (§9.3): an entry added in
one window (over the socket, or through the `/add` API) patches every open
board live, running total and all, with no reload. The total is a SQL `SUM`
in the shim, so the same `foreign` boundary that runs a fetch also carries a
live database. Build the shim before running:

```
rustc --edition 2021 --crate-name ledger_store --crate-type cdylib \
  -l sqlite3 -o examples/ledger/foreign/ledger.store.so \
  examples/ledger/foreign/ledger.store.rs
```

The driving test builds it automatically and skips loudly where a Rust
toolchain or `libsqlite3` is absent — a SQLite integration cannot be tested
without SQLite — and it drives the reactive path too: a board holding only a
socket is patched live by another client's write. This delivers reactivity
for a foreign store (ADR-0014); a `stored` database backend and a Postgres
client remain the proposed next stages.

## locker

Per-user storage in one keyword (ADR-0015). `owned stored notes` on a
singleton gives every signed-in user their OWN list, saved to disk and
isolated from everyone else's — no keying by user id anywhere, and no way
to reach another user's data. `owned` has no meaning without a user, so the
routes guard with `allow`; an anonymous read would fault, never fall
through to a shared value. The test signs up two people, has each keep a
note, and proves each sees only their own — then restarts the server and
logs back in to show the notes persisted, still isolated, keyed by the
stable account id. The `/` page is a gated board: sign in and keep notes,
each user seeing only their own — the owned read rendering right in the view.
