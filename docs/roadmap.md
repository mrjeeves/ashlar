# Roadmap

An honest "not yet" ledger. Each open item names the requirements it will
satisfy and the test that will prove it. A "planned" row anywhere in the
suite tree — including `suites/coverage.md` if and when one exists — is a
debt-ledger entry, not coverage. Open items come first; below them is the
dated record of what was delivered and what proves it, because a requirement
with no passing test is not a satisfied requirement (T-META). An empty open
section is a claim, so it is kept honest: an item leaves it only when its
test runs for real.

## Open — three items

**`data` has no discriminator, so a boundary cannot ask what arrived**
([ADR-0026](decisions/0026-data-is-a-union-with-no-discriminator.md)). Every
value from outside a program is `data`, a union of six members, and nothing in
the language distinguishes them. `number(t)` and `json(t)` already answer "not
that shape" with `none`; there is no such answer for "is this a map", so a body
that is valid JSON but not an object faults on the first index and ends as a
**500 whose message begins `internal:`** — the runtime taking the blame for a
condition the caller chose. Two halves: the missing conversions, and the status
belonging to the caller. Serves **A4, D3, G4**. Proven by: `examples/quarry`'s
driving test, which today asserts the 500 and names the ADR, plus a T-A4 fixture
once the guard exists. Related and cheaper to state than to fix: the idiom the
checker pushes hardest toward at a boundary — `number(text(x)) ?? 0` — launders
bad input into a plausible value, and rejecting costs three times the
characters of inventing.

**A comment between the parts of a one-line expression still has nowhere to
go.** [ADR-0024](decisions/0024-a-formatter-that-loses-code-is-not-a-formatter.md)
closed comment loss inside multi-line list and map literals — the comment now
prints at the item it was written above, and trailing comments stay on their
item's line. What remains is a comment written inside an expression that
prints on ONE line: there is no line to put it back on, so it still moves to
the next construct. Unlike the closed case it is visible rather than silent
(the comment lands somewhere a reader can see), which is why it is an item
here and not a defect. Serves **G1** (the formatter's meaning- and
comment-preserving claim). Proven by: an `assert_fmt_faithful` fixture whose
comment sits mid-expression, with the property extended from comment COUNT
to comment position.

**A provably cold A3 read needs a reader outside this repository.** Run 5
(2026-07-25, `suites/t_a3/results/2026-07-25-sonnet-run5.md`) scored **24/25**
against a bar of 20/25, so **A3 is satisfied** — but it is honestly labelled
*reduced-contamination*, not cold, because an in-repo reader still receives
`AGENTS.md` and so still learns the project's name and that it composes. That
residual cannot be closed from inside: the file is injected before any prompt.
Closing it needs a reader whose working directory is not this repo — a fresh
chat, or a session started elsewhere with the snippet pasted in. Per §1 of the
requirements, that is a case where an agent says what evidence it would need
rather than manufacturing it, so the item stays here.

What run 5 did close:

- **A3 is met by measurement, on the current corpus.** 20 of 25 snippets scored
  4/4 clean; the one failure was unanimous across three lenses.
- **A3-F5 is closed.** `peruser` and `watches`/`updates` — the constructs
  ADR-0019 respelled, whose validation ADR-0021 withdrew — both scored 4/4
  clean, on a run whose isolation was measured and recorded.
- **ADR-0021's argument became evidence.** `08-handle-pipe` is the one fixture
  whose fact the removed AGENTS.md section stated, and it is the one fixture
  that flipped to PASS in run 3 and back to FAIL in run 5. See
  [ADR-0023](decisions/0023-a3-run5-and-the-word-order-behind-f1.md).
- **The isolation rule got sharper because the probes contradicted it.** All
  three found AGENTS.md still stating language facts (refactor command names,
  transports, two banned words). None touches a rubric, so no score moved — but
  the honest rule is narrower than "no language facts": AGENTS.md must not hand
  a reader a fact an A3 rubric asks that reader to produce. That is now what
  `t_meta_agents_md_does_not_teach_the_language` asserts, by intersecting
  AGENTS.md's inline-code spans with the rubrics' vocabulary.

