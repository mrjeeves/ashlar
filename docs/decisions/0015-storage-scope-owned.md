# ADR-0015: Storage scope — retire `synced`, add `owned` (per-user state)

Date: 2026-07-23

Status: accepted

## Context

Ashlar carried three storage classes — `state`, `stored`, `synced`. Reading
the runtime showed the taxonomy does not match the binary:

- **`synced` has no distinct behavior anywhere.** `Storage::Synced` is
  defined in the AST and then handled in the lexer, parser, and formatter
  only — there is no branch in the evaluator or checker that makes it do
  anything `state` does not. The one class the runtime special-cases is
  `stored` (for persistence). Reactivity is *universal*: `assign_state`
  marks every dependent reader dirty regardless of class, and because there
  is no client code (views render on the server, §9.4), any view that reads
  any shared state re-renders on every connected client. "Synchronized to
  clients" is therefore a property the architecture provides
  unconditionally; `synced` named a distinction the binary never made — a
  guessability trap, three keywords implying three behaviors when there are
  two.
- **The per-user pattern is hand-rolled and dangerous.** A user's drafts,
  prefs, or todos are written today as a global `stored things: {text: T}`
  indexed by `req.user.id`. That manual keying is the source of the web's
  most common vulnerability (IDOR / broken access control): forget to
  scope, or scope by the wrong id, and one user's data is served to
  another. An agent tripping into the codebase can get this wrong silently.

Both point the same way: re-cut the taxonomy along the two axes that are
real — **lifetime** (ephemeral / durable) and **scope** (shared / per-user)
— with reactivity universal and unspoken.

## Decision

1. **Retire `synced`.** Remove the keyword and `Storage::Synced`. Its lone
   use (`ticker`) becomes `state` with identical behavior. The reference
   and G4 stop implying a class distinction the binary never had, and state
   reactivity across clients is documented as what it is: universal — read
   shared state in a view and it stays live on every client, always, with
   no opt-in.

2. **Add `owned` as a scope modifier** on state properties: `owned state`
   and `owned stored`. An `owned` property is scoped to the current user —
   each authenticated user has their own value, shared across all of that
   user's own instances (tabs, devices) and isolated from every other
   user. The runtime keys it by user id; source reads the bare name and
   gets the current user's value. `owned` is a **modifier, not a fourth
   class**: it composes with the lifetime axis and is part of the
   property's identity (fixed by the base layer, restated by later layers,
   exactly like a merge kind).

3. **Fail loud on a missing user.** The ambient "current user" is the one
   risk, and it is made loud, never silent:
   - **Compile error** where there is provably no user: reading or writing
     an `owned` property from a scheduled task's `run`, a boot `start`, a
     `spawn`, or any path with no request/render context — the checker
     proves these statically.
   - **Runtime fault** where the user is dynamically absent: an anonymous
     request (`req.user == none`) touching an `owned` property fails with a
     correction naming the guard to add — never a silent fall-through to
     shared or empty data.

4. **Security by construction.** Because the runtime scopes `owned` by the
   current user, one user cannot reach another's `owned` data — the IDOR
   footgun of manual `[req.user.id]` keying is removed. The naive read is
   the safe read, which is the AI-first principle applied to access
   control.

## Naming — decided by a T-A3 cold read

The word was chosen the way this repo decides names: guessability, measured.
Four candidates (`private`, `user`, `owned`, `personal`) were each shown to
three fresh readers with no context, in an identical neutral snippet, asked
only whether two signed-in people share the data or each get their own.

Per-user (correct) reads: **`owned` 3/3, `personal` 3/3, `user` 3/3,
`private` 1/3.**

- **`private` eliminated** — readers took it as OOP access-control
  ("restricted to this part"), the confidently-wrong failure the vision
  most opposes (A4); its single correct read only appeared once a reader was
  told to ignore other languages, a crutch real agents do not have.
- **`user` rejected despite a perfect read** — it overloads the identity
  vocabulary (`std.User`, `req.user`) and would be the first *domain noun*
  among otherwise structural reserved words, stealing the identifier app
  code most wants (`let user = login(...)`).
- **`owned` chosen over `personal`** — both read perfectly and avoid the
  collision; `owned` names the ownership / row-scoping semantic (reinforcing
  the security property) and lacks `personal`'s faint PII/GDPR connotation.

## Consequences

- **Reserved words** (§1): remove `synced`, add `owned` — still all
  structural/adjectival, no domain nouns taken.
- **G4 is reworded** (a requirements revision, justified by the vision).
  It currently reads "reactive state (local, persisted, and
  server-synchronized)." The capability set is unchanged, but the taxonomy
  is re-cut: reactive state along two axes — lifetime (in-memory /
  persisted) and scope (shared / per-user) — with cross-client
  synchronization a universal property of the no-client-code architecture,
  not a distinct class. Per the hierarchy, requirements yield to the vision,
  and AI-first guessability is the reason: the old wording implied a
  distinction the binary never made.
- **Reference edits** land with implementation: §9.3 (drop `synced`; add
  `owned`, its scope semantics, and its failure rules), §4 (the property
  grammar gains the optional `owned` scope modifier), §1 (reserved words),
  and a line in §9.6 tying `owned` to the request identity.
- **Diagnostics**: a new appended id for "owned property used with no user
  context" (compile) and its runtime-fault sibling, catalog rows in the same
  commit (docs/diagnostics.md, stable-id discipline).
- **Phase 2 synergy**: `owned stored` maps directly onto the database
  backend (ADR-0014) as row-level scoping by an owner column with the
  obvious index — the canonical multi-tenant shape. This cleanup lands
  *before* the backend so the backend is built against the final taxonomy.
- **Tests**: T-G (owned scoping across one user's instances; isolation
  between users; the runtime fault), T-A4 (an owned-in-a-task loud-fail
  fixture), T-D (the new diagnostics' corrections), T-META coverage, and the
  `ticker` rename. An example demonstrating `owned` (per-user notes or
  prefs) is worth adding when implemented.
- **Implementation is a distinct increment** ("Phase 2a"): this ADR records
  the decision; the reference/checker/runtime/test work follows, tracked as
  roadmap items until their tests pass.
