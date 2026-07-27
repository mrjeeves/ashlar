# Ashlar

> **A language for code too abundant to trust by inspection.**

The software industry spent seventy years optimizing for a world in which
code was expensive to produce.

That world is ending.

A machine can now raise a city of software before a person has inspected one
street. Keystrokes are no longer scarce. Neither are files, functions,
services, or plausible implementations. The scarce things are
**comprehension, verification, good design, and safe change**.

Ashlar begins at that threshold.

It is an AI-first composition language whose primary author is a machine and
whose primary reader is a human reviewing the machine's work. Its first
runtime builds servers and interfaces. That is the present target of the
runtime, not the identity or permanent boundary of the language.

Ashlar does not ask agents to become infallible. It asks the language, build,
toolchain, runtime, and tests to make their fallibility legible.

This is what **AI-first** means here. Not autocomplete. Not a familiar language
with an agent bolted on. A language designed around the cost structure of
generated abundance:

- make the surface small enough to hold at once;
- make one construct predict the next;
- make names carry every binding the toolchain must understand;
- make incorrect guesses stop the build instead of changing the program;
- make every change expose its complete semantic reach.

Code is cheap.

Good design is not.

## The whole language begins with one thing

Ashlar has one composable unit: the `part`.

A route is a part with a `route`. A server is a part with a `port`. A view,
service, state store, scheduled task, static asset, and data shape are parts
distinguished by the properties they bear, not by separate declaration
systems.

```ash
space hello

part app {
  port = 8080
}

part greet {
  route = "/"
  handle pipe = (req: std.Request) => "hello from ashlar"
}
```

The apparent diversity of the running system is built from one composable
category and one discipline: declarations with the same name become one
thing.

That gives Ashlar its signature move. A space may extend a part it can see,
from another file, without editing the part's home:

```ash
space chat.audit
use chat.data

part chat.data.Store {
  prepare pipe = (body: text) => {
    log.info("prepared", { size: len(body) })
    return body
  }
}
```

No file owns the completed part. Its being is distributed across every space
that names it. The build flattens those declarations in the order already
expressed by the `use` graph. The result is inspectable, deterministic, and
independent of file location.

An ashlar is a stone cut precisely enough that the fit between stones is the
joint. In this language, **the fit between names is the joint**.

The unit is the part.

The joint is the name.

The proof is the build.

## Source is an ontology of intent

Names are the only binding mechanism.

Not paths. Not argument positions. Not declaration order. Not the directory a
file happened to land in. A dotted identifier such as `chat.data.Store` is a
name, not a route through the filesystem.

Source says what should be true. The build computes what exists: where
declarations live, which names they denote, how layers flatten, which foreign
binding won, and what a change can affect. Delete the manifest and the build
derives it again. Move a source file and only the recorded location changes.

This is the doctrine written across the entire project:

> **The build is state. The code is intent.**

The [philosophical version](docs/ontology.md) is grander and more exact:
Ashlar source is an ontology of intent; the whole system is an ontology of
being; and the discipline of the project is that the second remains derivable
from the first.

The distinction matters because generated code will exceed what any reviewer
can read. Trust in the unread cannot come from confidence in its author. It
must come from computable dependence.

## Composition is legislated

Ashlar permits exactly five merge kinds:

- the unmarked default replaces;
- `append` joins text and lists and merges maps one level;
- `deep` merges maps recursively;
- `stack` runs every layer and merges returns onto state;
- `pipe` runs every layer and passes each result to the next.

A sixth is added only by removing one.

This austerity is deliberate. Every additional way to combine code is another
meaning an author may imply, another guess a reader may form, another branch a
compiler must explain, and another semantic delta a refactor must contain.

The merge kind belongs to the property. Every layer that touches the property
must restate it. A layer cannot silently turn a pipeline into replacement,
change teardown order, or invent a new composition rule. If two spaces leave
their order arbitrary, the compiler still produces a deterministic result—but
it confesses the arbitrary choice and supplies the declaration that would make
the order intentional.

