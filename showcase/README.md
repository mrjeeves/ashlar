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

One command, no arguments. Needs Rust 1.65+; `ledger` additionally needs
SQLite's development package (`libsqlite3-dev` / `sqlite-devel`) because it links
the real library. Everything else needs only Rust. It starts all seventeen — each
on its own port — then tells you to open <http://127.0.0.1:8080>. Ctrl-C stops
every example at once.

Both launchers state the same name-to-port map, and
`t_examples_showcase_launchers_agree_on_every_port` asserts they agree with each
other and with `examples/gallery/settings.json` — so the three copies cannot
drift, a new example cannot stay out of the gallery, and the gallery cannot
frame an address nothing is serving.

On Windows, sixteen of the seventeen work unchanged. `ledger` reaches SQLite over
the `native` transport, which needs a POSIX dynamic loader, so its page serves
but its store faults with that correction; `abacus` is the cross-platform
foreign example, a Python worker co-process.

`ledger` is the one example with a build step: it reaches a real SQLite database
over the `native` transport, so its shim must compile first. If that fails the
launcher prints **rustc's actual error** and, when the error names libsqlite3,
the package to install — then says plainly that the other sixteen are
unaffected. When it succeeds, the launcher runs `ashlar foreign check` to prove
the capability is reachable rather than assuming the build implies it.

Each launcher builds the release binary if needed, builds `ledger`'s SQLite shim
where it can,
and runs each example with `ashlar run examples/<name> --port <n>` — the source
keeps `port = 8080`, so nothing in any example changes (the port is a
deployment fact, §9.1/B5). Ctrl-C stops them all. Click a name in the gallery's
sidebar to swap frames.

The frames are the **real servers** — there is no baked snapshot to drift from
the apps (ADR-0016). Each example also runs standalone the usual way:

```
ashlar run examples/counter  # http://127.0.0.1:8080
```
