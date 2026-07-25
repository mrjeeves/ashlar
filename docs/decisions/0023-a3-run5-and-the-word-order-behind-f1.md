# ADR-0023 — A3 run 5: the contamination proved itself, and F1 moved

**Status:** accepted and applied, 2026-07-25. Amended the same day with the F1
candidate read, which **refuted the word-order hypothesis this ADR proposed.**
No grammar change follows. See "What the candidate read actually found".
**Evidence:** `suites/t_a3/results/2026-07-25-sonnet-run5.md` (55 agents, 0 errors).
**Confirms:** ADR-0021, by experiment rather than by argument.
**Closes:** A3-F5. **Relocates:** A3-F1. **Records:** A3-F7.

## The result

**24 / 25, against a bar of 20 / 25. A3 is satisfied** on the current corpus,
measured under the revised protocol. Twenty snippets scored 4/4 clean. The one
failure was unanimous across three judging lenses.

The run is labelled **reduced-contamination**, not cold, and that label is now
load-bearing rather than decorative — see below.

## What ADR-0021 argued, and what run 5 proved

ADR-0021 withdrew runs 3 and 4 on a directional argument: extra knowledge can
turn a wrong answer right but never the reverse, so passes under contamination
are not evidence while failures still are. That argument was sound but indirect.

Run 5 produced the direct evidence, and I did not design the experiment — the
corpus did. The section removed from `AGENTS.md` in ADR-0021 said, among other
things:

> Chain properties (`stack`/`pipe`) must restate their kind on every layer;
> pipe layers must agree in parameter AND return shape.

Exactly one fixture asks a reader to state that fact: `08-handle-pipe`, whose
first two rubric bullets are that both `handle` functions run as chained stages.
Its history:

| run | `AGENTS.md` in the reader's prompt | `08-handle-pipe` |
|---|---|---|
| 1 (2026-07-22) | did not exist in the tree | FAIL 1/4, wrong-claim |
| 2 (2026-07-22) | did not exist in the tree | FAIL 2/4, wrong-claim |
| 3 (2026-07-25) | **present, stating pipe chaining** | **PASS 4/4** |
| 5 (2026-07-25) | present, no longer stating it | FAIL 2/4, wrong-claim, 0/3 panel |

The one fixture whose fact leaked is the one fixture that flipped to PASS while
the leak was open and flipped back the moment it closed. Nothing else about the
fixture, the rubric, or the method changed between runs 3 and 5.

**A3-F5 closes on the same run.** ADR-0019 respelled `owned` → `peruser` and
`reads`/`writes` → `watches`/`updates`; run 4 validated the new spellings and
ADR-0021 withdrew that validation because its passes were contaminated. Both
fixtures now score **4/4 clean with isolation measured**:

> `11-peruser` — "a per-user, persisted list of text strings, starting out empty
> **for each user**."

> `25-foreign-reactive` — "one `updates` it, the other `watches` it … consumers
> of `all` are understood to observe the effects of `save`."

## The isolation probes contradicted my own fix, and they were right

Three probes, spawned identically to the readers, were asked only to reproduce
their own project instructions and list any language facts in them. All three
reported that `AGENTS.md` **still states language facts**, despite the sentence I
had put in it claiming it must never teach the language. They agree on which:
`move` never narrows visibility; `rename`/`rekind`/`move` are the byte-identical
commands; `meld` and `pattern` are banned words; `native`/`worker`/`http` are
transports and `native` needs a POSIX loader; `Unknown` absorbs what the checker
cannot prove; a program has parts, homes, and a composition order.

They are correct, and the claim was an overclaim. AGENTS.md is the *workflow*
contract, and the workflow legitimately involves the language's vocabulary — a
file that names refactor commands cannot honestly say it mentions nothing about
the language. A total ban is not achievable and pretending otherwise is how a
guard rots.

So the rule is restated to the property that actually matters:

> **AGENTS.md must not hand a reader any fact an A3 rubric asks that reader to
> produce.**

