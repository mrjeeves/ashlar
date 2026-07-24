# Showcase

A live gallery of every example: one page, a sidebar, and a frame that swaps
between the running apps.

## Run it

```
./showcase/serve.sh          # start all fourteen, each on its own port
open showcase/index.html     # then open the page (file:// is fine)
```

`serve.sh` builds the release binary if needed, builds `ledger`'s SQLite shim,
and runs each example with `ashlar run examples/<name> --port <n>` — the source
keeps `port = 8080`, so nothing in any example changes (the port is a
deployment fact, §9.1/B5). Ctrl-C stops them all. Click a name in the sidebar,
or use the arrow keys, to swap frames.

The frames are the **real servers** — there is no baked snapshot to drift from
the apps (ADR-0016). Each example also runs standalone the usual way:

```
ashlar run examples/counter  # http://127.0.0.1:8080
```
