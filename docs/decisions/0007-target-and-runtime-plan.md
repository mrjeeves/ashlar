# ADR-0007: Rust host, zero external crates, server-rendered runtime

Date: 2026-07-22

Status: accepted

## Context

G1 requires a single binary with no install step and no runtime dependency
resolution. G5 requires the *absence* of a package registry to be treated
as a requirement, not a gap — removing version resolution, transitive
conflict, and supply-chain surface as pure cost to an agent author.
Requirements §11 leaves the compilation target (bytecode vs. native)
unspecified, binding only F1 (a single-file change in a 1,000-file project
must re-check in under 100ms). G2 and G3 require that the same handler serve
HTTP and WebSocket with transport invisible in handler code, and that hot
reload preserve process state — both of which are runtime-architecture
decisions, not surface-syntax ones, so they belong here rather than in the
reference.

## Decision

**Host language: Rust, zero external crates.** The compiler and runtime are
a single self-contained binary (satisfying G1 directly), and the zero-
dependency policy is deliberately the same shape as G5 applied to the
implementation itself: no version resolution, no transitive conflict, no
supply-chain surface for the tool that enforces those properties on user
programs. `crates/ashlar/Cargo.toml` records this as an explicit policy,
not an accident of not having gotten to a dependency yet.

**Front end:** tree-walking over an in-memory program representation.
Incremental caching to satisfy F1's 100ms bound is planned but not yet
built; the F1 benchmark itself is deferred until the pipeline is complete
enough to measure honestly. A benchmark run against today's much smaller,
much simpler pipeline would report a number, but that number would be noise
— it would not predict the latency of the real, feature-complete front end,
and a passing-but-meaningless benchmark is worse than an honestly absent
one because it would look like a satisfied requirement. This is why F1 is
tracked in docs/roadmap.md rather than claimed as done.

**Execution:** build-time literal merging is implemented now — merges that
are fully determined by layered values at build time (C6, reference §4)
don't need a runtime evaluator at all. An expression evaluator and a
bytecode VM are the next increment for the parts of the language that do
need runtime evaluation (function bodies, request handlers, view renders).
Compilation target (§11 of the requirements) is not decided beyond this:
bytecode is the current plan, not a commitment the requirements make.

**Views:** server-rendered, with events and DOM patches carried over the
builtin socket — a LiveView-family model. The browser runs no program
code; a view function renders `std.Element` on the server, and rendering
observes reads reactively so that a changed `state` property re-renders and
patches automatically (reference §9.4). This single-program model is what
makes G2 (transport invisibility) and G3 (hot reload preserving state)
coherent rather than two separately-engineered features: there is no
client/server split for either concern to fall into, because there is only
ever one program.

**Persistence:** an embedded key-value store, with `stored` properties
keyed by their full dotted name (reference §9.3) — the same naming
discipline (B1, B6) that governs everything else in the language governs
where its own persisted state lives, rather than persistence needing a
separate addressing scheme.

**FFI:** `foreign` declarations are bound by the build to `foreign/<space>`
host libraries in the project (reference §9.10) — the one boundary
everything not in the builtin set crosses, with the manifest recording the
resolved location per G1's "no runtime dependency resolution" (resolution
happens at build time, once, and is recorded, not repeated at every
process start).

## Consequences

- G1 and G5 are satisfied by the same policy decision (zero crates)
  applied at two levels: the language has no package registry (G5), and
  the tool that implements the language doesn't depend on one either — the
  two facts reinforce each other rather than being independently argued.
- Because F1 is explicitly not yet benchmarked, docs/roadmap.md carries it
  as a named, pending item with its own proof obligation (a hard-failing
  benchmark at 1,000 files) rather than it being silently assumed true.
  Anyone relying on F1 today is relying on an unverified plan, and that
  should be visible.
- The server-rendered, no-client-code architecture is a load-bearing
  choice, not an implementation detail: it is the reason G2 and G3 are
  achievable as one mechanism instead of two, and changing it later would
  require re-deriving both requirements' satisfiability from scratch.
- Because persistence keys off full dotted names, renaming a `stored`
  property is a refactor with real data-migration weight (the old key's
  data doesn't automatically follow the rename) — this is a known sharp
  edge for the refactor commands (E1-E6, docs/roadmap.md item 3) to address
  explicitly rather than something the naming scheme quietly waives.
