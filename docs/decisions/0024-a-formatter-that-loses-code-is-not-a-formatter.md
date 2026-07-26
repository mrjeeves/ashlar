# ADR-0024: A formatter that loses code is not a formatter

Date: 2026-07-26

Status: accepted

## Context

`quarry` (`examples/quarry`) is the first example written as a large,
layered program rather than as a demonstration of one construct. Writing
it turned up two defects in `ashlar fmt`, both silent, both older than the
example.

**1. `else if` in expression position changed what the program returned.**

```
return if a { "high" } else if b { "low" } else { "mid" }
```

The printer emitted the else branch as a braced block — `else { if b
{ "low" } else { "mid" } }`. That is not the same program. Inside a
branch, `if` at statement position parses as an if **statement**, and a
branch whose statement is an if statement has the value `none` (§6). So
the first `fmt` pass turned every input the first branch missed into
`none`, and `ashlar check` had nothing to say about it, because the
function's inferred return shape simply widened to `text?`.

The second pass then deleted the branch outright — `else {  }` — because
the inline branch printer handled exactly one statement form
(`Stmt::Expr`) and printed **nothing** for the rest. The result still
compiled clean.

**2. A comment inside a multi-line literal migrated to another
declaration.** Comments are flushed at construct boundaries, and literal
items are not construct boundaries, so a note written above one map key
stayed queued until the next property opened and printed there — now
describing something else. The comment *count* was unchanged, which is why
the existing property test never saw it.

Neither defect could be reached by the corpus that guards the formatter.
`assert_fmt_faithful` — same AST, idempotent, comments preserved — runs
over the T-A3 snippets and the reference's ```ash blocks, and no snippet
in either had ever used an `else if` chain as an expression, a branch
carrying a `let`, or a comment inside a literal. `t_examples` asserts
`fmt(src) == src` over the examples, which is a fixpoint check on files
that are already canonical: it cannot see a construct the formatter
mangles until someone commits one, and committing one is how it was found.

## Decision

**An `else` branch that is itself an if-expression prints as `else if`.**
Chaining is not cosmetic here; braces around it change the value of the
branch. The whole chain prints inline when every branch is a single
expression, and in block form when any branch carries statements — so a
`let` inside a branch has somewhere to live.

**A branch printer is total.** `branch_inline` now falls back to block
form for anything that is not a lone expression, instead of emitting
nothing (and instead of joining statements with `; `, which is not
Ashlar — semicolons are a compile error).

**Comments inside multi-line list and map literals are flushed at the
item they were written above, and trailing comments stay on their item's
line.** A comment attached to the wrong declaration is worse than a
missing one: it is confidently wrong.

**The formatter's property corpus now includes `examples/`.** The three
properties are what would have caught defect 1 the moment any example used
an `else if` chain — the AST fingerprint for the meaning change,
idempotence for the erasure. The examples are corpus (AGENTS.md); they had
been held to a weaker formatter property than the T-A3 snippets, and now
they are not.

## Consequences

- `ashlar fmt` is meaning-preserving on the two shapes it was not.
  `reference/ashlar.md` and `README.md` both call it that, and now the
  claim is true of the binary.
- Three regression tests: the `else if` chain (statement and inline
  forms), the statement-bearing branch, and comment placement inside both
  literal kinds. Each is stated as a property, not as expected bytes,
  except the one assertion that the canonical chain still prints on one
  line — the fix must not blow every `if` into a block.
- No existing source changed shape: none of the seventeen examples, the
  T-A3 corpus, or the reference blocks reformat differently. The bug was
  only reachable by constructs the corpus had never contained.
- The formatter still refuses to preserve a comment written *between* the
  parts of an expression that prints on one line (there is no line to put
  it back on). That is a narrower gap than the one closed here, and it is
  visible rather than silent.
