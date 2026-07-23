# ADR-0010: A stylesheet is the presentation peer of `foreign`

Date: 2026-07-23

Status: accepted

## Context

The examples proved every language claim except one an agent hits
immediately when building a real interface: how a view is styled. The
first reflex — inline `style="..."` attrs, or a `<style>` blob smuggled
in as escaped text — fails the vision on two counts. It is an *unchecked
string* threading presentation through the type system (the side channel
the language exists to abolish), and it puts a *location* nowhere or
everywhere rather than letting names bind. The AI-first principle makes
the bar concrete: an agent that reaches for styling should guess the
right shape, and a wrong guess should fail loud, not render quietly
unstyled.

CSS is already name-bound. `class="composer"` in markup binds to
`.composer` in a sheet by name and nothing else — precisely "the fit
between names is the joint." So the question was never "what styling
system should Ashlar grow"; it was "where does the sheet attach, without
a path appearing in source."

## Decision

**A stylesheet is a named boundary to a foreign presentation language,
resolved by the build — the presentation peer of `foreign` (§9.10) and
the sibling of `files` (§9.8).**

- The server root declares `style = "name"`. The build resolves it to
  `assets/name.css`; a declared sheet that is missing is a loud build
  error, exactly as a missing `files` directory or `foreign` library is.
- The runtime serves the sheet at `/name.css` and injects
  `<link rel="stylesheet" href="/name.css">` into every served page's
  head. The href is *derived by the runtime*, never written in source.
- Views carry `class` names; those names bind to the sheet's rules by
  name. That is the entire styling model — no new syntax, no checked
  presentation language inside Ashlar.

The parallel is exact and load-bearing: `foreign` is "the one boundary
everything not in the builtin set crosses, with the manifest recording
the resolved location" (ADR-0007). Appearance is not in the builtin set,
and CSS is a language defined outside Ashlar. So a stylesheet crosses the
same kind of boundary the same way — named in source, located by the
build, defined elsewhere, checked at the edge.

Pure convention (auto-linking `assets/style.css` if present) was
rejected: it is *more* name-only but fails **silently** when the file is
misnamed, and "nothing this reference does not define happens silently"
plus the vision's fail-loud tiebreaker both forbid that. A declared
`style` that errors when absent is the loud-failure form.

## Consequences

- An agent that has read §9.8/§9.10 already knows §9.4's styling: same
  shape, same failure. Maximal resonance is the AI-first property; a
  distinct styling mechanism would have been one more thing to know.
- Because the sheet is a real served asset, it carries no escaping
  constraint — the earlier "avoid `>` and quotes in a `<style>` blob"
  problem dissolves; the file is ordinary CSS.
- CSS class names live outside Ashlar's name graph, so `ashlar rename`
  does not track them across `.ash` and `.css` — the same honest limit
  `foreign` accepts for the bodies it does not check. Styling is named in
  Ashlar and defined in CSS, and the boundary is where the tracking
  stops.
- `t_examples` stages the whole project tree (not just `.ash`) so a
  declared sheet is present at runtime; `t_g` pins that the link lands in
  the head and the sheet serves as `text/css`.
