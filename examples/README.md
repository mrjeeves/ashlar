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
(it starts each example on its own port) and open <http://127.0.0.1:8080> —
`gallery`, below, which frames the other sixteen, all of them live at once.

Most of them are worth opening **twice**. Almost every one now has something
in it that two windows share, because a language whose whole runtime story is
"the server holds the state and the page follows it" is not demonstrated by a
page that only ever talks to itself.

## enclave

**The chat.** A room for the people who hold this program's mesh id, and
nobody else. No server in the middle, no account, no address anyone wrote
down: the id baked into the build IS the invite, so distributing the program
distributes the key, and a group changes its locks by rolling a new one.

The whole app is one element. `enclave.ash` is a `port`, a `start` that joins
the mesh, one setting layered to take its own room rather than the shared
area, and:

```ash
el(mesh.room, {})
```

Everything inside that is the vendored `mesh` space, and this file says none
of it: who is in the room down one side with their presence live off the
roster, what is on the shelf under them, and the conversation down the other.
Your own words sit on the other side of the thread and read as yours; a run of
lines from one person says their name once; arrivals are notices the room works
out for itself from the roster, so they cost no traffic and no two members can
disagree about who was there; a file appears where it was put, with its size
and a button that fetches it; the first line said since you opened the page is
marked; and a filter narrows the conversation without sending anything
anywhere. What was said survives the site restarting.

The conversation **scrolls inside its own pane** — the page does not grow and
the composer stays where you left it. That is `min-height: 0` on the panes
that scroll, because a flex or grid item defaults to being as tall as its
contents and will happily push a composer off the bottom of the screen; and
the pane is fed newest-first and turned upside down by the stylesheet, which
is what keeps a chat pinned to its newest line with no client code at all.

Putting a file in the room is a **file picker and a drop zone**, in the shelf
where the files are, with an × on each of your own to take it back down. The
form is a native post (§9.2): no handler, no socket, no client application
code — the browser sends the file, the runtime writes it under the project's
runtime state and hands the route `{name, size, path}`, and a path is what the
mesh's `offer` already took. Dropping a file on the form does the same thing,
because the shim treats a drop on a form with a file input as choosing one; the
picker works with the shim switched off, which is why the drop is the part that
lives there.

It was `/share <path>` for one round — a command line wearing a chat's clothes,
on the excuse that a browser cannot hand a server a path it can open. True, and
beside the point: a picker hands over the file. What was actually missing was
`multipart/form-data` in the runtime (ADR-0026).

None of that is the language. The `mesh` space is `foreign` (§9.10) and derives
to `ashlar mesh worker`, which drives the control socket the mesh node already
exposes to its own clients — so a project that wants a room writes no binding,
and the mesh ships nothing on Ashlar's behalf. Nothing polls, in the browser or
in the program: the node streams presence and messages to its own clients, the
worker is one of them, and a push re-renders every view that read what moved.

On a machine with no mesh node the site still serves, and the room's header
says why instead of showing an empty room that looks merely quiet. Its driving
test stands up the node's own control socket and lets the shipped worker drive
it, so what is faked is the network and nothing else; a second test runs it
against a socket that is not there at all. What two machines would prove is
open in `docs/roadmap.md`, not implied here.

## gallery

The showcase itself, and the reason `setting` exists. It renders every other
example as a LIVE frame — sixteen addresses, all running — and its source
contains not one of them. `Catalog` declares `setting lead: Site` and
`setting groups: [Group]`; deployment fills both in from `settings.json`, and
starting without them refuses by name rather than serving dead frames (§9.12,
ADR-0020). `enclave` leads on the stage because the settings say so; clicking
any tile's name promotes it there instead, which is per-instance state patched
over the socket — the page never reloads and the frames under it keep whatever
they were showing. The tile handlers are inline functions in an element's
attrs, closing over the mapped `Site`.

## hello

The smallest server, and the smallest thing worth two windows. `port` on one
part, a route answering plain text on another — and a page that says how many
people have it open, which costs one shared `state` and the view lifecycle
(`start` on mount, `stop` when the socket goes). No heartbeat, no polling, and
the first window is told about the second without asking.

## counter

The two scopes a `state` property can have, side by side — the one thing about
§9.3 worth learning first, and invisible in a single window. The left counter
is `state` on a part instantiated with `el`, so it is per-instance: yours. The
right one is the same keyword on a singleton, so it is the program's: press it
and the number moves in every other window at once. One word apart, and the
driving test proves the difference by asserting what is NOT in the patch.

## todo

One shared list, `stored` on disk, live in every window on it. `oninput`
mirrors the field into per-instance state and `onsubmit` commits it; ticking an
item, dropping one, and clearing the done ones all run server-side and patch
everybody. It also counts who is looking, off the same view lifecycle `hello`
uses. Values are immutable, so a tick is a new list with a new item in its
place — the spread is the whole of it.