Determinism alone is not enough. A repeatable accident is still an accident —
so a `use` edge that resequences any part's layers raises `W002` naming the
order before and after, and `ashlar delta` prints the whole report. A silent
reordering was a real defect here, not a hypothetical
([ADR-0032](docs/decisions/0032-observability-was-decided-and-never-built.md)).

## A world may be strange, but it may not deceive

Ashlar treats intelligibility as a design constraint.

A fixed corpus shows fresh readers valid Ashlar without its reference and asks
what the code means. A misread is evidence against the surface, not a failure
of the reader. Candidate syntax is read in place, with its neighboring tokens,
because words do not carry meaning alone. The project has changed language
words when the naive reading pointed toward a security error, and it has
refused to change them when the evidence did not justify the disruption.

Where correct cold reading is impossible, the wrong reading must fail loudly.
Plausible constructs imported from neighboring languages are kept as an
adversarial corpus. Semicolons, hash comments, imports, classes, truthiness,
shadowing, interpolation, and illegal overrides do not run with surprising
Ashlar meanings. They stop with a correction.

False familiarity is worse than unfamiliarity.

An unfamiliar language sends the reader to the reference. A deceptively
familiar one sends the bug to production.

The complete contract lives in [`AGENTS.md`](AGENTS.md) — one file carrying
the working contract and the language reference together, under a hard
40,000-byte ceiling covering both (ADR-0031). Anything it does not define is a
compile error, never a secret feature.

## Semantic freedom is not free

Traditional languages often maximize the number of valid ways to express an
idea. Ashlar minimizes that freedom in order to maximize something more
valuable for agent-authored systems: **derivability**.

The toolchain must be able to compute:

- what every name denotes;
- why every layer runs where it does;
- which behavior changes when an intent changes;
- every source, state, deployment, and foreign-binding site a refactor touches;
- whether a correction can be applied without judgment.

Aliases, dynamic name construction, operator overloading, user-defined syntax,
selective imports, a package registry, and multiple competing composition
systems all enlarge the space of possible meanings. Ashlar declines them.

That does not mean every change is local. A change may cross hundreds of files
and still be safe when its complete semantic delta is calculated, announced,
applied atomically, and verified. A one-line edit is unsafe when its
consequences depend on convention or hidden state.

The governing trade is not simplicity against complexity.

It is **semantic freedom against derivability**.

## The compiler does not complain; it corrects

Diagnostics are structured JSON Lines first and human prose second. Every
diagnostic carries a stable identity, the requirement it enforces, a source
location, a one-sentence cause, and a correction specific enough to apply
without judgment.

When the compiler includes machine edits, applying them must remove the error
without introducing another **and leave the program meaning what it meant** —
the stronger half, learned when `ashlar fix` once rewrote an ambiguous name to
the wrong part and silently changed what a page rendered
([ADR-0032](docs/decisions/0032-observability-was-decided-and-never-built.md)).
Both promises are executed as tests. The quality of the compiler is measured in
compile-to-clean rounds, which converge in a mean of 1.00 — over the 24% of the
diagnostic corpus that carries a machine-applicable fix at all. The rest name
every candidate and leave the choice to the author, because picking one would
be a guess wearing a correction's clothes, and that number is printed on every
run so it cannot fall quietly.

```sh
ashlar check examples/chat
ashlar check examples/chat --human
ashlar fix
```

The default output is for the machine that wrote the program. The optional
rendering is for the person reviewing it. The structured truth comes first.

## Refactoring is part of the language

Generated abundance is safe only when change is contained.

Ashlar therefore treats refactors as toolchain operations, never as global
text replacement:

```sh
ashlar radius chat.data.Store
ashlar rename chat.data.Store Ledger
ashlar rekind chat.data.Store.prepare pipe
ashlar move chat.data.Store archive
```

The toolchain computes and reports the complete blast radius before writing,
updates source and name-bearing deployment files together, migrates persisted
keys when names move, applies atomically or not at all, and refuses work whose
boundary it cannot prove.

