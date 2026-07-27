# AGENTS.md — the working contract, and the language

You are working on **Ashlar**, an AI-first composition language. This is the
one file to read before touching anything: the contract first, the reference
second. Humans start at `README.md`; the two must never
disagree, and fixing a disagreement is part of your task.
`docs/writing-ashlar.md` collects the traps that catch agents who guess.

## The one rule that orders all others

```
VISION          docs/vision.md. Fixed. If the vision is wrong, stop and say so.
REQUIREMENTS    docs/requirements.md. Revised only when it fails the vision.
TESTS           The current best encoding of the requirements. Revised freely.
CODE            Whatever makes the tests pass.
```

Lower layers yield to higher ones — always. "The test is inconvenient" never
justifies changing a test; "the test mis-encodes the requirement" is the only
thing that does. Everything below the vision is **yours to change without
asking**, provided the change serves the layer above and arrives with the
evidence and tests that show it does. Requirements are revised by **execution**,
not argument (`docs/requirements.md` §1–2).

Stop and say so in exactly two cases: **the vision looks wrong** (the user's to
change — argue, never edit), or **you cannot get the evidence** (an agent read
needs a fresh agent, a benchmark needs a machine). Neither covers "this is
large" or "this reverses an ADR": both are normal, and new evidence outranks
an older record.

## Hard rules (each has a test with teeth)

1. **`meld` and `pattern` are banned** from the language and its docs (T-B).
2. **Zero dependencies** (G1): no external crates. JSON, SHA-1, HTTP,
   WebSockets, PBKDF2 are hand-rolled. Write the code; don't add a crate.
3. **`unsafe` appears exactly twice**, both `#[cfg(unix)]`: `dlopen`/`dlsym` in
   `foreign.rs`, and `signal` in `http.rs`, whose handler only sets an
   `AtomicBool` (ADR-0029). Never add a third for a capability: `worker` and
   `http` are safe Rust.
4. **No stubs** (`t_no_stubs`): no `todo!`, `unimplemented!`, no commented-out
   "coming soon". A construct that doesn't fully work does not exist.
5. **Diagnostic ids are stable** (`docs/diagnostics.md`): reuse an id with a new
   cause when the requirement is the same; a new id is appended, never
   renumbered, with its catalog row in the same commit.
6. **Diagnostics are corrections.** One-sentence cause, correction specific
   enough to apply without judgment. Machine edits leave the program strictly
   better (D2) and never change what a name resolves to (D6). T-D5 watches the
   mean rounds-to-clean and the share of the corpus that is fixable at all.
7. **No false positives.** `Unknown` absorbs what the checker cannot prove. When
   in doubt, stay silent and note the gap.
8. **Examples are corpus** (`t_examples`): everything under `examples/` compiles
   clean, is formatted, and is DRIVEN over real HTTP/WebSockets — a broken
   example is a failing test, not a discovery. T-BROWSER re-drives it with a
   real browser and a finding closes only THERE. A capability the corpus does
   not use defends nothing; the undefended list is in `docs/roadmap.md`.
9. **Refactors never partially apply** (E-series): radius first, atomic apply,
   post-verify rollback, reversal to the same PROGRAM — same parts, homes and
   composition order, closure only ever WIDENED. Byte-identity is a property
   specific commands have, not a law (ADR-0018); never weaken a passing
   assertion.

## The suite is the definition of done

```
cargo test                 # all 17 binaries; must be green in debug
cargo test --release       # F1 latency gate is release-only (<100ms hard)
cargo build --tests        # zero warnings, always
```

Floor is **Rust 1.65** and `Cargo.lock` stays at lockfile **version 3**, both
pinned by `t_meta_toolchain_floor_is_declared_and_reachable`; "zero warnings"
means on the floor too. `suites/coverage.md` maps every requirement to its test
and T-META checks that map both ways — read it rather than a list here. Two
suites cannot run in CI: **T-A3**, the agent-read gate
(`suites/t_a3/PROTOCOL.md`), run with this file in context; and **T-BROWSER**
(`suites/t_browser/`), needing a browser and node. Every new behavior lands
with the test that catches its regression.

## Sync duties — what must move together

| you changed | you must also touch |
|---|---|
| language behavior | the reference below + a test + (if user-visible failure) `docs/diagnostics.md` |
| a diagnostic's cause/fix | its `docs/diagnostics.md` row |
| a design trade | **EDIT the ADR that owns it.** A new file only when it reverses or supersedes one; an ADR that deferred its implementation closes in place with a Resolution. Carrying out a decision already made spawns no file |
| work found but not done | `docs/roadmap.md`, with its requirement and the test that will prove it. Delivered work does NOT go there |
| anything in `README.md` | keep README, this file, and reality agreeing |
| the reference below | re-run T-A1/A2/A5 and eyeball the budget |

