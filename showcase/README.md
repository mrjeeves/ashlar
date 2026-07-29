# Showcase

Launchers. They start every example at once, each on its own port, with
`examples/gallery` on **8080** — an Ashlar program that frames the other sixteen.

The gallery used to live here as a hand-written `index.html` opened over a local
file URL, because a page of links needs addresses and B5 forbade a location in
source. Settings fixed the requirement rather than the symptom (ADR-0020), so
the gallery moved into `examples/` where every other program lives, and this
directory is now just the two launchers.

## Run it

```
./showcase/serve.sh          # macOS / Linux
./showcase/serve.ps1         # Windows, or pwsh anywhere
```

One command, no arguments. Needs Rust 1.65+, and Python 3 for the two examples
that reach a co-process. Nothing else: `ledger` prefers a compiled shim and
falls back to Python where one cannot be built, so no example needs a
development package or a second toolchain. It starts all seventeen — each
on its own port — then tells you to open <http://127.0.0.1:8080>. Ctrl-C stops
every example at once.

Both launchers state the same name-to-port map, and
`t_examples_showcase_launchers_agree_on_every_port` asserts they agree with each
other and with `examples/gallery/settings.json` — so the three copies cannot
drift, a new example cannot stay out of the gallery, and the gallery cannot
frame an address nothing is serving.

`enclave` asks the mesh node this machine runs. With none — not installed, not
open, or on the other side of a WSL boundary — it serves an empty roster that
says so and carries the correction, because a machine with no mesh is an
ordinary machine and not a broken site.

On Windows all seventeen work, which was not true until `ledger` gained its
second binding: the `native` transport needs a POSIX dynamic loader Windows has
not got, so there the launcher binds ledger's Python worker instead. Both
foreign examples are then co-processes, which is the transport that runs
anywhere.

`ledger` is the one example with a build step, and it is now optional. It
reaches a real SQLite database, and it ships two bindings for that one
capability: a C-ABI shim the launcher compiles where it can, and a Python
worker using the standard library's SQLite where it cannot. Windows has no
POSIX dynamic loader, so `native` can never work there; some machines have no
`rustc`; some have `libsqlite3` but not its development package. In every one
of those cases the launcher binds the worker instead, says so, and the example
runs unchanged — same SQL, same shapes, same live board. Where the shim does
build, the launcher runs `ashlar foreign check` to prove the capability is
reachable rather than assuming the build implies it.

`abacus` is Python too, and the interpreter answers to different names on
different machines. The launchers look for `python3`, then `python`, then (on
Windows) `py`, and rebind the space to whichever they find — the example's own
`foreign.json` stays the honest default and is never edited. With no Python at
all, both say so plainly and the other fifteen are unaffected.

Each launcher runs `cargo build --release` before starting anything. That is a
no-op in a fraction of a second when nothing has changed, and it is deliberate:
the old check built only when the binary was *missing*, so a `git pull` left you
serving the code from before it — the one failure that looks like success for a
showcase whose claim is that the frames are the real servers. Without cargo on
PATH an existing binary is still used, with a note saying it may predate the
checkout.

Then it builds `ledger`'s SQLite shim where it can, and runs each example with
`ashlar run examples/<name> --port <n>` — the source keeps `port = 8080`, so
nothing in any example changes (the port is a deployment fact, §9.1/B5). Ctrl-C
stops them all.

The gallery leads with `enclave` on a full-width stage and puts every other
example in a live grid under its section heading, so a whole section is read at
a glance rather than one click at a time. Clicking a tile's name promotes it to
the stage; which example leads is a setting, like the addresses.

**If something does not start, the launcher says so.** Each example's output
goes to `.showcase-logs/<name>.log`, and after starting them the launcher checks
which are still running rather than announcing success it never verified. A
program that refused to start — a port already taken, a missing setting, a
stored value that no longer fits its shape — is named, with the last few lines
it printed. `gallery` failing is called out specifically, because it is the page
on 8080 and its absence is what a browser reports as `ERR_CONNECTION_REFUSED`.

The frames are the **real servers** — there is no baked snapshot to drift from
the apps (ADR-0016). Each example also runs standalone the usual way:

```
ashlar run examples/counter  # http://127.0.0.1:8080
```