**A3-F1 remains open as a design finding**, now failed in every run whose readers
were not handed the answer (1, 2, 5) — and its cause is finally identified,
because the candidate read **refuted the hypothesis ADR-0023 proposed.** A 2×2
over the merge-kind word and its position, three readers per cell, produced
**0 of 11 readers stating that both functions run** — every cell, both word
orders. So it is not the word and not its position: *cross-file layering itself
does not cold-read*, which is also why `24-composed-program` fails the same way
with no kind word at all. No grammar change follows; the pre-commitment in
ADR-0023 was made before the data arrived and it holds.

One lead is recorded rather than acted on: `handle chain =` produced 0/3 wrong
claims and 3/3 explicit abstention where the control produced 3/3 wrong claims.
Better failure mode, not comprehension, on three readers — respelling a core
keyword on that is the over-fit ADR-0015 committed with `owned`. A future attempt
should test **the layering construct**, not the kind word.

Also recorded, not acted on: **A3-F7**, `{text: number}` read as a one-field
record rather than a map (`17-optional-index`, 3/4, still passing) — a different
misread of the construct ADR-0008 fixed, worth having in hand if a future run
fails there.

One thing stays true regardless and needs no decision: **cold-read the
construct, never the word** (ADR-0015 scored `personal` 3/3 on the bare word; in
its slot it reads as `private`).

Delivered 2026-07-27 — **a `return` is a shape position**
([ADR-0025](decisions/0025-a-return-is-a-shape-position.md), closed in place).
Where a `pipe` property's return shape is fixed from outside the body —
declared, or established by the one nominal data shape its layers name — that
shape is pushed into the property's `return` positions and map literals are
checked against it, as in any argument position. Both halves closed by the one
change, as it predicted: the correct program compiles, and `return { a: "hot" }`
beside `return v` is now `error[E006] a `probe.Reading` needs `n`.` instead of a
clean check and a required field missing from the response — the D3 third
category closed. **`Gate.keep` is gone from `guardrails`**; an identity property
existing to give a literal a shape was the language telling on itself. The
correction points the right way too: seeded by file order it read `Make every
layer return `{text: data}``, deleting the annotation that was right.

Serves **A4, D1, D3**. Proven by five tests in `check.rs`'s agreement suite —
both reproductions, the `none` branch that must stay clean, nested-literal
scoping, and the correction's orientation — plus `suites/t_a4/39-return-shape-drift`.
The bound worth stating: a **list** literal at a `return` is still inferred, and
that narrowness is what keeps the pass from rejecting a correct `return none`.

Removed 2026-07-26 — **`examples/quarry`.** It was a public status board over a
fleet fixed at boot, fed by a simulated sensor this repo wrote: composition
demonstrated honestly, over a domain that does not exist and a structure nothing
outside the program could change. The recursion it advertised — roots, blast
radius, the drawn tree — recomputed the same answer forever, and its cycle guard
was unreachable code written for a condition the example could not produce. An
example that cannot be run for its own sake is a fixture wearing a product's
clothes, and `examples/` claims every entry earns its place, so it came out
rather than being made editable.