## Operating discipline

- Work on a branch; every merged increment leaves the suite green and the docs
  true. Never commit runtime artifacts (`.ashlar-state.json`, `ashlar.manifest`).
- Contract files (`tokens.rs`, `ast.rs`, `diag.rs`, `resolved.rs`, `lib.rs`)
  change rarely and deliberately — they are the interfaces between stages.
- A bug's fix lands WITH the regression test that would have caught it, in the
  same commit; big claims get adversarial verification against the built binary.
- The honest sentence beats the impressive one — in diagnostics, docs, commit
  messages, and this file.
- **Prose is not an artifact.** Before adding a file, section, or paragraph —
  an ADR above all — find the one that should have said it and fix that. A
  recital duplicating something checkable is cruft the moment it is written.

**Budget: ≤40,000 bytes total, reference included** (T-A1), no reference section
over 20% of the reference's bytes (T-A5). Every sentence below is true of the
binary and every ```ash block compiles clean (T-A2). Spend words like money.

<!-- REFERENCE:BEGIN -->
## The Ashlar Reference

Servers and interfaces are Ashlar's current runtime target — delivered scope,
not the language's identity (ADR-0030). Everything the language and that
runtime do is below; anything this reference does not define is a compile
error, never a silent behavior.

## 1. Files and lexical rules

A source file is UTF-8: a **space header**, then zero or more **use
declarations**, then zero or more **part declarations**. Nothing else may
appear at the top level.

```ash
space chat.ui
use chat.data

part Timestamp {
  show = (at: number) => text(at)
}
```

Statements end at end of line; semicolons are a compile error whose fix removes
them. Comments run from `//` to end of line, and `#` is an error with the same
fix. Blocks use `{ }`. Indentation is not significant — `ashlar fmt`
canonicalizes it to two spaces. Commas separate items inside `( )`, `[ ]`, and
inline `{ }` literals; a trailing comma is allowed.

Reserved words: `space use part foreign state stored peruser setting append deep
stack pipe reverse let if else for in return true false none and or not`. A
reserved word cannot name anything. The shape names of §5 (`text`, `number`,
`bool`, `data`) are recognized only in shape positions and are ordinary names
elsewhere — which is how `text(n)` the conversion and `text` the shape coexist.

Identifiers are letters, digits, and `_`, not starting with a digit. Two names
in one scope differing only by case or separator convention (`userName` and
`user_name`) are a compile error: the compiler supplies naming discipline.
Dotted identifiers like `chat.ui.Message` are **names**; dots group names for
reading and imply no relationship between levels.

## 2. Names, spaces, and use

Every declaration lives in a space, named by the space header. A part's **full
name** joins the two: `part Message` in `space chat.data` declares
`chat.data.Message`.

`use` names a space and brings every name it provides into scope — its own
parts and everything it uses, transitively. There is no import list, no
aliasing, no way to bring in a single name. `use` of a part name is a compile
error whose fix names the part's space.

A name resolves against everything in scope, and must resolve to exactly one
definition:

- Zero resolutions: compile error, the fix naming the closest matches and the
  `use` that would provide them.
- More than one: compile error, the fix rewriting the reference as a full
  dotted name. There is no shadowing and no local preference — a bare name two
  visible parts could answer to is ambiguous even if one is declared in the
  current space.

A full dotted name resolves if the part exists and is visible through the use
graph. Source never contains a location — no paths, URLs, or versions. Where
code lives is in the manifest (§10), which the build derives.

`std` is provided by the runtime and implicitly used everywhere; its parts and
functions (§9) resolve like any name and may be qualified (`std.len`) when a
bare name is ambiguous. Declaring a layer on a `std` part is a compile error.

## 3. Parts

```ash
space chat.data

part Message {
  id: text
  body: text
  sent: number
  read: bool = false
}
```

A part declaration is `part Name { properties }`. A bare name introduces a part
in the current space; a dotted name declares a **layer** on an existing part
and must match the full name of one visible through the use graph. A dotted
name matching nothing is a compile error naming the nearest match, so a typo
can never silently introduce a new part:

```ash
space chat.audit
use chat.data

part chat.data.Message {
  audit: text = "none"
}
```