Reversal restores the program, not a fetishized arrangement of bytes. Commands
that can promise byte identity do and are tested for it. Commands that must add
a declaration preserve meaning and report the addition. This distinction
exists because the project once made byte identity universal, found that the
rule forced safe work back into unsafe text editing, and revised the
requirement instead of defending the slogan.

That is Ashlar's method in miniature: keep the principle, test the mechanism,
and let evidence expose when the mechanism has been mistaken for the
principle.

## Two loops govern the work

Ashlar has a [constitution](docs/vision.md):

```text
VISION          Fixed. If it is wrong, stop.
REQUIREMENTS    Revised when they fail to express the vision.
TESTS           The current best proof of the requirements.
CODE            Whatever makes the tests pass.
```

Code yields to tests. Tests yield to requirements. Requirements yield to the
vision. Nothing overrides the vision.

That hierarchy runs through two loops.

**The inner loop builds.** Name the requirement. Write the proof. Build until
it passes. If the proof cannot pass, question the requirement against the
vision instead of weakening the test to spare the code.

**The outer loop discovers what the requirements forgot.** Build a whole
program. Execute it over the transports it actually serves. Give it values its
author did not choose. Put it in a real browser. Let two people type at once.
Pull the process down with a real signal. Then read what happened.

The expensive findings have come from that outer loop: correct data rejected
by the checker, incomplete data accepted, comments reassigned by the
formatter, malformed caller input blamed on the server, simultaneous edits
lost, subscriber faults silencing other subscribers, lifecycle hooks that
were documented and never ran, sockets that died while pages still looked
alive.

All of those lived in a repository whose existing tests were green.

A green suite proves the contract the project knew to ask for.

Execution against an uncooperative world discovers when the contract itself
is wrong.

## See it before believing it

With Rust 1.65 or newer:

```sh
git clone https://github.com/mrjeeves/ashlar.git
cd ashlar
./showcase/serve.sh
```

On Windows, or anywhere with PowerShell:

```powershell
./showcase/serve.ps1
```

The launcher builds the release binary and starts the example corpus, each
program on its own port. Open **<http://127.0.0.1:8080>** for the gallery. The
gallery is itself written in Ashlar and frames the running programs, not
screenshots or copied HTML.

Most examples need only Rust. `ledger` crosses the native boundary into SQLite
and needs the SQLite development package (`libsqlite3-dev` on Debian/Ubuntu,
`sqlite-devel` on Fedora). Without it, the launcher skips that program loudly
and starts the rest.

Run one project directly:

```sh
cargo build --release
target/release/ashlar run examples/chat
```

## One program, from server to browser

Views render on the server. The browser runs a transport shim, not a second
application. Events travel over the built-in socket, handlers execute against
the same named state as routes and tasks, and every view that read a changed
value re-renders and patches in place.

```ash
part tally {
  label: text
  state n: number = 0
  view = () => el("button", { class: "count", onclick: bump }, [
    label + ": " + text(n),
  ])
  bump = () => {
    n = n + 1
  }
}
```

Shared state reaches every connected reader. `peruser` state reaches only its
authenticated user. Persisted state is checked against its current shape on
startup. Hot reload carries state properties by full name, reconnects open
pages, and keeps the old program alive when new source does not compile.
Per-page view-instance state is reborn because its page is gone; the
requirement says that now because a real browser proved it.

The builtin runtime covers routing, HTTP and WebSocket handling, reactive
views, sessions, authorization, persisted and in-memory state, per-user scope,
channels, schedules, background work, structured logs, files, settings, and
hot reload.

It ships as one Rust binary with zero external crates.

## The boundary admits that there is an outside

Ashlar is a closed language, not a universe pretending nothing exists beyond
it.

Everything outside the builtin set crosses one `foreign` boundary. Source
names a capability and its shape. Deployment chooses how to reach it: a native
library, a long-lived worker in any language, or an HTTP service. The program
does not change when the transport changes, and values are shape-checked where
they cross.

