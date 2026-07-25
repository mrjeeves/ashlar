space gallery

// The gallery: one page listing every other example, live in a frame.
//
// It is here because writing it was IMPOSSIBLE until settings existed. A page
// of links needs addresses, and an address is not something a program can know
// when it is written — so the showcase was an HTML file with a hand-kept port
// map, opened over `file://`, outside the language it was advertising. That is
// the shape of a language that has become a universe unto itself.
//
// Read this file looking for a URL. There is not one.

// One example as the gallery sees it. `url` is a field like any other: the
// NAME is source, the value arrives from outside (§9.12).
part Site {
  name: text
  blurb: text
  url: text
}

// Examples arrive grouped, so the sidebar's headings are data too — no
// ordering logic, no lookup table, just the shape the deployment supplies.
part Group {
  label: text
  sites: [Site]
}

// The one thing this program cannot know when it is written: where the other
// examples are serving. A `setting` declares the name and the shape; deployment
// supplies the value. There is no default, so starting without one fails with
// the name and shape in hand instead of serving a page of dead frames.
part Catalog {
  setting groups: [Group]
}

part app {
  port = 8080
  style = "gallery"
}

part page {
  route = "/"
  view = () => el(shell, {})
}

// The chrome. `chosen` and `at` are per-instance state (§9.4): clicking runs
// on the server, and the frame's `src` is patched in place — the page itself
// never reloads.
part shell {
  state chosen: text = ""
  state at: text = ""

  view = () => el("div", { class: "app" }, [
    el("nav", { class: "side" }, [
      el("div", { class: "brand" }, [
        el("p", { class: "wordmark" }, ["ashlar"]),
        el("p", { class: "tagline" }, ["every example, live"]),
      ]),
      el("div", { class: "sections" }, sections()),
    ]),
    el("section", { class: "main" }, [bar(), body()]),
  ])

  sections = () => map(Catalog.groups, (g: Group) => el("div", { class: "section" }, [
    el("p", { class: "label" }, [g.label]),
    el("div", { class: "links" }, links(g)),
  ]))

  links = (g: Group) => map(g.sites, (s: Site) => el("button", {
    class: cls(s.name),
    onclick: (e: std.Event) => show(s.name, s.url),
  }, [
    el("span", { class: "name" }, [s.name]),
    el("span", { class: "blurb" }, [s.blurb]),
  ]))

  cls = (n: text) => (if n == chosen { "item active" } else { "item" })

  show = (n: text, u: text) => {
    chosen = n
    at = u
  }

  bar = () => el("header", { class: "bar" }, [
    el("h1", { class: "title" }, [heading()]),
    el("code", { class: "cmd" }, [command()]),
    el("span", { class: "spacer" }, []),
    opener(),
  ])

  heading = () => (if chosen == "" { "ashlar" } else { chosen })

  command = () => (if chosen == "" { "ashlar run examples/hello" } else { "ashlar run examples/" + chosen })

  opener = () => (if at == "" { el("span", { class: "hint" }, ["nothing open yet"]) } else { el("a", { class: "open", href: at, target: "_blank", rel: "noopener" }, ["open in a tab"]) })

  body = () => (if at == "" { landing() } else { el("iframe", { class: "frame", title: "example", src: at }, []) })

  landing = () => el("div", { class: "landing" }, [
    el("div", { class: "card" }, [
      el("h2", {}, ["Pick one on the left."]),
      el("p", {}, ["Each is a separate Ashlar program, serving on its own port. This page frames whichever one you choose."]),
      el("p", { class: "muted" }, ["This page is an Ashlar program too — and it does not know where the others are. Their addresses arrive as a setting, so nothing in its source is a location."]),
    ]),
  ])
}