That is narrower, true, and *mechanically checkable*:
`t_meta_agents_md_does_not_teach_the_language` now intersects AGENTS.md's
inline-code spans with every inline-code span in every `## Must state` list (171
tokens). The intersection is exactly one — `http`, which fixture `05-limits-deep`
uses as a **map key** and AGENTS.md means as a **transport**. Different
referents, no fact transferred, exempted with that reason recorded in the test.
Anything new fails the suite.

Two checks confirm it has teeth: appending run 3's actual leaking sentence to
AGENTS.md fails the test on `stack` and `pipe`, and the test caught the very
paragraph I wrote to describe it (I had written the builtin-space prefix in
prose).

None of the residual facts touches a rubric, so no run-5 score is in doubt from
them. Two are *near*: the artifacts line names state files while `10-stored` is
about persistence, and hard rule 9 says a program has a composition order while
`03` and `24` are about composition order — but neither says which construct
persists or what the order is, which is what those bullets ask for.

## A3-F1 relocated: the misread is a parse error, not a vocabulary one

`08-handle-pipe` has now failed every run whose readers were not handed the
answer. Runs 1 and 2 read the cause as "`pipe` fails to suggest chaining," and
run 2 filed it as an accepted residual. Run 5's answer shows that was the wrong
diagnosis:

> "`handle pipe = (req: std.Request) => req` — defines a handler **named
> `pipe`** that takes a `std.Request` and returns it unchanged"

The reader never asked what `pipe` means, because it never identified `pipe` as
the merge kind. It took `pipe` for the property's **name**. Then, reasoning well
from that premise, it reached the honest conclusion that the snippet is
under-determined:

> "Whether the language treats that as 'the second definition legitimately
> extends/overrides the first' or as 'a duplicate/conflicting definition of the
> same handler' is a resolution-semantics question the snippet itself doesn't
> settle"

There is a specific, testable cause. Ashlar's declaration grammar puts storage
words **before** the name and merge kinds **after** it. A reader who has met the
storage form first learns modifier-then-name, applies it to `handle pipe`, and
concludes the name is the second word. The two halves of one grammar disagree
about word order.

**The same confusion appears in a second fixture, in the opposite direction.**
`24-composed-program` lost bullet 1 because the reader again refused to decide —
and there the correct answer is that a redeclaration *replaces*:

> "One point of genuine ambiguity I can't resolve … whether reopening … replaces
> the original `add` outright or composes both bodies"

So the finding is not "`pipe` reads as replace." It is that **a redeclared
property does not tell a reader whether it replaces or composes, in either
direction** — which is exactly the job the merge-kind system exists to do. Two of
twenty-five fixtures hit it; one failed.

### The candidate read

Acting on this would change a core grammar across every example, so it got the
ADR-0019 treatment: candidates read blind, in slot, never as bare words. A 2×2
over the merge-kind **word** and its **position** relative to the property name,
three readers per cell, judged only on whether the chaining fact lands and
whether the reader explicitly refuses to decide. The design separates the two
hypotheses: if position is the cause, both before-the-name cells improve; if
vocabulary is, both `chain` cells do.

I pre-committed in this file, before seeing the result, that nothing clearly
winning means the grammar stays.

## What the candidate read actually found

| cell | spelling | n | **states both run** | refuses to decide | reads as replacement | wrong claim |
|---|---|---|---|---|---|---|
| A (control) | `handle pipe =` | 3 | **0/3** | 1/3 | 1/3 | 3/3 |
| B | `pipe handle =` | 2\* | **0/2** | 1/2 | 0/2 | 1/2 |
| C | `handle chain =` | 3 | **0/3** | 3/3 | 0/3 | **0/3** |
| D | `chain handle =` | 3 | **0/3** | 1/3 | 2/3 | 2/3 |

\* one cell-B reader died on an API error mid-response and returned nothing; its
slot is not counted rather than retried, so B is n=2.

**Zero of eleven readers, across four spellings and both word orders, stated
that both functions run.** Not one. Nobody stated the value-threading fact
either.

**The word-order hypothesis is refuted.** Putting the kind before the name (B, D)
did not move the load-bearing measure at all, and D was *worse* than C on
confidently-wrong claims. The parse-error story in the section above — that a
reader mistakes the second word for the name — is a real thing one reader did,
but fixing it changes nothing, because the readers who parsed the declaration
correctly still could not tell what two declarations of one property do.

