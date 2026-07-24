# The Ashlar Reference

This is the complete reference for Ashlar, a composition language for servers
and interfaces. Everything the language does is described here. Anything this
reference does not define is a compile error, never a silent behavior. Source
files use the extension `.ash` and are UTF-8.

Ashlar programs are built from one composable unit, the **part**. UI elements,
routes, services, state stores, and data shapes are all parts and compose by
the same mechanism. Names are the only binding mechanism: no file path,
argument position, declaration order, or file location affects what a name
refers to.

## 1. Files and lexical rules

A source file is a **space header**, then zero or more **use declarations**,
then zero or more **part declarations**. Nothing else may appear at the top
level.

```ash
space chat.ui
use chat.data

part Timestamp {
  show = (at: number) => text(at)
}
```

Statements end at end of line. Semicolons are a compile error; the fix removes
them. Comments run from `//` to end of line; `#` is a compile error with the
same fix. Blocks are enclosed in `{ }`. Indentation is not significant; the
formatter (`ashlar fmt`) canonicalizes it to two spaces. Commas separate items
inside `( )`, `[ ]`, and inline `{ }` literals; a trailing comma is allowed.

Reserved words: `space use part foreign state stored owned append deep stack
pipe reverse let if else for in return true false none and or not`. A
reserved word cannot name anything. The shape names of §5 (`text`, `number`,
`bool`, `data`) are recognized only in shape positions and are ordinary names
everywhere else — which is how `text(n)` the conversion and `text` the shape
coexist.

Identifiers are letters, digits, and `_`, not starting with a digit. Two names
in the same scope that differ only by case or by separator convention (for
example `userName` and `user_name`) are a compile error: the compiler supplies
naming discipline. Dotted identifiers like `chat.ui.Message` are **names**;
dots group names for reading and imply no relationship between the dotted
levels.

## 2. Names, spaces, and use

Every declaration lives in a space. The space header names it. A part's **full
name** is the space name joined to the declared name: `part Message` in
`space chat.data` declares `chat.data.Message`.

`use` names a space and brings every name that space provides into scope:
its own parts, and everything it uses, transitively. There is no import list,
no aliasing, and no way to bring in a single name. `use` of a part name is a
compile error; the fix names the part's space.

A name reference resolves against everything in scope. It must resolve to
exactly one definition:

- Zero resolutions: compile error, with the fix naming the closest matches
  and the `use` that would provide them.
- More than one resolution: compile error; the fix rewrites the reference as
  a full dotted name. There is no shadowing and no local preference: a bare
  name that two visible parts could answer to is ambiguous even if one is
  declared in the current space.

A full dotted name always resolves if the part exists in the program and is
visible through the use graph. Source never contains a location: no paths, no
URLs, no versions. Where code lives is recorded in the manifest (§11), which
the build derives.

The space `std` is provided by the runtime and is implicitly used by every
space. Its parts and functions (§9) resolve like any other name and may be
qualified (`std.len`) when a bare name is ambiguous. Declaring a new layer on
a `std` part is a compile error.

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

A part declaration is `part Name { properties }`. A bare name introduces a
part in the current space. A dotted name declares a **layer** on an existing
part, and must match the full name of a part visible through the use graph —
a dotted name that matches nothing is a compile error naming the nearest
match, so a typo can never silently introduce a new part:

```ash
space chat.audit
use chat.data

part chat.data.Message {
  audit: text = "none"
}
```

A part is a singleton: referencing its full or bare name yields the one
composed part. Parts used as views (§9.4) additionally instantiate per use.
A part with only field properties (name and shape, no value) is a **data
shape**; values of that shape are written as plain literals (§6).

Each space may declare at most one layer of a given part; a second
declaration of the same part in the same space is a compile error, and the
fix merges the blocks.

### Composition order

Layers flatten in **use order**: if space B uses space A (directly or
transitively), B's layer sits on A's. The result is deterministic and
computed from declarations alone; file layout never affects it. A cycle in
the use graph is a compile error naming the cycle.

If two spaces layer the same part and neither uses the other, the compiler
orders them by space name and emits warning `W001` naming both layers and the
`use` declaration that would order them. Add that `use` to decide the order
deliberately.

## 4. Properties and merge kinds

A property is declared as:

```
[owned] [state|stored] name [kind [reverse]] [: shape] [= expression]
```

- With `= expression` and no storage word, the property is a **value
  property**: a build-time fact, immutable at runtime.