A part is a singleton: its full or bare name yields the one composed part.
Parts used as views (§9.4) also instantiate per use. A part with only field
properties (name and shape, no value) is a **data shape**, whose values are
written as plain literals (§6). Each space may declare at most one layer of a
part; a second is a compile error whose fix merges the blocks.

### Composition order

Layers flatten in **use order**: if space B uses space A, directly or
transitively, B's layer sits on A's. The result is deterministic and computed
from declarations alone; file layout never affects it. A cycle in the use graph
is a compile error naming the cycle.

If two spaces layer the same part and neither uses the other, the compiler
orders them by space name and emits `W001` naming both layers and the `use`
that would order them. Add it to decide the order deliberately — and note that
doing so can resequence layers elsewhere, which `W002` reports (§11).

## 4. Properties and merge kinds

A property is declared as:

```
[setting] [peruser] [state|stored] name [kind [reverse]] [: shape] [= expression]
```

- With `= expression` and no storage word: a **value property**, a build-time
  fact, immutable at runtime.
- With a shape and no `=`: a **field**. Data shapes and view parts declare
  fields; a field with `= expression` has a default.
- With `state` or `stored`, optionally prefixed `peruser` (§9.3): a **state
  property**, runtime-mutable, initial value required. `peruser` without a
  storage word is a compile error.
- With `setting`: a **setting** (§9.12) — the shape is source, the value a
  deployment fact. A shape is required; a storage word is a compile error,
  since a setting is fixed before the program runs.

Each property name is declared at most once per layer.

**kind** is how layers of one property combine when the part flattens. There
are exactly five:

| kind | behavior |
|---|---|
| *(none)* | Replace. The later layer's definition wins entirely. |
| `append` | Lists concatenate, text concatenates, maps merge one level. |
| `deep` | Like `append`, but maps merge at every depth. |
| `stack` | All layers' functions run in order; each return merges onto the receiver. |
| `pipe` | All layers' functions run in order; each receives the previous return. |

Omitting the kind means replace; the common case carries no ceremony. The kind
is part of the property's identity, fixed by the base-most layer declaring it,
and every later layer touching that property must restate it: a different kind,
or none where the identity has one, is a compile error whose fix restates the
declared kind. (To change a kind, use the `rekind` refactor, §11.)

`append` and `deep` apply to text, lists, and maps; on a number, bool, or
function they are a compile error. Merging is computed at build time and fully
determined by the layered values: no outcome depends on runtime state.

```ash
space config

part Config {
  greeting = "hello"                  // replace
  tags append: [text] = ["core"]
  limits deep = { http: { max: 10 } }
}
```

### stack and pipe

`stack` and `pipe` properties hold functions. Calling the property runs every
layer's function in composition order.

- `stack` functions take no parameters and must return a map or `none`; a
  returned map merges one level onto the part's state properties, and the call
  returns the part. Lifecycle is not a separate concept: it is `stack` plus the
  use order.
- `pipe` functions take exactly one parameter. The first receives the call's
  argument, each later one the previous return, and the call returns the last.
  All layers must agree in parameter and return shape.

`reverse` after `stack` or `pipe` runs layers derived-to-base — the right
default for teardown — and is fixed with the kind, restated like it.

```ash
space srv

part Server {
  state ready: bool = false
  start stack = () => {
    return { ready: true }
  }
  stop stack reverse = () => {
    log.info("stopping")
    return none
  }
  handle pipe = (req: std.Request) => req
}
```

## 5. Shapes

Every expression has a shape known at build time.

- `text` — UTF-8. Literals use `"` or `'` (formatter canonicalizes to `"`);
  escapes `\" \' \\ \n \t`. A literal may not contain a raw newline (join with
  `+`) or `${` — there is no text interpolation; both are compile errors.
- `number` — IEEE-754 double, integers exact to 2^53. Literals `42`, `3.5`, `-1`.
- `bool` — `true` or `false`.
- `[shape]` — list, e.g. `[text]`. Literal: `[1, 2, 3]`.
- `{text: shape}` — map. Keys are always text and the key shape is written
  literally as `text`, e.g. `{text: number}`. Literal: `{ a: 1, b: 2 }`, keys
  bare identifiers or text literals. Any other key shape is a compile error.
- `data` — any of text, number, bool, none, list of data, map of data: the
  shape of decoded payloads.
- A part name — the composed part (for a data shape, values matching its
  fields; otherwise the singleton).
- `shape?` — optional: the shape or `none`. Plain shapes never hold `none`.
- `(shapes) -> shape` — a function shape, used in `foreign` (§9.10).

