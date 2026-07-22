# Diagnostic catalog

Stable ids for every diagnostic the compiler emits. Ids never change meaning
across releases; retired ids are never reused. `req` names the requirement
each diagnostic enforces (see docs/requirements.md). Wire format is JSONL,
defined in reference §8 and implemented in `src/diag.rs`.

Rules for every diagnostic (D1):
- one location, one-sentence cause, and a correction specific enough to apply
  without judgment;
- attach machine `edits` only when applying them resolves this diagnostic and
  introduces no new error (D2) — otherwise the `note` carries the instruction
  and `edits` is empty.

| id | req | emitted by | condition | fix |
|---|---|---|---|---|
| E001 | B3 | resolver | name resolves to nothing (incl. dotted part declaration matching no visible part) | note nearest matches and the `use` that would provide them; `null`/`nil`/`undefined` get edits replacing with `none` |
| E002 | B3 | resolver | name resolves to more than one definition, or a `let`/param declares a name already visible | edits qualifying the reference with its full dotted name; for shadowing, note says rename the local |
| E003 | B4 | resolver | two names in one scope differ only by case or separator convention | note names both declarations; no edits (renaming is a judgment) |
| E004 | C5 | composer | layer states a different merge kind than the property's identity | edits restating the declared kind |
| E005 | C5 | composer | layer omits the kind on a property whose identity has one | edits inserting the declared kind after the property name |
| E006 | A4 | checker | shape mismatch: cause states the expected and actual shape — including pipe layers disagreeing in parameter/return shape (§4) and `stack` return keys that are not state properties | mechanical edits where safe (`text(...)` wrap on mixed `+`, `!= none` on optional conditions); precise notes otherwise |
| E007 | A4 | parser | unexpected token (generic parse error) | note states what was expected |
| E008 | B7 | resolver | `use` names a part or unknown space | edits rewriting to the part's space when that is the case |
| E009 | A4 | lexer | `${` inside a text literal | note: Ashlar has no interpolation; join with `+` |
| E010 | A4 | lexer | `;` anywhere | edits replacing it with a newline (deletion would join `a; b` wrongly) |
| E011 | A4 | lexer | `#` comment | edits replacing `#` with `//` |
| E012 | A4 | lexer | raw newline inside a text literal | note: close the literal and join with `+` |
| E013 | C5 | composer | property declared twice in one layer | note names both declarations |
| E014 | C2 | resolver | second layer of one part declared in the same space | note: merge the blocks; names both files |
| E015 | C2 | resolver | cycle in the use graph | note lists the cycle |
| E016 | A4 | parser | reserved word used as a name | note lists the word |
| E017 | B3 | resolver | layer declared on a `std` part | note: std parts cannot be extended |
| E018 | A4 | parser | foreign top-level construct: `import`, `from`, `export`, `class`, `function`, `def`, `struct`, `interface`, `enum`, `mod`, `package` | note names the Ashlar construct to use instead (`use` / `part`) |
| E019 | C4 | composer | stack function with parameters, or pipe function without exactly one | note states the arity rule |
| E020 | C4 | parser | `reverse` after `append`/`deep`/no kind | edits deleting `reverse` |
| E021 | A4 | checker | route rules: two patterns can match one path (duplicates and capture overlaps alike), a capture bound twice in one route, or an illegal capture name | note names both parts (or the offending capture); changing a path is the author's choice |
| E022 | B6 | parser | file does not begin with a `space` header, or `space`/`use` appear after declarations | note: add `space <name>` as the first line (name is the author's choice — no edits) |
| E023 | A4 | parser | foreign statement construct: `while`, `switch`, `match`, `try`, `catch`, `throw`, `var`, `const`, `elif` | note names the Ashlar construct to use instead (`for`/recursion, `if`, `??`/`none`, `let`) |
| E024 | E2 | resolver | function literal outside the two legal positions (property value, call argument) | note: name it as a property, or inline it at the call |
| E025 | A4 | resolver | assignment target is not a state/stored/synced property of the enclosing part | note: declare the property with a storage word, or use `let` for a local |
| E026 | G4 | composer | part has `every` but no `run` function property | note: add `run = () => { ... }` |
| E027 | C5 | composer | layer states a different storage word than the property's identity (omitting is allowed) | edits restating the declared storage |
| E028 | C4 | composer | `append`/`deep` on a number, bool, or function value, or layered literals of differing mergeable shapes | note states the mergeable shapes |
| W001 | C3 | resolver | two spaces layer one part and neither uses the other | edits adding the `use` that orders them (to the lexicographically later space's file, after its header) |

E013 also covers a duplicate key inside one map literal (same layer, same
construct — undefined by the reference, therefore an error).

Conventions for messages: causes are single sentences ending with a period,
name things with backticks, and state facts ("`Message` resolves to
`chat.data.Message` and `note.Message`."), not advice — advice lives in the
fix note.