## diary

Sessions end to end (§9.6): signup/login/logout routes, the `allow` guard
turning anonymous requests into 403s before `handle` runs, and `req.user!`
proven safe inside the guard. The `/` page is a login gate for visitors and a
private reader for members — the request identity crossing into the view. Once
inside, that identity is the interesting part: the page shows who ELSE is
signed in right now (reference-counted off the view lifecycle, so closing a tab
removes you) and carries a book anyone signed in can leave a line in, signed
with the email the session proved. The test drives the full lifecycle including
the server-side session ending on logout.

## press

All the merge kinds in one part, layered from a second space without editing
the first (§4): `append` joins the tag lists, `deep` merges the limit maps one
level, `pipe` chains the render base-first, and paired `stack` / `stack
reverse` properties boot in use order and tear down derived-first. The `/` page
is a live window onto the COMPOSED part: your text on the left, what the whole
pipe made of it on the right, and under them the merged values read straight
back out — the tags both spaces contributed, and the limits map `deep` filled
from two spaces that never mentioned each other.

## poll

Channels (§9.5), placed honestly: votes are `stored` state, so reactivity alone
keeps every tally live — the channel carries what state doesn't, the ephemeral
"last vote" ticker. Each board instance subscribes in its `start stack` (the
subscription dies with the instance) and keeps a per-instance `latest`: a fresh
page joins at "none yet" no matter how many votes came before it. The ballot is
stored too, so anybody can put another stone on it and every open page grows
the row. Each option is a bar whose fill width is the number itself — data, not
appearance, the same call pong makes for its ball. The test proves the push
arrives through the channel alone: an HTTP vote patches a connected view whose
`latest` no code in that request assigns.

## ticker

Server-driven reactivity (§9.7 + §9.3): a scheduled part's `run` bumps a
`state` counter on an `every` interval, and every connected view that read it
re-renders — no user event anywhere in the loop. Beside it, the same shared
property written by a person instead: mark a beat and the mark appears on
everybody's page. One kind of state, two writers, and the views cannot tell
which one moved it.

## pong

A real-time game with zero client code: a 20fps `every` schedule advances
the ball server-side, sliders steer the paddles over `oninput`, and both
players' pages re-render from the same shared `state`. Each control is
its own view instance, so the field's twenty-patches-a-second never
replace a slider mid-drag. The play field is placed with inline geometry
(those pixel coordinates are game state, not appearance); the chrome
around it is class-bound. Open it in two windows and play.

## foundry

Background work joined directly to a live interface (§9.7 + §9.4). A POST
queues a brief and returns while it is still waiting; `spawn` runs the worker
between requests, and the worker's state change patches every connected board.
The API, worker, and UI coordinate through one named part, with no client
application code and no job-runner dependency. Because a queue that drains
instantly is a queue you cannot see, anyone can hold the line — briefs then
pile up in the waiting lane on every open board, and can be called off there —
until somebody releases it and the whole queue drains, live, in front of both
of you.

## guardrails

A typed policy pipeline assembled by the use graph. The core space owns the
route and `Decision` shape; two other spaces independently layer length and
content checks onto `Gate.review`. Their order is declared by `use`, every
layer must preserve the pipe's shape, and neither policy edits the core or the
other policy — the composition model applied to work that separate agents can
safely own. The `/` page runs the whole composed policy live: type a message
and the verdict — allowed, or blocked with each layer's reason — decides as you
type. Submitting it puts the decision in a shared log every open page sees, so
what the gate did for somebody else is visible; the HTTP route writes to the
same log, which is how an API call lands on a page nobody touched.

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

## slate

A shared pad. Open a URL, type, and whoever else has it open sees the text
as you write it. No account, no session, no invite — **the URL is the
permission**, which is what makes a pad a pad rather than a document with
a sharing dialog.

It is here for one problem, and everything else in it is ordinary: **two
people typing at once.** The browser shim sends an event carrying the
field's whole value, so the server never sees "insert `x` at offset 40" —
it sees "this page says the pad now reads THIS". Take that at face value
and the slower typist's snapshot lands on top of the faster one's, and a
paragraph is gone with no error anywhere. That is the failure this example
refuses.

So each page keeps `base` — the text it had in front of it — as
per-instance state, an edit is `(base, incoming)` against the pad's
`current`, and the merge is three-way, line by line. Two people editing
different lines both keep their work; two people editing the SAME line is
a real conflict, where the copy already on the pad wins and the writer who
lost is **told** — a shared editor may drop an edit, but never in silence.
When any edit lands the pad publishes its new text on a channel every open
editor subscribes to (§9.5), so a page's `base` moves with its screen; and
a line matching what the pad held one version ago is read as a page that
has not caught up rather than as an opinion, so a fast typist cannot
silently undo a colleague. The trade that bound implies is stated in
[ADR-0028](../docs/decisions/0028-what-a-snapshot-transport-can-merge.md).

