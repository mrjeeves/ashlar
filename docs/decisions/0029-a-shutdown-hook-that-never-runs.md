# ADR-0029: A second `unsafe`, so shutdown is real

Date: 2026-07-27

Status: accepted; implemented

## Context

Reference §9.1 has said since the runtime existed: *"On shutdown it calls
`stop`, then flushes stored state."*

An adversarial read of the docs against the binary found it false. There is
no signal handling anywhere in `crates/ashlar/src`, and nothing outside the
test suite ever sets the stop flag — `run_serve` creates it, passes it, and
drops it. A program logging in its shutdown hook logs nothing on `SIGTERM`
or `SIGINT`. The hook is reachable only from the library `serve` API that
`t_examples` uses.

`examples/slate` winds its pads down there. So does every program that
closes what it opened.

Rule 4 of AGENTS.md: *a command or construct that doesn't fully work does
not exist.* Two ways to satisfy it — remove the hook, or make it run.

## Decision

**Make it run, and take a second `unsafe` to do it.**

Rust's standard library has no signal API, and rule 2 forbids adding a
crate. Catching a signal therefore requires an `extern "C"` declaration and
a handler, which is `unsafe` — and rule 3 said the workspace has exactly one
`unsafe`, the `dlopen`/`dlsym` pair.

The alternative was removing the hook, which is worse than it sounds: the
shutdown hook is where a program releases what it holds, and a language for
servers that cannot say "when I stop, do this" has a hole no library can
fill for it. Deleting a documented, used, correct-when-invoked construct to
preserve a count of `unsafe` blocks is optimising the wrong number.

So the count goes to two, and the shape is held identical to the first:
`#[cfg(unix)]`, two extern declarations, one call site, and a handler that
does nothing but set an `AtomicBool` — the only action async-signal-safety
permits. The flag is read at the top of the event loop and sets the caller's
existing stop flag, so there remains exactly ONE shutdown path and the
library caller's behaviour is unchanged.

Off unix nothing is caught, and the reference now says so rather than
promising it. Stored state survives regardless: it is flushed whenever it
changes, not at exit.

## Consequences

- Rule 3 states two `unsafe` sites and the argument for the second. The
  prohibition it carried is narrowed rather than dropped: do not add a third
  for a **capability** — the `worker` and `http` transports are safe Rust
  and are where a capability belongs.
- A second defect surfaced behind the first, and it would have made the fix
  look like it worked while losing its evidence. `log.*` queues into the
  evaluator and the EVENT LOOP drains it each tick — which shutdown has just
  left. The hook ran and its lines were pushed and never printed. Shutdown
  drains the queue itself now, then flushes stdout and stderr, because the
  CLI exits through `process::exit` and runs no destructors. The last thing
  a program says is the thing most likely to be lost.
- `t_g_a_signal_runs_the_stop_stack_and_its_last_words_are_printed` spawns a
  real `ashlar run`, sends a real `SIGTERM`, and asserts both halves. A test
  that set the flag directly would have proved nothing about signals.
