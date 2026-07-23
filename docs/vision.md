# Vision

Fixed. If the vision is wrong, stop.

## The hierarchy

Four layers. Each serves the one above it. When two layers conflict, the higher one wins.

```
VISION          The principles below. Fixed. If the vision is wrong, stop.
REQUIREMENTS    docs/requirements.md. Revised when it fails to express the vision.
TESTS           The current best encoding of the requirements. Revised freely.
CODE            Whatever makes the tests pass.
```

Code yields to tests. Tests yield to requirements. Requirements yield to the vision. Nothing overrides the vision.

## The principles

**AI-first.** An AI that trips into this codebase should guess right, and fail loud when it guesses wrong. Most of this code is read and written by agents with no prior context, so the surface stays small enough to hold at once and features resonate — knowing one predicts the next — while the places a guess could go wrong are shaped to stop the build, not ship. This is the test the rest of these principles serve.

**Code is cheap, good design isn't.** Generation is nearly free. Verification, comprehension, and change are not. Optimize those; never ration generation.

**Names matter more than anything.** Names are the only binding mechanism. Not paths, not positions, not file locations, not declaration order.

**The build is state, the code is intent.** Source declares what should be true. The build computes where everything lives and how it resolves.

**Things should work similarly to other things in a way that makes sense.** Prefer resonance with what a reader already knows on the surface; prefer internal consistency in semantics. Where they conflict, resolve toward whichever fails loudly when guessed wrong.

**Refactoring is a first-class concern.** Changing intent must have computable blast radius. This is what makes cheap code trustworthy: if an agent generates ten times the volume a human reads, the unread portion is only safe when changes to it are provably contained.

The last two are one principle at two time scales. State derived at build time is what makes intent editable without fear.
