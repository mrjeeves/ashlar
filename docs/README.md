# docs/

The paper trail, in the order the hierarchy reads it:

| file | what it is |
|---|---|
| [`vision.md`](vision.md) | The fixed principles. Everything below serves this. |
| [`requirements.md`](requirements.md) | Numbered, testable requirements (A–G series) with the suite map. |
| [`roadmap.md`](roadmap.md) | The honest "not yet" ledger. Currently empty — every item is delivered, dated, and moved off. |
| [`diagnostics.md`](diagnostics.md) | The stable diagnostic catalog: E001–E028 + W001, each with its requirement, stage, cause, and correction. |
| [`decisions/`](decisions/) | ADRs 0001–0016: what was decided, why, and what it cost — from the name itself to the stylesheet boundary, view reconciliation, semantic freedom and derivability, the origin-not-edge deployment posture, the proposed data layer (databases named in source, bound in deployment; kept off the loop's blocking path), the storage-scope cleanup (retire `synced`, add per-user `owned`), and the shared design language + live showcase for the examples (with the `--port` run-time override). |

Agents working in this repo start at [`../AGENTS.md`](../AGENTS.md).
The language reference lives in [`../reference/ashlar.md`](../reference/ashlar.md)
and outranks everything here except the vision: it is the contract the
tests encode. The cold-read gate protocol and its run results live in
[`../suites/t_a3/`](../suites/t_a3/).