The browser runs no editor code: no CRDT library, no operational transform
shipped to the client, no diffing in JavaScript. The merge is about forty
lines of Ashlar on the server. Around it: presence off the socket lifecycle
(arrive on mount, depart when the socket closes), revisions snapshotted on
a rhythm and restorable — where restoring is an edit like any other, so it
merges and nobody mid-sentence loses their line — and two spaces layering
the edit seam, `slate.limits` for size and `slate.history` for snapshots,
ordered by the `use` between them.

## ledger

The datastore is a real **SQLite database file**, reached across the `foreign`
boundary (§9.10) — the one example that leaves the language for its data.
`data.ash` names the operations (`record`, `recent`, `total`) and shape-checks
every returned row against the `Entry` data shape; the SQL lives outside
Ashlar. SQL is the persistence peer of CSS: **named in Ashlar, defined outside
it** — no query string and no connection string ever appears in source (B5).
The board reads the ledger with `recent` and `total` — both declared
`watches Entry` — while `record` is `updates Entry`, so the SQLite store is
**reactive** (§9.3): an entry added in one window (over the socket, or through
the `/add` API) patches every open board live, running total and all, with no
reload. The total is a SQL `SUM` on the far side, so the same `foreign`
boundary that runs a fetch also carries a live database.

It ships that capability **twice**, and which one answers is deployment's
(ADR-0017 §5e):

- `foreign/ledger.store.rs` — a std-only Rust `cdylib` linking the system
  `libsqlite3` over the C ABI. This is the corpus site the `native` transport
  exists to defend, and the default on a machine that can build it.
- `foreign/ledger.store.py` — the same three operations, the same SQL, over
  `worker`, using the SQLite already in Python's standard library. No compiler,
  no development package, no dynamic loader.

Nothing in `data.ash` or the views knows which one answered, and the driving
suite runs the example both ways with the same assertions — including the
reactive patch, because reactivity belongs to the boundary and not to a
transport. `./showcase/serve.sh` builds the shim where it can and binds the
worker where it cannot (Windows has no POSIX loader; some machines have no
`rustc`; some have `libsqlite3` but not its development package), printing
which is in force and why.

To build the shim by hand you need the development package, because it *links*
SQLite rather than bundling it — the runtime library most systems already ship
is not enough for `-l sqlite3`:

```
sudo apt install libsqlite3-dev      # Debian/Ubuntu (incl. WSL)
sudo dnf install sqlite-devel        # Fedora/RHEL
sudo pacman -S sqlite                # Arch
# macOS: ships with the Xcode command line tools
```

```
rustc --edition 2021 --crate-name ledger_store --crate-type cdylib \
  -l sqlite3 -o examples/ledger/foreign/ledger.store.so \
  examples/ledger/foreign/ledger.store.rs
```

`ashlar foreign check examples/ledger` confirms the result either way, and says
which binding it proved. This delivers reactivity for a foreign store
(ADR-0014); a `stored` database backend and a Postgres client remain the
proposed next stages.

## abacus

The foreign boundary with **no compiler anywhere** (ADR-0017). `summarize` is
declared in Ashlar and implemented in ten lines of Python, reached over the
`worker` transport — a co-process speaking JSON Lines on stdin/stdout, bound in
`foreign.json`, never in source. There is no shared library, no C ABI, and no
build step: the whole contract is "read a JSON object per line, answer with
one." The answer is still shape-checked against the `Summary` data shape at the
boundary, so a drifting worker faults at the call site rather than leaking bad
data. The page has two of them: a scratch line that is yours alone, and a bench
anybody can add a number to — the worker is a co-process for the PROGRAM, not
for a page, so a number added in one window re-runs it and patches the figures
in all of them. `ashlar foreign check` proves the worker speaks the protocol
before any request does. Needs `python3`; the driving test skips loudly without
it.

## locker

Per-user storage in one keyword (ADR-0015, spelled by ADR-0019), shown against
the thing it is not. `peruser stored notes` on a singleton gives every
signed-in user their OWN list, saved to disk and isolated from everyone else's
— no keying by user id anywhere, and no way to reach another user's data. Right
beside it, one word shorter, `stored shelf` is one list for everybody. The page
puts them in two columns: the left is yours, the right moves in every window on
the site, and a note can be pushed from the left to the right but never the
other way — because nothing can read another person's locker, not even to copy
out of it. `peruser` has no meaning without a user, so the routes guard with
`allow`; an anonymous read would fault, never fall through to a shared value.
The test signs up two people, has each keep a note, and proves each sees only
their own — then restarts the server and logs back in to show the notes
persisted, still isolated, keyed by the stable account id.