What it found stays, because those were real and are fixed with tests:
[ADR-0024](decisions/0024-a-formatter-that-loses-code-is-not-a-formatter.md)
(the formatter changing meaning and then deleting a branch),
[ADR-0027](decisions/0027-a-subscribers-fault-is-that-subscribers.md) (one
subscriber's fault silencing a channel and blaming the publisher), and the
64KiB frame the examples' socket reader could not parse. The open finding it
recorded, [ADR-0026](decisions/0026-data-is-a-union-with-no-discriminator.md),
keeps its reproduction: the hostile-body assertions moved to `slate`'s driving
test, which is a route the outside world writes to.

Delivered 2026-07-26 (third pass) — **`slate`: a shared pad, and the one
problem that makes a pad real software.** The eighteenth example, and the answer
to a fair criticism of the seventeenth: `quarry` demonstrates composition, but
it is a readout over a structure fixed at boot, fed by a simulator this repo
wrote — a dashboard, not an application. `slate` is a product: open a URL, type,
and whoever else has it open sees the text as you write it. No account, no
session, no invite; the URL is the permission.

It exists for **two people typing at once**, which is not optional for a pad and
is not solvable by pretending. The browser shim sends the field's whole value,
so the server never sees an operation — it sees a snapshot, and taking snapshots
at face value is last-write-wins: the slower typist's copy lands on the faster
one's and a paragraph disappears with no error anywhere. So each page keeps the
text it had in front of it as per-instance state, an edit is `(base, incoming)`
against the pad's `current`, and the merge is three-way, line by line. Different
lines both survive; the same line is a conflict where the pad's copy stands and
the writer who lost is told. When any edit lands the pad publishes its new text
on a channel every editor subscribes to, so a page's base moves with its screen;
and a line matching what the pad held one version ago is read as a page that has
not caught up, so a fast typist cannot silently undo a colleague
([ADR-0028](decisions/0028-what-a-snapshot-transport-can-merge.md), which states
what that bound costs as well as what it buys).

The browser runs no editor code: no CRDT library, no operational transform
shipped to the client, no diffing in JavaScript. The merge is about forty lines
of Ashlar. Around it: presence off the socket lifecycle, revisions snapshotted
on a rhythm where restoring is itself an edit and so merges, and two spaces
layering the edit seam — `slate.limits` for size, `slate.history` for snapshots
— ordered by the `use` between them. Proven by
`t_examples_slate_merges_two_people_typing_at_once`, which drives live
co-editing and a crossed keystroke over two real sockets, true concurrency over
the HTTP edit route where both clients state the base they were editing, the
conflict that must be reported rather than swallowed, and the deliberate later
rewrite that must still land so the lag rule cannot freeze the text.

Delivered 2026-07-26 (second pass) — **the open world, which the first pass of
`quarry` assumed away.** The example was built as a clean room: a fixed fleet,
deterministic fake telemetry, and every input well formed — a shape chosen partly
so its own test would not flake, which is the failure the vision names when it
says the unread portion is only safe when changes are provably contained. Driving
it with hostile input found three things, two of them defects in the runtime and
its suite:

- **A subscriber's fault was everyone's**
  ([ADR-0027](decisions/0027-a-subscribers-fault-is-that-subscribers.md)).
  `publish` propagated the first failing handler's fault, so delivery stopped
  silently for every subscriber after it AND the publisher's unrelated request
  ended with that fault's status — a visitor posting a reading got
  `500 division by zero.` from a different visitor's open page. `spawn` already
  had the right rule (§9.9); channels now follow it. Reference §9.5 says so, and
  T-G proves both halves.
- **The examples' socket reader could not read a large frame.** `t_examples`'s
  `ws_read` handled RFC 6455's 2-byte extended length but not the 8-byte one,
  so any frame ≥64KiB desynchronised it permanently and a test simply never
  found what it was waiting for. Measured: with 2 pages open the frames are
  ~44KB, with 8 they are ~134KB, because the runtime broadcasts a patch set to
  every socket. `t_g`'s copy of the helper had always been correct.
- **The boundary launders bad input**
  ([ADR-0026](decisions/0026-data-is-a-union-with-no-discriminator.md)), which
  is open work. `quarry` now refuses instead of defaulting, counts every
  refusal where the board shows it, and asserts the one hostile shape the
  language cannot guard.

The rig was rewritten too: independent uniform draws became a mean-reverting
walk, because a machine that was at 70 a half-second ago is not equally likely
to be at 12 now. Measured over ~600 readings, the old rig produced 8 incidents
and hysteresis changed nothing; the walk produces 1 with hysteresis and 3
without — so both the streaks layer and the recovery mark now do visible work
instead of decorating a coin flip.

Delivered 2026-07-26 — **`quarry`: a layered program with nobody signed in,
and the two formatter defects it found.** `examples/quarry` is the seventeenth
example and the first written as a large program rather than as a demonstration
of one construct: eight spaces in one `use` chain, five of them layering a
single store through all five merge kinds — `classify` and `announce` as
`pipe`s, `boot`/`wind` as `stack` and `stack reverse`, `tags` as `append`,
`limits` as `deep`. Nothing in it authenticates. There is no `signup`, no
`login`, no session and no `peruser`; the complexity is composition, and the
board is the same page for everyone who opens it.

It carries three things no other example had. **`allow` with no identity in
it**: the public report desk guards on program state — a shutter the board can
close — so a closed desk ends the request with 403 before `handle` runs, which
is authorization without authentication. **Static assets** (§9.8): `files =
"manual"` publishes the fleet layout at `/manual/fleet.json`, the first use of
that construct outside T-G. And a **view part that instantiates itself**,
drawing the fleet graph to whatever depth the data has, beside a recursion among
named functions that computes a fault's blast radius from the same `feeds`
edges — locals are single-assignment, so the frontier and the answer are
parameters, and the walk carries fuel because a graph that arrives as data can
cycle where a `use` graph cannot. Proven by
`t_examples_quarry_is_a_public_board_with_no_login`, which drives the layered
escalation (two thresholds readings, then a streaks escalation on the third),
the 403 at the closed desk, the 404s, the recursive tree, the asset and its
traversal guard, cross-client reactivity over a socket, the alert channel, and
`stored` surviving a restart.

Writing it found two silent defects in `ashlar fmt`
([ADR-0024](decisions/0024-a-formatter-that-loses-code-is-not-a-formatter.md)),
both fixed here with regression tests. `else if` in EXPRESSION position printed
as `else { if ... }`, which is a statement block whose value is `none` — so the
first pass changed what the program returned and the second deleted the branch
outright (`else {  }`), with `ashlar check` clean throughout. And a comment
inside a multi-line literal migrated onto the next declaration, where it
described something else; the comment count was preserved, which is why the
property test never saw it. The formatter's property corpus — same AST,
idempotent, comments preserved — now runs over `examples/` as well, because
`t_examples` only ever asserted `fmt(src) == src` on files that were already
canonical and so could not see a construct the formatter mangles until one was
committed.

It also found what [ADR-0025](decisions/0025-a-return-is-a-shape-position.md)
records and the open section now carries: a `return` is not yet a shape
position.

Delivered 2026-07-25 — **the showcase is an Ashlar program.** `examples/gallery`
replaces `showcase/index.html`: a sidebar of the other fifteen examples with a
live frame, on port 8080, launched by both `serve.sh` and `serve.ps1` alongside
everything else. The `file://` step is gone, and so is the hand-written page —
the quickstart is now one command and one URL.

This is what settings were for, and the proof is a negative: the program renders
fifteen addresses and **its source contains none of them.** `Catalog` declares
`setting groups: [Group]`; deployment supplies `examples/gallery/settings.json`;
starting without it refuses by name (`gallery.Catalog.groups : [Group]`) instead
of serving dead frames. `t_examples_gallery_frames_a_chosen_example` drives the
whole path — every example present in the sidebar, **no address in the page
before a click**, then a click over the socket patching in
`src="http://127.0.0.1:8081"`, and a final assertion that `gallery.ash` contains
neither `http` nor `127.0.0.1`. B5's scan now covers `examples/` too (comments
stripped — a comment binds nothing), so a location in an example's source is a
failing test rather than a code review.