- With a shape and no `=`, it is a **field**: data shapes and view parts
  declare fields; a field with `= expression` has a default.
- With `state` or `stored`, optionally prefixed `owned` (§9.3), it is a
  **state property**: runtime-mutable, initial value required. `owned`
  without a storage word is a compile error.

Within one part, each property name is declared at most once per layer.

**kind** is the property's merge kind: how layers of the same property
combine when the part flattens. There are exactly five:

| kind | behavior |
|---|---|
| *(none)* | Replace. The later layer's definition wins entirely. |
| `append` | Lists concatenate, text concatenates, maps merge one level. |
| `deep` | Like `append`, but maps merge at every depth. |
| `stack` | All layers' functions run in order; each return merges onto the receiver. |
| `pipe` | All layers' functions run in order; each receives the previous return. |

Omitting the kind means replace; the common case carries no ceremony. The
kind is part of the property's identity, fixed by the base-most layer that
declares the property. Every later layer that touches the property must
restate the same kind: stating a different kind, or omitting the kind on a
property whose identity has one, is a compile error. The fix restates the
declared kind. (To actually change a kind, use the `rekind` refactor, §12.)

`append` and `deep` apply to text, lists, and maps; declaring them on a
number, bool, or function is a compile error. Merging is computed at build
time and is fully determined by the layered values: no merge outcome depends
on runtime state.

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

- `stack` functions take no parameters. Each must return a map or `none`; a
  returned map merges one level onto the part's state properties. The call
  returns the part. Lifecycle is not a separate concept: it is `stack` plus
  the use order.
- `pipe` functions take exactly one parameter. The first receives the call's
  argument; each later one receives the previous return; the call returns the
  last return. All layers of a `pipe` property must agree in parameter and
  return shape.

`reverse` after `stack` or `pipe` runs layers derived-to-base — the correct
default for teardown. `reverse` is fixed with the kind, restated like it.

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

- `text` — UTF-8 text. Literals use `"` or `'` (formatter canonicalizes to
  `"`); escapes `\" \' \\ \n \t`. A literal may not contain a raw newline
  (join lines with `+`) and may not contain `${` — Ashlar has no text
  interpolation; both are compile errors.
- `number` — IEEE-754 double. Integers are exact to 2^53. Literals: `42`,
  `3.5`, `-1`.
- `bool` — `true` or `false`.
- `[shape]` — list, e.g. `[text]`. Literal: `[1, 2, 3]`.
- `{text: shape}` — map. Keys are always text and the key shape is written
  literally as `text`, e.g. `{text: number}`. Literal: `{ a: 1, b: 2 }`;
  keys are bare identifiers or text literals. Any other key shape is a
  compile error with a correction.
- `data` — any of: text, number, bool, none, list of data, map of data. The
  shape of decoded payloads.
- A part name — the composed part (for a data shape, values matching its
  fields; for any other part, the singleton).
- `shape?` — optional: the shape or `none`. Plain shapes never hold `none`.
- `(shapes) -> shape` — a function shape; used in `foreign` declarations
  (§9.10).

A literal is checked against the shape the position expects. For a data-shape
part: every field without a default must be present, every present key must
be a declared field, every value must match the field's shape.

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

Both operands of an operator must share one shape; mixing (for example text
`+` number) is a compile error, and the fix inserts a conversion such as
`text(n)`. Conditions must be `bool`: there is no truthiness, and using any
other shape as a condition is a compile error.

Access:

- `value.field` — field of a data-shape value or property of a part. Checked
  at build time; unknown fields are compile errors.
- `list[i]` — index from 0; shape `element?` (`none` past the end).
- `map[key]` — lookup; shape `value?` (`none` when absent). Computed keys are
  data access only: parts, properties, spaces, and every name the compiler
  reasons about cannot be reached by computed key.
- `f(args)` — call. Arity and shapes checked.

`if` is an expression when both branches are present and yield one shape:
`let label = if read { "seen" } else { "new" }`.

Division by zero and `!` on `none` are the two runtime faults expressions can
raise; both carry the source location and fail the surrounding request or
task (§9.2). They are undetectable at build time because they depend on
runtime values.

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

A block body returns with `return expression`, or `return` (yielding `none`),
or by falling off the end (yielding `none`). Statements:

- `let name = expression` — local binding. Locals are single-assignment. A
  `let` or parameter name that is already visible — as a part, a property of
  the enclosing part, or a `std` name — is a compile error; rename the local.
  There is no shadowing anywhere in Ashlar.
- `name = expression` — assignment to a state property of the enclosing part
  (§9.3).
