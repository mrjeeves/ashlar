# ADR-0020 — Settings, and what B5 actually forbids

**Status:** accepted and applied, 2026-07-25.
**Revises:** B5.

## The complaint that started it

> a programming language cannot be a universe unto itself. it's going to need
> settings, links to things, etc. … We can't keep designing around being blind.
> The point is to reduce the paths to failure, increase the paths to
> understanding.

The provocation was concrete. The showcase is a page listing fifteen running
examples. Writing it in Ashlar — the obvious thing for a language whose whole
claim is that it builds interfaces — turned out to be **impossible**, because:

- B5 said "No source file contains a location," and `t_b5` enforces it by
  scanning `.ash` for `http://`; and
- `std` has no file I/O. The entire builtin list is data and collection
  functions.

So the only way a list of addresses could reach an Ashlar program was across the
`foreign` boundary — a co-process or a C ABI shim. To render a list of links,
the language demanded a Python worker. That is a path to failure the language
invented for itself.

## What B5 got wrong

The vision says:

> **Names matter more than anything.** Names are the only binding mechanism.
> Not paths, not positions, not file locations, not declaration order.
>
> **The build is state, the code is intent.** Source declares what should be
> true. The build computes where everything lives.

Both claims are about **binding**: a name must never resolve *through* a
location, and the build — not the author — decides where things live. Neither
says a program may not *know* a location, and neither says configuration does
not exist.

B5 hardened "source must not bind by location" into "source must not mention a
location." That is strictly stronger, it is not what the vision asks for, and it
cost the language the ability to be configured at all. A requirement that
forbids configuration is not serving a vision about *names*; it is serving a
slogan about *strings*.

## The decision

B5 now forbids binding by location, and permits a **setting**: a property whose
name and shape are source and whose value is a deployment fact.

```ash
part app {
  port = 8080
  setting endpoint: text
  setting retries: number = 3
}
```

- **A shape is required.** It is the only thing that can check a value which
  does not exist yet. `setting x` with no shape is `E030`.
- **No default means required.** With a default it is optional. Starting without
  a required value fails **before the first request**, naming every missing
  setting and its shape at once — not the first one, because an operator filling
  these in wants the whole list.
- **A setting is immutable.** `setting state x` is `E030`: the value was decided
  before the program ran, and a storage word would claim otherwise.
- **Values live in `settings.json`** at the project root, or at
  `ASHLAR_SETTINGS`, keyed by full property name. That is deliberately the same
  relationship `foreign.json` has to `ASHLAR_FOREIGN` and `--port` has to
  `port` — the third instance of one pattern, not a fourth mechanism.
- **Reading one is an ordinary property read.** Settings are seeded into the
  same value map state properties use, so nothing else in the evaluator needed a
  special case, and `rename`/`radius` already work over them because they are
  names.

No new diagnostic ids beyond `E030`: a `settings.json` key naming no declared
setting is `E001` (a name resolving to nothing — the same treatment
`foreign.json` keys got in ADR-0017), and a value of the wrong shape is `E006`.
That nothing else was needed is evidence the construct fits rather than intrudes.

## Why this is the smaller change it looks like

It reuses the property grammar exactly. A field is `name: shape`; a setting is a
field whose value arrives from outside. The cost is one reserved word, one
diagnostic id, and ~1,300 reference bytes.

The alternative designs were worse:

- **A settings *part*** — a data shape whose values deployment supplies. Needs a
  marker word anyway, and invents a second kind of part.
- **Reuse `foreign`** — a capability returning config. This is the status quo,
  and it is what made the showcase need a co-process to know a port number.
- **Env vars read by a builtin** (`env("ENDPOINT")`) — a location by another
  name, unshaped, unnameable by the toolchain, and undetectable when missing
  until the call. Every property settings have, this lacks.
- **Let `port = 8080` generalize** — settings *are* the general form `port` is a
  special case of, and unifying them would rewrite every example for no gain
  today. Left alone deliberately; noted here so the resemblance is not mistaken
  for an oversight.

## What the cold read said

Four candidate spellings were read blind in-slot (`setting`, `given`, `config`,
`bound`). **All four conveyed the meaning** — externally supplied, no default
means required, absence is an error caught before serving. The construct's
*shape* does the work, not the word: every reader seized on the contrast between
`setting endpoint: text` and `setting retries: number = 3` as the signal.

`setting` was chosen because `config` collides with existing names (a part in
`press`, a space in two A3 fixtures) and because it is the word the requirement's
author used. **Caveat recorded honestly:** those reads were run as in-repo
subagents, which are contaminated — see ADR-0021. None of the four words appears
in `AGENTS.md`, so the specific inference is probably sound, but it is not a
clean gate result and is not claimed as one.

## What this does not open

Settings are values, not code, and not names the compiler resolves. A setting
cannot name a part, a space, or a capability — B1 and B2 are untouched. Source
still contains no location that the build resolves; `settings.json` is
deployment's file, the way `foreign.json` is.