A literal is checked against the shape its position expects. For a data-shape
part: every field without a default present, every present key a declared
field, every value matching the field's shape.

```ash
space chat.view
use chat.data

part Latest {
  last: chat.data.Message? = none
}
```

Function parameters declare shapes; return shapes and `let` locals are
inferred. Shape mismatches are compile errors stating the expected and actual
shape and the smallest correcting edit.

## 6. Expressions

Literals: text, number, `true`, `false`, `none`, lists, maps, and data-shape
literals as above. Spread inside literals copies entries: `[...xs, x]`,
`{ ...m, read: true }` (later keys win).

Operators, loosest to tightest binding:

| operators | meaning |
|---|---|
| `or` | boolean or, short-circuit |
| `and` | boolean and, short-circuit |
| `not` | boolean not (prefix) |
| `== !=` | structural equality on any two values of one shape |
| `< <= > >=` | number and text ordering |
| `??` | if the left is `none`, the right; else the left |
| `+ -` | number add, subtract; `+` also joins two texts or two lists |
| `* / %` | number multiply, divide, remainder |
| `!` | (postfix) asserts non-`none`: yields the value, fails at runtime on `none` |
| `.` `[ ]` `( )` | field access, index, call |

Both operands must share one shape; mixing (text `+` number) is a compile error
whose fix inserts a conversion such as `text(n)`. Conditions must be `bool` —
there is no truthiness, and any other shape as a condition is a compile error.

Access:

- `value.field` — field of a data-shape value or property of a part, checked at
  build time: an unknown field is a compile error. On a `data` value it is a
  lookup yielding `data?` and is not checked (below).
- `list[i]` — index from 0; shape `element?` (`none` past the end).
- `map[key]` — lookup; shape `value?` (`none` when absent). Computed keys are
  data access only: parts, properties, spaces, and every name the compiler
  reasons about cannot be reached by a computed key.
- `f(args)` — call. Arity and shapes checked.

`if` is an expression when both branches are present and yield one shape:
`let label = if read { "seen" } else { "new" }`.

Division by zero and `!` on `none` are the two runtime faults expressions can
raise; both carry the source location and fail the surrounding request or task
(§9.2). They are undetectable at build time: both depend on runtime values. Field access on a `data` value is the third thing left to runtime and
the only silent one: a runtime union has no fields to check, so `e.data.valeu`
answers `none` rather than failing the build.

## 7. Statements and functions

Function literals take `name: shape` parameters (`()` when there are none)
and have an expression body or a block body:

```ash
space demo

part math {
  double = (n: number) => n * 2
  describe = (items: [text]) => {
    for i in items {
      log.info(i)
    }
  }
}
```

A block body returns with `return expression`, or `return` or falling off the
end (both `none`). Statements:

- `let name = expression` — local binding, single-assignment. A `let` or
  parameter name already visible — a part, a property of the enclosing part, or
  a `std` name — is a compile error; rename the local. There is no shadowing
  anywhere in Ashlar.
- `name = expression` — assignment to a state property of the enclosing part
  (§9.3).
- `if cond { ... } else if cond { ... } else { ... }` — parentheses around the
  condition are allowed as grouping. Branches are blocks.
- `for x in listValue { ... }` — iterate a list.
- `for k, v in mapValue { ... }` — iterate a map's entries, key-ordered.
- An expression alone — evaluated for effect.

There is no `while`, no `switch`, and no exception handling; `if`, `for`,
recursion among named functions, and `fail` (§9.9) cover their uses, and a
construct this reference does not define is a parse error.

**Where functions may appear.** A function literal is legal in exactly two
positions: as a property's value — which names it — and inside a call's
argument, where it is single-use. "Inside an argument" includes a list or map
literal written there, which is how an event handler reaches an element's attrs
(§9.4). It cannot be bound with `let`, put in a property's own list or map,
stored in a field, or returned: a function is either named or handed straight
to a call. A *named* function — a property
whose value is a function — is a value: `Part.save` may be passed, stored, and
referenced, because it has a name the toolchain can rename and track. Function
properties may call each other, recursion included, through their names.

## 8. Errors and diagnostics

Compiler output is machine-readable first. `ashlar check` writes one JSON
object per diagnostic, one per line:

```
{"id":"E002","req":"B3","level":"error",
 "loc":{"file":"chat/ui.ash","line":4,"col":10,"end_line":4,"end_col":17},
 "cause":"`Message` is ambiguous: it could be `chat.data.Message` or `note.Message`.",
 "fix":{"note":"Qualify it with the one you mean: `chat.data.Message`, `note.Message`.",
   "edits":[]}}
```

