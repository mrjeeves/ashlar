# docs/

The paper trail, in the order the hierarchy reads it:

| file | what it is |
|---|---|
| [`vision.md`](vision.md) | The fixed principles. Everything below serves this. |
| [`requirements.md`](requirements.md) | Numbered, testable requirements (A–G series) with the suite map. |
| [`roadmap.md`](roadmap.md) | The honest "not yet" ledger. Open section currently empty; below it, the dated record of what was delivered and what proves it. |
| [`diagnostics.md`](diagnostics.md) | The stable diagnostic catalog: E001–E029 + W001, each with its requirement, stage, cause, and correction. |
| [`decisions/`](decisions/) | ADRs 0001–0019: what was decided, why, and what it cost — from the name itself to the stylesheet boundary, view reconciliation, semantic freedom and derivability, the origin-not-edge deployment posture, the proposed data layer (databases named in source, bound in deployment; kept off the loop's blocking path), the storage-scope cleanup (retire `synced`, add per-user scope — spelled `peruser` since ADR-0019), the shared design language + live showcase for the examples (with the `--port` run-time override), the foreign boundary as a capability whose transport is bound in deployment (keyed by space name, so the refactors, the checker, and the manifest all see it), reversibility recast as a property specific refactors have rather than a law over all of them, and the A3 run-3 cold-read findings that respelled `owned` to `peruser` and `reads`/`writes` to `watches`/`updates`. |
| [`ontology.md`](ontology.md) | An essay reading Ashlar as a philosophical ontology: the clean metaphysical reading, six strains, and where the metaphysics actually lives. Reflective, not normative. |
| [`philosophical_edges.md`](philosophical_edges.md) | The essay's open questions in working form — philosophical guidance for the ongoing design. |

Agents working in this repo start at [`../AGENTS.md`](../AGENTS.md).
The language reference lives in [`../reference/ashlar.md`](../reference/ashlar.md)
and outranks everything here except the vision: it is the contract the
tests encode. The cold-read gate protocol and its run results live in
[`../suites/t_a3/`](../suites/t_a3/).