So: **no grammar change, and no keyword change.** The pre-commitment holds.

### What did move: the failure mode

`handle chain =` produced **zero actively-wrong claims and unanimous explicit
abstention** — every reader said, in substance, "I cannot tell from the syntax
whether this replaces or composes," and one added the reason:

> "'chain' suggests multi-stage composition, which would make this additive (log,
> then still fall through to whatever the original chain did) rather than a full
> override."

The control produced the opposite: 3/3 actively-wrong claims, including a reader
who confidently concluded the program is *invalid* —

> "That's a duplicate/conflicting definition, which the resolution checker is
> required to catch and reject"

Against requirement A4's own principle — false familiarity is worse than
unfamiliarity — trading confident-and-wrong for uncertain-and-leaning-right is a
real improvement. It is also three readers, on one fixture, measuring failure
mode rather than comprehension. Changing a core keyword on that evidence is
precisely the over-fit ADR-0015 committed with `owned`, and this ADR said so
before the data arrived. **Recorded as the strongest lead for any future attempt;
not acted on.**

### Where the finding actually lives now

Not the kind word. Not its position. **Cross-file layering itself does not
cold-read**, and three independent lines of evidence now say so:

1. Eleven readers, four spellings, zero successes — the variable under test was
   not the cause.
2. `24-composed-program` fails the same way with **no kind word at all** (the
   default is replacement), so the confusion does not require a merge kind to be
   present.
3. Multiple readers converged on "two declarations of one name is a duplicate
   definition a resolver should reject." That is a *reasonable* reading. One name
   declared twice means an error in most languages, and no token on either
   declaration overturns a prior that strong.

The information a cold reader needs is not on the property line. It is the fact
that Ashlar merges same-named declarations across files — the vision's signature
move, "extending someone else's part without editing their file." A single word
in one declaration cannot carry a whole composition model.

That reframes what A3 can tell us here. The gate measures whether a construct
reads correctly to someone who has never seen the language; for 24 of 25
constructs the answer is yes. For this one, the honest answer is that the feature
is *unguessable by construction*, and no spelling tested changes it. Removing it
is not on the table — it is the language's central claim.

Two things bound the cost, and neither is an excuse:

- **The misread is not silent for an author.** E004 and E005 force every layer to
  restate the kind, so nobody writes a layer without naming the merge behavior;
  the risk is in *reading* an unfamiliar codebase, not in writing one.
- **It is not the `owned` failure mode.** ADR-0019's finding was silently wrong
  in the direction of a security bug — a reader believing a shared store was
  per-user. Here the wrong belief surfaces the first time the program runs.

F1 therefore stays open as a recorded design finding with its cause correctly
identified for the first time, rather than as a keyword to be respelled. A future
attempt should test **the layering construct**, not the kind word: the candidate
worth reading is a marker on the *extending declaration* that says it extends,
and the question is whether that can exist without reintroducing a location or a
declaration order the vision forbids.

## A3-F7 recorded, not acted on

`17-optional-index` passed 3/4, missing the map-shape bullet: the reader called
`{text: number}` "a record/object with exactly one declared field: `text`, of
type `number`". ADR-0008 replaced a set-looking map shape with this colon form
and run 2 confirmed no reader formed the *set* reading. That still holds — this
is a different misread of the same construct, and a predictable one, because a
brace containing `key: type` is a record in most languages.

One reader, one bullet, on a passing snippet, and every alternative ADR-0008
weighed was worse. Recorded so a future run that fails here starts with the prior
rather than rediscovering it.

## What does not change

The corpus, the rubrics, and the bar. A3 is satisfied at 24/25 and the
requirement text needs no revision. The one item this leaves open is not a
finding at all: a **provably cold** read still needs a reader whose working
directory is not this repository, because `AGENTS.md` is injected before any
prompt can decline it. That is out of reach from inside the repo, which is the
one circumstance §1 of the requirements says to name rather than paper over.
