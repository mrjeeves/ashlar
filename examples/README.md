# Examples

Each directory is a complete Ashlar project: run one with
`ashlar run examples/<name>` and open `http://127.0.0.1:8080`. Every
example is compiled and format-checked by the test suite
(`crates/ashlar/tests/t_examples.rs`) — if it's here, it builds.

## hello

The smallest server: one part declares the `port`, one part owns a
route. Two parts, no ceremony.

## counter

The live view protocol (§9.4) in one file: a `view` part with
per-instance `state`, instantiated with `el`, its `onclick` handler
running server-side over the built-in socket. The browser runs no
program code — open two windows and click.

## chat

The language's composition story in four files:

- `data.ash` — a data shape (`Message`), a `stored` map that survives
  restarts, and a `pipe` property (`prepare`).
- `api.ash` — routes, a `start stack`, JSON request handling, and the
  §9.6 auth builtins (`signup`/`login`).
- `audit.ash` — a separate space LAYERS the store and the app: its
  `prepare` pipe layer runs after the base's (use order is composition
  order), and its `start` stack joins the boot sequence. No base file
  was edited.
- `ui.ash` — a view that reads the store; any post re-renders every
  connected client that read it (§9.3 reactivity).
