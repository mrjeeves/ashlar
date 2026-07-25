# Showcase

A live gallery of every example: one page, a sidebar, and a frame that swaps
between the running apps.

## Run it

```
./showcase/serve.sh          # macOS / Linux
./showcase/serve.ps1         # Windows, or pwsh anywhere
```

One command, no arguments. Needs Rust 1.65+ and nothing else. It starts all fifteen — each on its own port — and
prints a `file://` path to open; that page is the gallery. `file://` is fine,
the page needs no server of its own. Ctrl-C stops every example at once.

Both launchers state the same name-to-port map, and
`t_examples_showcase_launchers_agree_on_every_port` asserts they agree with each
other and with `index.html` — so the three copies cannot drift, and a new
example cannot stay out of the gallery.

On Windows, fourteen of the fifteen work unchanged. `ledger` reaches SQLite over
the `native` transport, which needs a POSIX dynamic loader, so its page serves
but its store faults with that correction; `abacus` is the cross-platform
foreign example, a Python worker co-process.

Each launcher builds the release binary if needed, builds `ledger`'s SQLite shim
where it can,
and runs each example with `ashlar run examples/<name> --port <n>` — the source
keeps `port = 8080`, so nothing in any example changes (the port is a
deployment fact, §9.1/B5). Ctrl-C stops them all. Click a name in the sidebar,
or use the arrow keys, to swap frames.

The frames are the **real servers** — there is no baked snapshot to drift from
the apps (ADR-0016). Each example also runs standalone the usual way:

```
ashlar run examples/counter  # http://127.0.0.1:8080
```