Every diagnostic identifies a location, states the cause in one sentence, and
states the correction specifically enough to apply without judgment. When a
`fix` with `edits` is present, applying it produces source that compiles past
that error without introducing a new one and never changes what a name resolves
to; `ashlar fix` applies such fixes. Where no edit is derivable without a judgment
the author has not made — an ambiguous name — `edits` is empty and the note
names every candidate. `id`s are stable, `req` names the
requirement enforced, and `ashlar check --human` renders the same diagnostics
as prose. Warnings never block a build; errors do.

## 9. The runtime

One binary, `ashlar`, compiles and runs programs. No install step, no package
manager, no registry: everything below is built in, everything else enters
through `foreign` (§9.10).

### 9.1 Running

A part with a `port` property is a server root. `ashlar run` starts the
program's single one, or errors listing candidates if there is not exactly one;
`ashlar run chat.app` names one explicitly. The bound port is the root's `port`
unless `ashlar run --port 8091` overrides it — a deployment fact bound at run
time, never written in source (B5). On start the runtime loads stored state
(§9.3), then calls the root's `start` stack if declared. On shutdown — `SIGINT` or
`SIGTERM` on unix — it calls `stop`, prints what that stack logged, then
flushes stored state; elsewhere a signal is not caught and only stored state
survives, flushed whenever it changes. A source
change rebuilds and hot-reloads in place: state properties carry over by full
name, open pages reconnect and re-render, and a change that fails to compile
emits diagnostics and leaves the old program running.

```ash
space chat

part app {
  port = 8080
  start stack = () => {
    log.info("up")
    return none
  }
}
```

### 9.2 Requests and routing

A part with a `route` property receives requests. `route` is text matched
against the request path; `{name}` segments capture into `params`.

```ash
space chat.api
use chat.data

part messages {
  route = "/api/messages/{id}"
  allow = (req: std.Request) => req.user != none
  handle pipe = (req: std.Request) => {
    let m = chat.data.Store.messages[req.params["id"]!]
    return m ?? fail(404, "no such message")
  }
}
```

`std.Request` has `path: text`, `method: text` (lowercase),
`params: {text: text}`, `data: data` (the decoded JSON or form body, `none` if
absent), `headers: {text: text}`, `user: std.User?` (§9.6).

The same handler serves HTTP and WebSocket; transport is not visible in handler
code. Over HTTP the path is the URL; over the built-in socket a client sends
`{path, data}` envelopes to the same routes. The return value is the response:
a data value renders as JSON, text as plain text, an `Element` (§9.4) as an
HTML document, `redirect(path)` as a redirect. `fail` ends it with a status (§9.9); an
uncaught runtime fault ends it with 500 and a structured log entry. Two routes matching one path is a compile error naming both.

### 9.3 State

State properties are the runtime-mutable data of a part. Two axes describe
one: its **lifetime** — `state` (in memory) or `stored` (on disk) — and its
**scope** — shared by everyone, or `peruser`.

```ash
space chat.data

part Store {
  state draft: text = ""                            // in memory, shared
  stored messages: {text: chat.data.Message} = {}   // on disk, shared
  peruser stored seen: number = 0                    // on disk, per user
}
```

- `state` — lives for the process (per instance in view parts, §9.4).
- `stored` — persisted by the runtime's embedded store, keyed by the property's
  full name; survives restarts. Values are validated against the current shape
  at startup: a field added since the value was written is filled from its
  default, one with no default is a startup error naming every gap at once.
- `peruser` — a modifier before `state` or `stored`: the value is scoped to the
  current user, each isolated from every other by construction. Reading or
  writing one with no user in scope — an anonymous request, a scheduled task,
  `spawn`, or a `start` stack — is a runtime fault (§9.9), never a silently
  shared value. Whether a user is in scope is a fact about the call, not the
  declaration, so it is not decidable at build time.

Every state property is reactive, and because views render on the server with
no client code (§9.4) that reach is universal: any view that read a value
re-renders when it changes — a shared value across every client, a `peruser`
value only its own user's.

Assignment (`name = expression`) rebinds a state property. Values are immutable,
so to change a list or map assign a new one (`messages = { ...messages, [id]: m }`
is not legal — computed keys cannot appear in literals; use
`put(messages, id, m)`). Only functions in layers of the owning part may assign
its state properties; others read them by name or call a function that assigns.

### 9.4 Views

