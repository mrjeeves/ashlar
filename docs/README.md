# docs/

The paper trail, in the order the hierarchy reads it:

| file | what it is |
|---|---|
| [`vision.md`](vision.md) | The fixed principles. Everything below serves this. |
| [`requirements.md`](requirements.md) | Numbered, testable requirements (A–G), the two-loop method, and the suite map. |
| [`roadmap.md`](roadmap.md) | The open ledger: what is not yet true. Delivered work is not recorded here — `suites/coverage.md`, `decisions/`, and `git log` each hold it better. |
| [`diagnostics.md`](diagnostics.md) | The stable diagnostic catalog: E001–E030 + W001, each with its requirement, stage, cause, and correction. |
| [`decisions/`](decisions/) | The ADRs: what was decided, on what evidence, and what it cost. Numbered in order; a new file only when a decision REVERSES an earlier one, and a decision that deferred its own implementation is closed in place. Read the index by title — restating them here is a copy that goes stale. |
| [`writing-ashlar.md`](writing-ashlar.md) | The traps that catch agents who guess instead of reading the reference. Linked by path, not imported. |
| [`ontology.md`](ontology.md) | An essay reading Ashlar as a philosophical ontology, with [`philosophical_edges.md`](philosophical_edges.md) carrying its open questions. Reflective, not normative: nothing in either constrains the work. |

Agents working in this repo start at [`../AGENTS.md`](../AGENTS.md).
The language reference lives inside [`../AGENTS.md`](../AGENTS.md), after the
`REFERENCE:BEGIN` marker (A1), and outranks everything here except the
vision: it is the contract the tests encode. The cold-read gate protocol and its run results live in
[`../suites/t_a3/`](../suites/t_a3/).