- `if cond { ... } else if cond { ... } else { ... }` — parentheses around
  the condition are allowed as grouping. Branches are blocks.
- `for x in listValue { ... }` — iterate a list.
- `for k, v in mapValue { ... }` — iterate a map's entries, key-ordered.
- An expression alone — evaluated for effect.

There is no `while`, no `switch`, and no exception handling; `if`, `for`,
recursion among named functions, and `fail` (§9.9) cover their uses, and a
construct this reference does not define is a parse error.

**Where functions may appear.** A function literal is legal in exactly two
positions: as the value of a property — which names it — and inside an
argument of a call, where it is single-use. A function literal cannot be
bound with `let`, stored in a list, map, or field, or returned from another
function. A *named* function — a property whose value is a function — is a
value: `Part.save` may be passed, stored, and referenced, because it has a
name the toolchain can rename and track.

Function properties may call each other, including recursively, through
their names.

## 8. Errors and diagnostics

Compiler output is machine-readable first. `ashlar check` writes one JSON
object per diagnostic, one per line:

```
{"id":"E002","req":"B3","level":"error",
 "loc":{"file":"chat/ui.ash","line":4,"col":10},
 "cause":"`Message` resolves to chat.data.Message and note.Message.",
 "fix":{"note":"Qualify the reference.",
   "edits":[{"file":"chat/ui.ash","line":4,"col":10,"end_col":17,
     "text":"chat.data.Message"}]}}
```

Every diagnostic identifies a location, states the cause in one sentence, and
states the correction specifically enough to apply without judgment. When a
`fix` with `edits` is present, applying it produces source that compiles past
that error without introducing a new one; `ashlar fix` applies such fixes.
Diagnostic `id`s are stable across releases; `req` names the requirement the
diagnostic enforces. `ashlar check --human` renders the same diagnostics as
prose. Warnings (`level":"warn"`) never block a build; errors always do.

## 9. The runtime

One binary, `ashlar`, compiles and runs programs. There is no install step,
no package manager, and no registry; everything below is built in, and
everything else enters through `foreign` (§9.10).

### 9.1 Running

A part with a `port` property is a server root. `ashlar run` starts the
program's single server root, or errors listing candidates if there is not
exactly one; `ashlar run chat.app` names one explicitly. The bound port is
the root's `port`, unless `ashlar run --port 8091` overrides it — a deployment
fact bound at run time, never written in source (B5). On start the runtime
loads stored state (§9.3), then calls the root's `start` stack property if
declared. On shutdown it calls `stop`, then flushes stored state. While
running, a source change rebuilds and hot-reloads the program in place;
state properties carry over by full name, and open pages reconnect and
re-render themselves. If the change fails to compile, diagnostics are
emitted and the old program keeps running.

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

A part with a `route` property receives requests. `route` is a text pattern
over the request path; `{name}` segments capture into `params`.

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

`std.Request` has fields `path: text`, `method: text` (lowercase),
`params: {text: text}`, `data: data` (the decoded JSON or form body, `none` when
absent), `headers: {text: text}`, and `user: std.User?` (§9.6).

The same handler serves HTTP and WebSocket; transport is not visible in
handler code. Over HTTP the path is the URL; over the built-in socket a
client sends `{path, data}` envelopes to the same routes. The return value is
the response: a data value renders as JSON, text as plain text, an `Element`
(§9.4) as an HTML document, `redirect(path)` as a redirect. `fail(status,
message)` ends the request with that status; an uncaught runtime fault ends
it with status 500 and a structured log entry. Two routes matching one path
is a compile error naming both.

### 9.3 State

State properties are the runtime-mutable data of a part. Two axes describe
one: its **lifetime** — `state` (in memory) or `stored` (on disk) — and its
**scope** — shared by everyone, or `owned` (per user).

```ash
space chat.data

part Store {
  state draft: text = ""                            // in memory, shared
  stored messages: {text: chat.data.Message} = {}   // on disk, shared
  owned stored seen: number = 0                      // on disk, per user
}
```

- `state` — lives for the process (per instance in view parts, §9.4).
- `stored` — persisted by the runtime's embedded store, keyed by the
  property's full name; survives restarts. Values are validated against the
  current shape at startup, and a mismatch is a startup error.
- `owned` — a modifier before `state` or `stored`: the value is scoped to
  the current user, so each signed-in user has their own, isolated from
  every other by construction. Reading or writing an `owned` property with
  no user in scope — an anonymous request, or a scheduled task, `spawn`, or
  `start` stack — is a runtime fault (§9.9), never a silently shared value.