Writing it found a real contradiction the reference had carried for as long as
both sentences existed: §7 said a function literal may not be "stored in a list,
map, or field", §9.4 said an attr value may be "an inline function", and an attr
map is a map. The resolver enforced §7 and the renderer implemented §9.4 — so
`onclick: (e: std.Event) => pick(s)` was documented, fully built, and rejected
by `E024`. It went unnoticed because every example that needed a handler over a
list item introduced a child part instead, which works; the gallery is the first
program where a whole part per sidebar button is absurd.
[ADR-0022](decisions/0022-a-function-is-either-named-or-handed-over.md) resolves
it in favor of §9.4 and states the rule §7 was reaching for: a function is
either **named** (a property value) or **handed over** (inside a call's
argument, including a literal written there) — never *stored*. `let`, fields,
returns, and a property's own map literal stay errors, pinned in both directions
by `t_b_a_handler_may_be_inline_in_attrs_but_never_stored`.

Delivered 2026-07-25 — **the A3 gate's isolation, and the two runs it invalidates.**
Every in-repo agent is handed `CLAUDE.md` → `AGENTS.md` before it sees a prompt,
and AGENTS.md carried a section stating the syntax facts agents get wrong. The
A3 readers are in-repo agents. So runs 3 and 4 read their snippets with rubric
answers already in context, and their scores are withdrawn
([ADR-0021](decisions/0021-the-a3-readers-were-not-cold.md)); what survives is
listed under Open above. The syntax moved to `docs/writing-ashlar.md` — a link,
never an `@`-import — and the invariant became a test rather than a resolution:
`t_meta_agents_md_does_not_teach_the_language` reads the reserved-word list out
of `lexer.rs` and fails on one backticked keyword, one fenced ```ash block, one
`std.` reference, or one `@`-import in AGENTS.md. `PROTOCOL.md` step 1 now names
project instructions as context a reader may not have and requires the isolation
to be *reported and recorded* per run, so the next leak shows up in the results
file instead of in an audit two days later.

The uncomfortable part is worth writing down: the gate had found two real design
bugs and the repo had rewarded itself with `25/25`. The findings were sound —
contamination inflates scores, so a fail under contamination is a stronger fail
— but the number was not, and it took reading the harness rather than the
results to see it. A gate that grades its own isolation grades nothing.

Delivered 2026-07-25 — **settings: a program may depend on a value it cannot
know.** The showcase is a page of links to running examples, and writing it in
Ashlar was impossible: B5 banned a location from source and `std` has no file
I/O, so a list of addresses could only arrive across the `foreign` boundary. The
language demanded a Python co-process to know a port number. B5 is revised to
forbid *binding* by location — what the vision actually asks — and the `setting`
construct supplies the rest
([ADR-0020](decisions/0020-settings-and-what-b5-actually-forbids.md)):

```
setting endpoint: text        // required — no default
setting retries: number = 3   // optional
```

A shape is mandatory, because it is the only thing that can check a value which
does not exist yet; `setting` never combines with `state`/`stored`; and values
live in `settings.json` at the project root or at `ASHLAR_SETTINGS`, keyed by
full property name — the third instance of the `foreign.json`/`--port` pattern,
not a fourth mechanism. Reading one is an ordinary property read, so `rename`
and `radius` already worked over settings the day they existed.

One new diagnostic id (`E030`: no shape, or combined with a storage word) and no
others: an unknown key in `settings.json` is `E001` and a wrong value is `E006`,
exactly as `foreign.json` keys are. Proved by
`t_g_missing_required_setting_refuses_before_serving` — startup names **every**
gap with its shape at once and refuses before binding a port, a supplied value
reaches the response, and a default is overridden without touching source — plus
four unit tests in `settings.rs` and check-time validation in `check_project`.

Delivered 2026-07-25 — **a fresh clone builds on an old toolchain.**
`./showcase/serve.sh` on a real machine failed before compiling anything:
`failed to parse lock file ... version 4 requires -Znext-lockfile-bump`.
Lockfile v4 needs Cargo 1.78+, and this workspace has **zero dependencies**, so
that lock format bought nothing and locked people out. Nothing declared a
minimum version either, so there was no way to tell whether a toolchain was even
supposed to work.

`Cargo.lock` is back to version 3 (verified that cargo 1.94 does not rewrite it),
and the crate declares `rust-version = "1.65"` so cargo prints a sentence instead
of a mystery. The floor was **measured, not guessed**: old toolchains were
installed and the suite run on each. 1.60 fails on `let ... else` (used ~95
times); 1.65 builds and passes all 299 tests the suite then held, with zero
warnings; 1.70 and 1.74
likewise. One test of mine had quietly raised the floor to 1.70 by using
`is_some_and` — rewritten, since a convenience in an assertion is not worth five
minor versions of reach.

Building on the floor also caught a real defect the current compiler no longer
mentions: rustc 1.74 flagged a dead assignment in `http.rs`'s reload check
(`last_mtime = m` immediately before `break`, where reload restarts the outer
loop and re-reads the mtime anyway). It was right, and 1.94 says nothing — so
"zero warnings" now means on the floor too, where an older rustc is often the
stricter reader.

Pinned by `t_meta_toolchain_floor_is_declared_and_reachable`: the lockfile must
stay at version 3 with no dependency entries, `rust-version` must be declared,
and it must be ≥1.65 (because `let ... else`) and not silently drift upward —
raising the floor strands users, so it has to be a deliberate act that updates
the test and the README together.

Delivered 2026-07-25 — **the runtime builds and the showcase starts on
Windows.** Asked how to start the showcase, the honest answer turned out to be
"you can't" — `serve.sh` is bash, and underneath it `dlopen`/`dlsym` were
declared with no `cfg` gate at all, so the binary could not link on Windows and
the reference's promise of a `.so`/`.dylib`/**`.dll`** derived path was one no
build could keep. They were also the *only* platform-specific lines in the whole
workspace; everything else was already portable.

Fixed at the boundary rather than papered over: the POSIX loader is now confined
to `open_library`/`lookup` behind `#[cfg(unix)]`, and without it the `native`
transport refuses with the correction that matters — bind the space to a `worker`
or `http` transport, both of which need nothing but std and carry the same
envelope. That is complete behavior, not a stub, and it is ADR-0017's own
principle applied to a platform. The reference no longer promises `.dll`, and
`showcase/serve.ps1` is the PowerShell twin of `serve.sh`.

