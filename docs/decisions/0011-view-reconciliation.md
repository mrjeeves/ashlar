# ADR-0011: A view instance is its root element, reused across renders

Date: 2026-07-23

Status: accepted

## Context

Building the flagship (a team chat whose UI is parts nested inside parts)
surfaced two defects in the view model that the small single-view
examples never reached, because none of them nested a *stateful* or
*lifecycle-bearing* view inside a view that re-renders.

1. **`el(Part)` minted a fresh instance on every render.** A parent
   re-rendering re-created all its child views with new ids, resetting
   their per-instance `state` and re-running their `start` stacks. A
   presence probe whose `start` incremented a shared counter therefore
   incremented it on every parent render — and because the child's
   `start` ran inside the parent's render with the parent's read
   tracking active, the counter read made the *parent* depend on it, so
   the write re-rendered the parent, which re-mounted the child: an
   unbounded loop.

2. **Each instance rendered inside a wrapper `<div data-ash-instance>`.**
   That wrapper sat between a layout container and the child views it
   contained, so `display: grid` on a parent saw wrapper divs, not the
   child views — the sidebar landed in the wrong column and the layout
   collapsed. An agent writing ordinary grid/flex would never guess a
   hidden wrapper was the cause.

Both are AI-first failures: an agent composes views by nesting parts and
styles them with ordinary CSS, and both broke silently.

## Decision

**Views reconcile by position, and an instance is its own root element.**

- On re-render, the Nth `el(Part)` reuses the instance that sat at
  position N last render (same part): its `state` and subscriptions
  survive and `start` does **not** re-run. A child present last render
  but not produced now has departed — it unmounts, running its `stop`
  stack. `start` runs once on mount, `stop` once on removal; this is the
  same mount/unmount contract §9.5 already states for a page's sockets,
  now honored across re-renders too.
- A child's `start` stack runs *outside* the parent's read tracking, so
  a read it performs never makes the parent depend on that state. Lifecycle
  is not a render.
- The `data-ash-instance` marker is stamped onto the view's own root
  element rather than a wrapper. The instance *is* that element; a view
  whose root is not a single element still gets a wrapper, so patching
  always has exactly one node to swap.

## Consequences

- Nested stateful/lifecycle views compose correctly: per-instance state
  persists across parent re-renders, subscriptions are not duplicated,
  and presence-by-lifecycle (mount arrives, unmount departs) is stable.
- CSS layout on a container of child views works as written — the child
  views are the container's direct children, no wrapper between.
- Position keying handles stable structure and append well; reordering a
  list of *stateful* child views by insertion in the middle can still
  re-key those positions. The flagship's stateful children sit in fixed
  positions, and its dynamic lists (room and person links) are
  stateless, so this limit is not reached; a keyed-list construct is a
  future item if a real case needs it, recorded honestly rather than
  waved away.
- Regression tests pin both halves: `t_g_nested_child_is_reused...`
  proves `start` runs once and the child id is stable across parent
  re-renders (and fails against the old mint-every-render behavior), and
  the flagship's runtime + real-browser drives exercise the whole model.