Every state property is reactive, and because views render on the server
with no client code (§9.4) that reach is universal: any view that read a
value re-renders when it changes. A shared value reaches every client's
views; an `owned` value reaches only its own user's.

Assignment (`name = expression`) rebinds a state property. Values themselves
are immutable: to change a list or map, assign a new one
(`messages = { ...messages, [id]: m }` is not legal — computed keys cannot
appear in literals; use `put`: `messages = put(messages, id, m)`). Only
functions declared in layers of the owning part may assign its state
properties; other parts read them by name, or call a function property that
assigns. Every read is reactive: views observe reads automatically.

### 9.4 Views

A part with a `view` property is a UI element. `view` is a zero-parameter
function returning `std.Element`, built with `el`:

```
el(tag: text, attrs: {text: text}?, children: [std.Element]?)
el(PartName, fields: {text: data}?, children: [std.Element]?)
```

Text values may appear in `children` and render escaped. A part used with
`el` instantiates per use: its fields are set from the second argument, and
its `state` properties are per-instance. An instance *is* its view's root
element (the element carries the instance, with no wrapper around it), so a
layout container sees its child views directly. Across re-renders a view
reuses its children by position: the same `el(Part)` keeps the same
instance, so per-instance state and subscriptions survive and `start` runs
once on mount and `stop` once when the child is no longer rendered (§9.5).

```ash
space chat.widgets

part counter {
  label: text
  state n: number = 0
  view = () => el("button", { onclick: bump }, [label + ": " + text(n)])
  bump = () => { n = n + 1 }
}
```

Views render on the server. The browser runs no program code: events named
in attrs (`onclick`, `onsubmit`, `oninput`, with `value` in the event's
`data`) round-trip over the built-in socket, handlers run server-side, and
every view that read a changed state property re-renders and patches in
place. Patching preserves the focused field, its caret, and typing still
in flight; a server-side change to the field's value (a cleared draft)
still wins. An attr value is text, or the name of a function property, or an
inline function taking zero parameters or one (`(e: std.Event) => ...`;
`std.Event` has `name: text` and `data: data`). Serving a view part directly
from a `route` wires all of this up; no other setup exists.

Appearance is bound by name, never by a location in source. Elements carry
`class` names; a stylesheet supplies the rules. The server root names its
sheet — `style = "app"` resolves to `assets/app.css` like `files` (§9.8), a
missing declared sheet is a build error, and the runtime links it into
every served page. CSS is a foreign language for appearance: named in
Ashlar, defined outside it, the presentation peer of `foreign` (§9.10).
A `style` string attribute on an element is the wrong tool and unchecked;
give the element a `class` and write the rule in the sheet.

### 9.5 Channels

Named broadcast channels connect running code and clients. Channel names are
runtime data, not program names.

```
publish(channel: text, message: data)
subscribe(channel: text, handler)   // handler: (message: data) => ...
```

`subscribe` in a view part's `start stack` subscribes that instance and
unsubscribes it automatically when the instance unmounts; anywhere else the
subscription lives for the process. Cross-client reactivity (§9.3) rides
the same broadcast internally and needs no explicit channel.

### 9.6 Auth

The runtime provides accounts, sessions, and the request identity.

- `signup(email: text, password: text) -> std.User` — creates an account, or
  fails 409 on a duplicate email.
- `login(email: text, password: text) -> std.User` — verifies and opens a
  session (cookie over HTTP, socket-scoped otherwise); fails 401 on bad
  credentials.
- `logout()` — ends the session.
- `req.user: std.User?` — the session's account. `std.User` has `id: text`
  and `email: text`.

The session cookie is `HttpOnly` and `SameSite=Lax`, and gains `Secure`
when the request arrived over TLS — an `X-Forwarded-Proto: https` from a
terminating proxy in front of the server (ADR-0013).

Authorization is the `allow` property (§9.2): any routed part may declare
`allow = (req: std.Request) => bool`; `false` ends the request with 403
before `handle` runs. `allow` composes as replace unless a kind is declared.

### 9.7 Tasks and schedules

`spawn(f)` runs a zero-parameter function in the background; a fault in it is
logged, not fatal. A part with an `every` property is a scheduled task: the
runtime calls its `run` function property on that interval. `every` is a
text duration — digits then `ms`, `s`, `m`, `h`, or `d` — checked at build
time. A part with `every` and no `run` is a compile error.