Three copies of the name-to-port map now exist (both launchers plus
`index.html`, which must work from `file://` with no fetch), so
`t_examples_showcase_launchers_agree_on_every_port` makes drift impossible
instead of asking nicely in a comment — it also asserts the gallery launches
exactly the examples that exist, and that no two share a port. Writing it caught
a bug in its own parser first: pong's blurb contains "20fps", whose digits
joined the port.

**What is verified, and what is not.** The unix path is unchanged and proven —
`ledger`'s native SQLite transport and `abacus`'s worker both still pass their
runtime tests, and `foreign check` reports both reachable. The non-POSIX branch
was compile-checked by temporarily inverting the `cfg` gates and building it
here, so the code a Windows build takes does compile. Not verified: an actual
Windows build or run, and `serve.ps1`'s PowerShell syntax — this machine has
neither a Windows Rust target nor `pwsh`. Confirming those needs a Windows
machine, and until someone runs it there, that is the claim.

Delivered 2026-07-25 — **the two A3 run-3 findings are fixed in the language**
(ADR-0019). `owned` → `peruser` and `reads`/`writes` → `watches`/`updates`,
across the reserved-word list, tokens, parser, AST and resolved models, the
composer's storage identity, the formatter, the evaluator's per-user scoping and
its two runtime faults, `E029`'s cause and machine fix, reference
§1/§4/§9.3/§9.10, the diagnostics catalog, G4, the `locker` and `ledger`
examples, and the A3 fixtures. `owned`, `reads`, and `writes` are ordinary
identifiers again — `commons` already declares a property called `reads`, which
is now a live proof of it. E029's machine fix still converges in one round
(D2), and all 15 examples check clean and canonically formatted.