A part with a `view` property is a UI element: a zero-parameter function
returning `std.Element`, built with `el`:

```
el(tag: text, attrs: {text: text}?, children: [std.Element]?)
el(PartName, fields: {text: data}?, children: [std.Element]?)
```

Text values may appear in `children` and render escaped. A part used with `el`
instantiates per use: its fields come from the second argument and its `state`
properties are per-instance. An instance *is* its view's root element, with no
wrapper, so a layout container sees its child views directly. Across re-renders
a view reuses children by position — the same `el(Part)` keeps the same
instance, so per-instance state and subscriptions survive, `start` runs once on
mount and `stop` once when the child is no longer rendered (§9.5).

```ash
space chat.widgets

part counter {
  label: text
  state n: number = 0
  view = () => el("button", { onclick: bump }, [label + ": " + text(n)])
  bump = () => { n = n + 1 }
}
```

Views render on the server and the browser runs no program code. Events named
in attrs (`onclick`, `onsubmit`, `oninput`, carrying `value` and `caret` — the
caret's offset, or `none` where the target has none — in the event's `data`)
round-trip over the built-in socket, handlers run server-side, and every view
that read a changed state property re-renders and patches in place, preserving
the focused field, its caret and typing in flight; a server-side change to the
value (a cleared draft) still wins. An attr value is text, the name of a
function property, or an inline function of zero or one parameter
(`(e: std.Event) => ...`; `std.Event` has `name: text` and `data: data`).
Serving a view part from a `route` wires this up; no other setup exists.

A `title` element names the page: it sets the browser tab and re-renders like
any other element, so a title reading state follows it.

```ash
space site

part pad {
  route = "/pad"
  state name: text = "untitled"
  view = () => el("div", {}, [
    el("title", {}, ["pad — " + name]),
    el("h1", {}, [name]),
  ])
}
```

Appearance is bound by name, never location. Elements carry `class` names and a
stylesheet supplies the rules. The server root names its sheet — `style = "app"`
resolves to `assets/app.css` like `files` (§9.8), a missing declared sheet is a
build error, and the runtime links it into every page. A `style` string
attribute is the wrong tool and unchecked; give the element a `class`.

### 9.5 Channels

Named broadcast channels connect running code and clients. Channel names are
runtime data, not program names.

```
publish(channel: text, message: data)
subscribe(channel: text, handler)   // handler: (message: data) => ...
```

`subscribe` in a view part's `start stack` subscribes that instance and
unsubscribes on unmount; anywhere else it lives for the process. Cross-client
reactivity (§9.3) rides the same broadcast and needs no channel.

Handlers run in subscription order, and a fault in one is logged without
stopping the others or failing the publisher — the rule `spawn` follows
(§9.9): a subscriber is someone else's code.

A socket can die with neither end told and TCP never says so. The runtime
therefore heartbeats every open socket and sheds a peer that stops answering; a
page missing the beats it is owed marks `<html>` with `data-ash-offline` and
reconnects when the server returns. Style that attribute: a stale page that
looks live is the one failure this language will not leave silent.

### 9.6 Auth

The runtime provides accounts, sessions, and request identity.

- `signup(email: text, password: text) -> std.User` — creates an account, or
  fails 409 on a duplicate email.
- `login(email: text, password: text) -> std.User` — verifies and opens a
  session (cookie over HTTP, socket-scoped otherwise); fails 401 on bad
  credentials.
- `logout()` — ends the session.
- `req.user: std.User?` — the session's account. `std.User` has `id: text`
  and `email: text`.

The session cookie is `HttpOnly` and `SameSite=Lax`, gaining `Secure` when the
request arrived over TLS — an `X-Forwarded-Proto: https` from a terminating
proxy (ADR-0013).

Authorization is the `allow` property (§9.2): any routed part may declare
`allow = (req: std.Request) => bool`; `false` ends the request with 403 before
`handle` runs. It composes as replace unless a kind is declared.

### 9.7 Tasks and schedules

`spawn(f)` runs a zero-parameter function in the background; a fault in it is
logged, not fatal. A part with an `every` property is a scheduled task: the
runtime calls its `run` property on that interval. `every` is a text duration —
digits then `ms`, `s`, `m`, `h`, or `d` — checked at build time, and `every`
with no `run` is a compile error.

```ash
space jobs

part sweep {
  every = "10m"
  run = () => { log.info("sweeping") }
}
```

### 9.8 Files