```ash
space jobs

part sweep {
  every = "10m"
  run = () => { log.info("sweeping") }
}
```

### 9.8 Files

A part with a `files` property serves static assets. Its value names a
directory under the project's `assets/` root; the build records the actual
location in the manifest, and the route prefix is the part's `route`.

```ash
space site

part static {
  route = "/static"
  files = "public"     // serves assets/public at /static/...
}
```

### 9.9 Logging and failure

`log.debug`, `log.info`, `log.warn`, `log.error` each take a message and an
optional data map: `log.warn("slow", { ms: elapsed })`. Entries are
structured (JSON) with timestamp, level, message, data, and source location.

`fail(message)` or `fail(status, message)` raises a runtime fault: the
current request ends with that status (500 if unstated), the current task
logs it. There is no catching; a condition worth recovering from is worth a
`none`-returning function and a `??`.

### 9.10 Foreign functions

Everything outside the builtin set crosses one boundary:

```ash
space net

foreign fetch: (url: text) -> data
foreign post: (url: text, body: data) -> data
```

`foreign name: (shapes) -> shape` declares a function implemented outside
Ashlar. The build binds each foreign name of space `s` to the host library
`foreign/s` in the project (the manifest records the resolved location; a
missing or non-exporting library is a build error). Arguments and returns
cross as data and are shape-checked at the boundary at runtime; a mismatch
is a fault at the call site. Foreign calls may block; the runtime schedules
around them.

A foreign call may name a reactive collection, so a store behind the boundary
is live without leaving the language:

```ash
space store

part Row {
  key: text
}

foreign save: (key: text) -> bool writes Row
foreign all: () -> [Row] reads Row
```

`reads <Shape>` makes the call a dependency edge — a view that calls it
re-renders when the collection changes — and `writes <Shape>` marks that
collection changed, so every view that read it re-renders and patches, across
every connected client (§9.3). The collection is the data shape it names.
`reads`/`writes` are contextual (ordinary names elsewhere); one that resolves
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
| `now()` | milliseconds since epoch |
| `id()` | a new unique text id |

## 10. The build and the manifest

The build scans the project tree, resolves every name, flattens every part,
and writes `ashlar.manifest` (JSON): the format version, each space with the
files that declare into it, each part with its layers in composition order
(space, file, line), the use graph, foreign bindings, and asset locations.

The manifest is state, the source is intent: it is fully derived, never
hand-edited, and deleting it and rebuilding reproduces it exactly. Moving a
source file changes nothing but the manifest's recorded locations, because
no meaning attaches to where a file is. The build is incremental; a
single-file change re-checks in under 100ms at a thousand files, fast enough
to run on every edit.

## 11. The toolchain

| command | effect |
|---|---|
| `ashlar check` | compile; emit diagnostics as JSON lines (`--human` for prose) |
| `ashlar build` | check, then write the manifest and executable image |
| `ashlar run [part] [--port n]` | build, then start the server root, watching for changes; `--port` overrides the bound port |
| `ashlar fmt` | rewrite source into canonical formatting |
| `ashlar fix [id]` | apply machine-applicable fixes from the last check |
| `ashlar rename <full-name> <new-name>` | rename a space, part, or property |
| `ashlar rekind <part.prop> <kind>` | change a property's merge kind across all layers |
| `ashlar move <part> <space>` | move a part's home declaration to another space, adding the `use` lines both sides need |
| `ashlar radius <full-name>` | print every location a rename of the name would touch |
| `ashlar vendor <source>` | copy an external tree into the project so its spaces resolve |

Refactors are commands, not text edits. Each one first computes and reports
its complete blast radius from the manifest; applies atomically or not at
all, refusing with a reason if the radius cannot be fully computed; leaves no
stale reference behind; and is reversible. `rename` and `rekind` reversed
yield byte-identical source; `move` does too when the part sits at its
file's end and no `use` line needed adding — `move` adds `use` lines but
never removes them. `radius` alone answers "what would this touch"
without touching it.

## 12. What programs cannot do

For a reader arriving from other languages: Ashlar has no macros, no
user-defined syntax, no operator overloading, and no way to extend the
surface — this reference stays complete. No imports of single names, no
aliases. No exceptions, no `while`, no truthiness, no text interpolation. No
classes or inheritance: layers on parts. No package registry and no version
resolution: dependencies are code vendored into the tree. No dynamic access
to anything with a name: computed keys reach data only. Attempting any of
these is a compile error that names the Ashlar construct to use instead.