Delivered 2026-07-25 — **the binding file is a name-bearing fact, and the
name-governing systems now see it.** An audit of ADR-0017 against the vision
found one root cause with three symptoms: `foreign.json` keys bindings by SPACE
NAME, and none of the three systems that govern names knew the file existed.
`rename` rewrote the sources, left the key behind, and the program still checked
clean while the capability silently fell back to the derived native path — a
stale reference E2 forbids, found only when a request reached the boundary.
`check` ignored a key that named no space, which B3 makes an error for every
other name in the language. And the manifest recorded the derivation rule
instead of the resolution, so a worker-backed space was written down as a native
library that did not exist — a fiction in the file whose whole job is being the
derived truth ("the build is state"). Fixed together: the key and all three
probed library extensions are radius and are carried atomically and reversibly
(E2/E3/E4); an unbound key is `E001` naming the nearest space, and an
unparseable binding file is reported rather than silently becoming the default,
while an inert binding stays silent (no false positives); the manifest records
the transport that won and what it names. Pinned by a T-E end-to-end rename
proof, a T-B resolution test covering all four cases, a T-F manifest test, and
unit tests for the depth-aware key scanner. Two reference sentences and one ADR
paragraph that were false are now true, and D3's silent third category closed:
foreign reachability and `owned`-with-no-user each now state in the reference
WHY they are runtime facts rather than build-time ones.

## Everything else is delivered — 2026-07-22

Every item this page has carried is delivered, tested, and moved off:

- **Shape checker** — `check.rs`, proven by its unit suites, T-A4
  fixtures 31–38, and every reference example checking clean (T-A2).
  The D3 inventory it left behind (stack/pipe cross-layer agreement,
  route capture rules, deeper inference for unannotated recursion)
  closed in increment 9: pipe layers must agree in parameter and return
  shape, stack return literals must name state properties with fitting
  values, captures must be legal names bound once, and `-> ?` returns
  refine from concrete branches so recursive callers check (ADR-0009).
