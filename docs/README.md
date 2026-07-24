# docs/

The paper trail, in the order the hierarchy reads it:

| file | what it is |
|---|---|
| [`vision.md`](vision.md) | The fixed principles. Everything below serves this. |
| [`requirements.md`](requirements.md) | Numbered, testable requirements (A–G series) with the suite map. |
| [`roadmap.md`](roadmap.md) | The honest "not yet" ledger. Currently empty — every item is delivered, dated, and moved off. |
| [`diagnostics.md`](diagnostics.md) | The stable diagnostic catalog: E001–E028 + W001, each with its requirement, stage, cause, and correction. |
| [`decisions/`](decisions/) | ADRs 0001–0013: what was decided, why, and what it cost — from the name itself to the stylesheet boundary, view reconciliation, semantic freedom and derivability, and the origin-not-edge deployment posture. |
| [`ontology.md`](ontology.md) | An essay reading Ashlar as a philosophical ontology: the clean metaphysical reading, six strains, and where the metaphysics actually lives. Reflective, not normative. |
| [`philosophical_edges.md`](philosophical_edges.md) | The essay's open questions in working form — philosophical guidance for the ongoing design. |

Agents working in this repo start at [`../AGENTS.md`](../AGENTS.md).
The language reference lives in [`../reference/ashlar.md`](../reference/ashlar.md)
and outranks everything here except the vision: it is the contract the
tests encode. The cold-read gate protocol and its run results live in
[`../suites/t_a3/`](../suites/t_a3/).