A part with a `files` property serves static assets. Its value names an asset
under the project's `assets/` root, and what it names decides how it serves: a
**directory** mounts under the part's `route` as a prefix, a **file** answers
that one route exactly and nothing below.

```ash
space site

part static {
  route = "/static"
  files = "public"          // assets/public/ at /static/...
}

part robots {
  route = "/robots.txt"
  files = "robots.txt"      // that one file, at that one path
}
```

The single-file form answers absolute paths a program does not choose —
`/favicon.ico`, `/robots.txt` — without taking `/` from a page.

### 9.9 Logging and failure

`log.debug`, `log.info`, `log.warn`, `log.error` each take a message and an
optional data map: `log.warn("slow", { ms: elapsed })`. Entries are JSON with
timestamp, level, message, data, and source location.

`fail(message)` or `fail(status, message)` raises a runtime fault: the current
request ends with that status (500 if unstated), the current task logs it.
There is no catching — a condition worth recovering from is worth a
`none`-returning function and a `??`. `fail` never returns, so it fits any shape
and refusing costs one call: `number(t) ?? fail(400, "not a number")`.

### 9.10 Foreign functions

Everything outside the builtin set crosses one boundary:

```ash
space net

foreign fetch: (url: text) -> data
foreign post: (url: text, body: data) -> data
```

`foreign name: (shapes) -> shape` declares a CAPABILITY implemented outside
Ashlar. Arguments and returns cross as data, shape-checked at the boundary at
runtime; a mismatch is a fault at the call site.

**How a capability is reached is a deployment fact, never source.** By default
the build binds space `s` to the host library `foreign/s` (`.so`/`.dylib`). An
optional `foreign.json` at the project root (or at `ASHLAR_FOREIGN`) overrides
that per space; the manifest records whichever won, and a key naming no space
is `E001`:

| `via` | reached by | fields |
|---|---|---|
| `native` | `dlopen`, C ABI `char* f(const char* args_json)`; needs a POSIX loader | `library`, `symbols` |
| `worker` | a co-process speaking JSON Lines on stdin/stdout | `run` |
| `http` | POST to a URL (plaintext; TLS terminates at a proxy) | `url` |

```json
{ "tools": { "via": "worker", "run": ["python3", "foreign/tools.py"] },
  "geo":   { "via": "native", "library": "/usr/lib/libgeo.so.3",
             "symbols": { "lookup": "geo_lookup_v2" } } }
```

`symbols` binds an Ashlar name to a differently-spelled export. Every transport carries one envelope: a request is
`{"call": name, "args": [...]}`, an answer is a bare value, `{"ok": value}`, or
`{"error": text}` — the last a fault carrying that message. A `native` library
may export `ashlar_free(char*)` to take its buffer back. A worker is therefore
a loop in any language: read a line, decode `call` and `args`, print one JSON
answer, flush.

Reachability is not a build-time fact — the machine that compiles is not the
machine that deploys — so `ashlar foreign check` proves it on demand against the
bindings in force, before a request finds out. Foreign calls may block; the
runtime schedules around them.

One space name derives to a co-process rather than a library, and that
co-process is this toolchain: `mesh` — who else is on the private network this
machine joined, and the sites they serve. `ashlar mesh worker` speaks the
control socket the mesh already exposes to its own clients, so nothing outside
the project changes to make the boundary work. `ashlar run --mesh` publishes
the port it is serving through it, reaching that network and nobody else;
`ashlar mesh` says what it answers.

A foreign call may name a reactive collection, so a store behind the boundary
is live without leaving the language:

```ash
space store

part Row {
  key: text
}

foreign save: (key: text) -> bool updates Row
foreign all: () -> [Row] watches Row
```

`watches <Shape>` makes the call a dependency edge — a view calling it
re-renders when the collection changes — and `updates <Shape>` marks that
collection changed, so every view that read it re-renders and patches across
every connected client (§9.3). The collection is the data shape it names;
`watches`/`updates` are contextual (ordinary names elsewhere), and one resolving
to no part is E001.

### 9.11 std

The builtin space, implicitly used everywhere. Parts: `Request`, `Event`,
`User`, `Element`. Functions, in addition to `el`, `publish`, `subscribe`,
`signup`, `login`, `logout`, `spawn`, `redirect`, `fail`, and `log.*` above:

| function | meaning |
|---|---|
| `len(x)` | length of a text, list, or map |
| `range(n)` | `[0, 1, ..., n-1]` |
| `keys(m)` | a map's keys as a sorted list |
| `put(m, k, v)` | copy of map `m` with `k` set to `v` |
| `drop(m, k)` | copy of map `m` without key `k` |
| `slice(x, from, to)` | sub-list or sub-text, indexes from 0, end-exclusive |
| `find(xs, f)` | first element where `f(x)` is true, else `none` |
| `map(xs, f)` | list of `f(x)` for each element |
| `filter(xs, f)` | elements where `f(x)` is true |
| `sort(xs, f)` | copy sorted by comparing `f(x)` values |
| `join(xs, sep)` | texts joined with separator |
| `split(t, sep)` | text split into a list |
| `contains(x, y)` | whether text/list `x` contains `y` |
| `text(x)` | any value rendered as text |
| `number(t)` | text parsed as number, else `none` |
| `json(t)` | text parsed as data, else `none` |
| `fields(x)` | `x` if it is a map of data, else `none` |
| `now()` | milliseconds since epoch |
| `id()` | a new unique text id |

### 9.12 Settings

A program often depends on what it cannot know when written — where a service
is, a key, a limit. `setting` declares that: the name and shape are source, the
value a deployment fact.

```ash
space site

part app {
  port = 8080
  setting endpoint: text
  setting retries: number = 3
}
```

Values live in `settings.json` at the project root (or at `ASHLAR_SETTINGS`), a
JSON object keyed by full property name — `{"site.app.endpoint": "..."}`. One
with a default is optional; one without is required, and starting without it
fails before the first request, naming every missing setting and its shape at
once. A supplied value that does not fit its shape fails the same way. Read a
setting like any other property; it is immutable, so cannot be assigned. This
is how a location reaches a program without being written in source (B5). A key
naming no declared setting is
`E001`, a value of the wrong shape `E006`.

## 10. The build and the manifest

The build scans the project tree, resolves every name, flattens every part, and
writes `ashlar.manifest` (JSON): the format version, each space with the files
that declare into it, each part with its layers in composition order (space,
file, line), the use graph, foreign bindings, and asset locations. It is also
the baseline the next build's delta is measured against (§11).

The manifest is state, the source is intent: fully derived, never hand-edited,
and deleting it and rebuilding reproduces it exactly. Moving a source file
changes nothing but its recorded locations. The build is incremental — a
single-file change re-checks in under 100ms at a thousand files.

## 11. The toolchain

| command | effect |
|---|---|
| `ashlar check` | compile; emit diagnostics as JSON lines (`--human` for prose) |
| `ashlar build` | check, then write the manifest and executable image |
| `ashlar run [part] [--port n] [--mesh [network]]` | build, then start the server root, watching for changes; `--port` overrides the bound port, `--mesh` publishes the site to a private mesh (§9.10) |
| `ashlar mesh` | print what this machine's mesh answers: identity, roster, published sites |
| `ashlar fmt` | rewrite source into canonical formatting |
| `ashlar fix [id]` | apply machine-applicable fixes from the last check |
| `ashlar rename <full-name> <new-name>` | rename a space, part, or property |
| `ashlar rekind <part.prop> <kind>` | change a property's merge kind across all layers |
| `ashlar move <part> <space>` | move a part's home declaration to another space, adding the `use` lines both sides need |
| `ashlar radius <full-name>` | print every location a rename of the name would touch |
| `ashlar delta` | print what this working tree changed about the program's derived state since the last `ashlar build` |
| `ashlar vendor <source>` | copy an external tree into the project so its spaces resolve |

Refactors are commands, not text edits. Each computes and reports its complete
blast radius from the manifest, applies atomically or not at all — refusing with
a reason if the radius cannot be fully computed — leaves no stale reference
behind, and reverses to the same program, though not the same bytes: `rename` and `rekind` reverse byte-identically, while
`move` adds the `use` lines both sides need and never removes one, so reversing
it returns the same program with those lines present (ADR-0018). Every added line appears in the radius, and `radius` answers "what
would this touch" without touching it.

Adding a `use` has no command and the widest reach of any edit: it can
resequence composition order downstream (§3), so it is reported, not commanded. Against the previous build's manifest, an edit that resequences
any part's layers raises `W002` naming the part and its order before and after;
`ashlar delta` prints the report. No manifest, no baseline, no claim.

## 12. What programs cannot do

Each is a compile error naming the Ashlar construct instead: no macros,
user-defined syntax, or operator overloading — the surface does not extend; no
single-name imports or aliases; no
exceptions, `while`, truthiness, or text interpolation; no classes or
inheritance (layers on parts); no registry or version resolution (dependencies
are vendored); no dynamic access to anything named (computed keys reach data
only).