- **Evaluator and runtime** — `eval.rs` + `http.rs` + `ashlar run`,
  proven by T-G's runtime conformance tests (G2 byte-identity, G3 hot
  reload, multiplexed sockets, cross-client reactivity, foreign binding
  with runtime shape faults). Its residual list emptied in increment 8;
  the conformance pass then closed §9.5's instance lifecycle (start
  stacks on mount, page-scoped unmount with subscription cleanup),
  §9.1's root selection (`run <part>`, candidates listed when
  ambiguous), and `fix <id>`. Hardened 2026-07-23 against real browser
  socket behavior: requests assemble and responses drain without ever
  blocking the loop (a speculative preconnect socket that sends nothing
  once froze the whole runtime), outbound WebSocket frames queue per
  connection and shed peers by time-without-progress — never burst
  size — and an oversized body gets a 413 naming the limit instead of a
  reset. All of it pinned in T-G with hostile-socket tests. The view
  model was made AI-first the same day (ADR-0011): a view instance is
  its own root element (no wrapper breaking a parent's CSS layout), and
  nested views reconcile by position so per-instance state and
  subscriptions survive re-renders and `start`/`stop` fire once — the
  fix a flagship of parts-in-parts demanded.
- **Refactor commands** — `refactor.rs` + `rename`/`rekind`/`move`/
  `radius`, proven by T-E's 21 tests. The E6 residuals closed in
  increment 9: data-shape and view fields rename through the checker's
  field-site index; spaces rename as pure prefix substitution; `move`
  relocates a home declaration with `use`-graph additions and a stated
  E4 class (ADR-0009); `stored` keys migrate with their names, closing
  ADR-0007's orphaned-rows note. `vendor` landed with
  refuse-before-copy and roll-back-after semantics.
- **`ashlar fmt`** — comment-preserving canonical formatter with
  AST-preservation, idempotence, and comment-count properties enforced
  over the whole corpus.
- **F1 incremental latency** — hard sub-100ms release gate at 1,000
  files; currently 40ms incremental.
- **D5 round-trip metric** — one check → apply-machine-edits cycle;
  mean rounds-to-clean 1.00 over every machine-fixable fixture.
- **A5 reference budget** — under 40,000 bytes with the distribution
  printed on every run; §9.10 is the largest construct as the boundary
  grew (ADR-0017), still far under the 20% per-construct cap.
- **T-A3 surface findings** — the run-1/2 findings resolved by ADR-0008,
  validated by gate run 2 (23/24, and the last provably clean run). Run 3
  raised two more findings, both fixed by ADR-0019; runs 3 and 4 are void as
  cold reads (ADR-0021), so their scores are withdrawn and a full re-run is
  the one item open above. The findings survive; the numbers do not.
- **Showcase corpus** — fifteen complete projects, crowned by
  `commons`: a full team chat (auth, rooms, DMs, live messaging,
  presence-by-lifecycle, unread counts, plus moderation and mentions as
  independently owned layers) that exercises the whole language as one
  product, styled by a named sheet (ADR-0010). `ledger` is the first to
  exercise the `foreign` boundary for real: its datastore is a genuine
  SQLite database file, reached through a std-only cdylib shim that links
  the system libsqlite3 — the SQL lives outside Ashlar, the way CSS does.
  T-Examples compiles, format-checks, serves, and drives every project —
  commons and ledger included — over its real HTTP/WebSocket surface (the
  ledger driving test builds its shim and skips loudly where libsqlite3 is
  absent, since a SQLite integration cannot be tested without SQLite).
- **Deployment posture** — the binary is an origin; TLS and HTTP/2/3 are
  terminated at a reverse proxy (ADR-0013). The origin carries only the
  small correct pieces to sit behind one: `stored` state flushes
  atomically (temp + rename, so a crash never truncates it), and the
  session cookie is `HttpOnly` + `SameSite=Lax`, gaining `Secure` when
  `X-Forwarded-Proto` reports TLS. Both pinned in T-G.

What remains is not debt but doctrine, named where it lives:
`Unknown`-permissiveness for what the checker cannot prove (no false
positives, check.rs module docs) and reversal-as-property rather than
byte-identity-as-law (ADR-0018, generalizing ADR-0009's `move` trade). (The
once-weak v1 password hash is gone: v2 is salted, iterated PBKDF2, and v1
hashes upgrade transparently on login.) New requirements enter here as new
numbered items; the open ones are listed at the top of this page.

One proposed trajectory is partly delivered: **ADR-0014** sketches the data
layer beyond the `foreign` shim the `ledger` example demonstrates — a
database backend for `stored`, a hand-rolled non-blocking Postgres client
that never blocks the single loop, and horizontal scale by process count.
Delivered so far: Stage 1 (the SQLite-over-`foreign` example) and, on
2026-07-24, **reactivity for a foreign store** — a `reads <Shape>` /
`writes <Shape>` annotation on `foreign` that joins the SQL collection to the
§9.3 reactive graph (the collection is the table, the Shape is the schema).
`recent`/`total` `reads Entry`, `record` `writes Entry`, so a write from any
client patches every open `ledger` board live — no new threads, no `stored`
backend, a typo'd collection caught as E001, and a T-Examples test that drives
the cross-client patch. The `stored` database backend, the Postgres client, and
horizontal scale remain unbuilt — not blocked on a decision, since the design is
this page's to make, but un-started: no requirement compels them and nothing in
the suite is failing for want of them. They are speculative scope, and scope is
the one thing the hierarchy does not hand you.

Delivered 2026-07-24 — **ADR-0015** re-cut the storage taxonomy along its
two real axes. `synced` is retired: the runtime never gave it any behavior
`state` lacks, since no-client-code makes cross-client reactivity
universal. `owned` is added, a per-user scope modifier on `state`/`stored`
— each authenticated user's own value, isolated by construction, so the
manual `[req.user.id]` keying that invites IDOR disappears. It fails loud
where there is no user (an anonymous request, a scheduled task, `spawn`, or
`start` stack): a runtime fault, never a silently shared value. The word
was chosen by a T-A3 cold read of the WORD (`owned`/`personal`/`user` all read
per-user 3/3; `private` misread as OOP access-control) — a method ADR-0019 later
showed to be the wrong unit of measurement, respelling the keyword `peruser`. Shipped with the runtime
scoping, per-user persistence keyed by the stable account id, `E029`
(`owned` needs a storage word), the `ticker` rename, the `locker` example
(two users, isolated and persisted, driven by the suite), a T-G fault
proof, and the reference/G4 rewrite. One refinement stays named: catching
the no-user case at COMPILE time in provably user-less contexts
(task/boot/`spawn`) — the runtime fault already secures correctness; a
static check would only move the failure earlier.

Delivered 2026-07-24 — **the foreign boundary meets foreign systems where
they are** (ADR-0017). A `foreign` declaration now names a capability; how it
is reached is a deployment fact. Three transports carry one JSON envelope:
`native` (dlopen, now with arbitrary library paths, **symbol aliases** so an
existing library keeps its own names, `.so`/`.dylib`/`.dll` probing, and an
optional `ashlar_free` that ends the returned-buffer leak), **`worker`** (a
co-process speaking JSON Lines — any language, no compiler, no C ABI, no
shared library), and `http` (plaintext, TLS at a proxy per ADR-0013). An
optional `foreign.json` overrides the derived path per space; with no file,
every existing project resolves exactly as before. A shared result convention
(`{"error": …}` faults, `{"ok": …}` is the escape hatch, a bare value is the
result) makes a foreign failure diagnosable instead of "malformed JSON", and
`ashlar foreign check` proves every declared name reachable — dlsym for
native, protocol handshake for a worker — turning a runtime fault into a
build-time correction. The whole boundary moved to `src/foreign.rs`, which now
confines the workspace's only `unsafe`. Proven by four T-G conformance tests
(one per transport plus alias/free) and the new **`abacus`** example, whose
capability is ten lines of Python. Deliberately rejected and recorded: native
C type marshalling in source, WASM, command templating, a code-generating
`foreign scaffold`, and transport failover.

Delivered 2026-07-24 — **the examples wear a design** (ADR-0016). Every
project now declares a stylesheet in one restrained dark house language (the
`commons` palette, re-declared per project since the runtime serves one sheet
each, bound by `class` name); the four former API-only demos — `diary`,
`press`, `guardrails`, `locker` — grew a small `/` view that shows their idea
in the browser (a live pipe preview, a live policy verdict, a login gate, a
per-user board), each driven by `t_examples`. A top-level `showcase/` runs all
fifteen at once and flips between live frames — a launcher, not a baked
gallery, so the running app stays the only source of truth. Making that work
without editing any example's `port` added `ashlar run --port N` — a
deployment fact bound at run time (B5, reference §9.1/§11), pinned by
`cli::parse` tests — and a viewport `<meta>` now ships in every served head so
the pages are legible on a phone. No language rule changed; the suite stays
green in debug and release.