The boundary is intentionally honest. The compiler can derive the name,
shape, binding, and reactive consequences; it cannot derive the behavior of
opaque foreign code. Ashlar marks that limit instead of dissolving it into
unchecked calls throughout the program.

Likewise, the binary is an origin server. TLS and modern HTTP terminate at a
reverse proxy rather than inside a hand-written cryptographic stack. A
zero-dependency rule is not permission to counterfeit expertise.

Constraint is valuable only while it serves the vision.

## Proof is part of the product

The repository does not keep one test suite and call the matter settled. Its
proof surface includes:

- the bounded and executable language reference;
- cold-read comprehension and plausible-wrong-syntax corpora;
- exhaustive resolution and composition checks;
- machine-applied diagnostic corrections;
- blast-radius, atomicity, migration, and reversal checks;
- manifest reproduction, relocation invariance, and incremental latency;
- runtime conformance over HTTP, sockets, foreign transports, reload, and
  shutdown;
- the complete example corpus compiled, formatted, served, and driven;
- hostile-input sweeps over every declared route;
- a separate real-browser gate for the failures an in-tree client cannot see;
- a meta-test that proves every requirement has running evidence and that the
  coverage map itself is telling the truth.

The exact map from requirement to proof is
[`suites/coverage.md`](suites/coverage.md). The method is specified in
[`docs/requirements.md`](docs/requirements.md). The evidence behind changed
beliefs is preserved in [`docs/decisions/`](docs/decisions/).

A requirement without a test is not a requirement.

A metaphysics without a test is marketing.

## The toolchain

| Command | Purpose |
|---|---|
| `ashlar check` | Compile and emit JSON Lines diagnostics; `--human` renders prose. |
| `ashlar fix [id]` | Apply machine-safe corrections from the last check. |
| `ashlar build` | Check the program and derive its manifest. |
| `ashlar run [part] [--port n]` | Build, serve, watch, and hot-reload. |
| `ashlar fmt` | Produce canonical, comment-preserving, meaning-preserving source. |
| `ashlar radius <name>` | Report a rename's complete blast radius without changing a byte. |
| `ashlar delta` | Report what this tree changed about the program's derived state since the last build. |
| `ashlar rename <name> <new>` | Rename a space, part, property, or field atomically. |
| `ashlar rekind <part.prop> <kind>` | Change one property's merge kind across all layers. |
| `ashlar move <part> <space>` | Move a part and repair the visibility graph. |
| `ashlar vendor <source>` | Copy an external source tree into the program; there is no registry. |
| `ashlar foreign check` | Prove declared foreign names are reachable on this deployment. |

## The record is allowed to contradict itself

Ashlar is not finished by proclamation.

The decision record preserves mistakes alongside corrections: a storage word
chosen by an invalid experiment; “cold” readers who had already been handed
the answers; a location rule that accidentally forbade configuration; a
formatter that preserved comment count while moving meaning; a shutdown
guarantee with no signal path; a purity rule relaxed when it protected the
count instead of the capability.

These are not embarrassments edited out of the story. They are the story.

The project does not claim that its first principles mechanically produce
perfect requirements. It claims something more credible: evidence has
authority, the hierarchy says what must yield, and every corrected belief
leaves behind a test.

The current unfinished boundary lives in
[`docs/roadmap.md`](docs/roadmap.md). Delivered work is absent from that file
on purpose; it already has better homes in tests, decisions, and history.

The ambition is grand.

The accounting is exact.

## Follow the paper trail

[`docs/README.md`](docs/README.md) gives the reading order from the fixed
vision through requirements, evidence, decisions, philosophical edges, and
the open ledger. Agents working in this repository begin with
[`AGENTS.md`](AGENTS.md).

## Build what can be believed

Ashlar is not trying to predict every mistake an agent will make.

It is constructing a world in which mistakes have fewer places to hide; in
which names carry identity, builds carry reasons, diagnostics carry repairs,
refactors carry their radius, and a running program must survive contact with
clients it did not write.

Code will keep getting cheaper.

The future belongs to systems that make cheap code trustworthy.

## License

Ashlar is released under the [MIT License](LICENSE).
