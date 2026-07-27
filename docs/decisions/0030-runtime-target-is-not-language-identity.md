# ADR-0030: The runtime target is not the language's identity

Date: 2026-07-27

Status: accepted and applied

## Context

The README described Ashlar as “a composition language for servers and
interfaces.” It did so faithfully: the reference opened with the same phrase,
and requirements §10 said the language “builds servers and interfaces.”

The phrase compressed two different claims:

1. Ashlar is an AI-first composition language.
2. Its delivered runtime currently builds servers and interfaces.

Only the second claim is domain-specific. The vision never names servers,
interfaces, HTTP, or browsers. Its principles concern the economics of
agent-authored code, names as binding, source as intent, derived build state,
comprehension, and computable change. The A–F requirements express those
principles without depending on one runtime domain. The G requirements and
reference §9 describe the runtime that has actually been built.

Treating that first target as the language's identity quietly promoted an
implementation boundary into a permanent design boundary. It also made the
docs say more than the evidence supports in both directions: the composition
model is not proved to be confined to servers and interfaces, and no other
runtime target is delivered merely because the model may reach one.

## Decision

**Ashlar is an AI-first composition language. Its current runtime builds
servers and interfaces.**

Servers and interfaces are the first runtime target, not the identity or
permanent boundary of the language.

This correction does not claim that Ashlar is a general-purpose systems
runtime. A new target does not exist by implication. It requires an explicit
runtime contract, the same hierarchy of requirements and tests, and execution
against the uncooperative world that target actually inhabits.

The distinction is reflected at each level:

- the vision remains unchanged because it already describes the language;
- requirements §0 states the distinction and §10 stops turning today's runtime
  scope into the language's definition;
- the reference identifies servers and interfaces as the current runtime
  target while remaining the complete contract for what exists;
- the README leads with the language's actual identity and names the first
  runtime separately.

Historical ADRs remain records of the language as it was understood when their
decisions were made. Where their prose could now mislead a current reader, a
framing note points here rather than erasing the old reasoning.

## Consequences

- The language's core claim is stated at the level its vision supports:
  AI-first composition under derivable meaning and change.
- The runtime's current capability boundary remains explicit and testable.
- Future runtime work is neither prohibited by wording nor promised without
  implementation and proof.
- No syntax, compiler behavior, runtime behavior, diagnostic, or test
  requirement changes in this decision. This is a correction to the hierarchy
  of claims, not an expansion of delivered surface.

## Rejected

- **Keep the old phrase.** It is concise but conflates what Ashlar is with what
  its first runtime currently does.
- **Call Ashlar general-purpose.** Nothing in the repository proves that, and
  replacing one overclaim with another would make the framing less honest.
- **Stop mentioning servers and interfaces.** That would hide the runtime
  people can actually build with today. The distinction is more accurate than
  either absolute.
