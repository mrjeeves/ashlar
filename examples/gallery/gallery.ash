space gallery

// The gallery: every other example, live, on one page.
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

// Examples arrive grouped, so the section headings are data too — no
// ordering logic, no lookup table, just the shape the deployment supplies.
part Group {
  label: text
  sites: [Site]
}

// The two things this program cannot know when it is written: where the other
// examples are serving, and which one leads. A `setting` declares the name and
// the shape; deployment supplies the value. Neither has a default, so starting
// without one fails with the name and shape in hand instead of serving a page
// of dead frames.
part Catalog {
  setting lead: Site
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

// The page. `chosen`, `blurb` and `at` are per-instance state (§9.4): clicking
// a tile promotes it onto the stage above, on the server, and the stage is
// patched in place — the page itself never reloads, and the frames under it
// keep whatever they were showing.
part shell {
  state chosen: text = ""
  state blurb: text = ""
  state at: text = ""

  view = () => el("div", { class: "page" }, [
    el("title", {}, ["ashlar — every example, live"]),
    el("header", { class: "top" }, [
      el("p", { class: "wordmark" }, ["ashlar"]),
      el("p", { class: "tagline" }, ["Every frame below is a separate program serving on its own port. So is this page."]),
    ]),
    stage(),
    el("div", { class: "sections" }, sections()),
  ])

  // The stage: the lead example, big, until a tile is promoted onto it.
  stage = () => el("section", { class: "stage" }, [
    el("header", { class: "stage-top" }, [
      el("h2", { class: "stage-name" }, [named()]),
      el("p", { class: "stage-blurb" }, [said()]),
      el("span", { class: "spacer" }, []),
      el("code", { class: "cmd" }, ["ashlar run examples/" + named()]),
    ] + back() + [
      el("a", { class: "open", href: where(), target: "_blank", rel: "noopener" }, ["open in a tab"]),
    ]),
    el("iframe", { class: "stage-frame", title: named(), src: where() }, []),
  ])

  named = () => (if chosen == "" { Catalog.lead.name } else { chosen })
  said = () => (if chosen == "" { Catalog.lead.blurb } else { blurb })
  where = () => (if chosen == "" { Catalog.lead.url } else { at })

  back = () => (if chosen == "" { [] } else { [el("button", { class: "back", onclick: reset }, ["back to " + Catalog.lead.name])] })

  reset = () => {
    chosen = ""
    blurb = ""
    at = ""
  }

  // Every section, and every example in it — all framed and running, so a
  // section is read at a glance rather than one click at a time.
  sections = () => map(Catalog.groups, (g: Group) => el("section", { class: "section" }, [
    el("h3", { class: "label" }, [g.label]),
    el("div", { class: "grid" }, tiles(g)),
  ]))

  tiles = (g: Group) => map(g.sites, (s: Site) => el("article", { class: cls(s.name) }, [
    el("header", { class: "tile-top" }, [
      el("button", {
        class: "tile-name",
        title: "put it on the stage",
        onclick: (e: std.Event) => show(s.name, s.blurb, s.url),
      }, [s.name]),
      el("a", { class: "tile-open", href: s.url, target: "_blank", rel: "noopener" }, ["open"]),
    ]),
    el("p", { class: "tile-blurb" }, [s.blurb]),
    el("iframe", { class: "tile-frame", title: s.name, src: s.url }, []),
  ]))

  cls = (n: text) => (if n == chosen { "tile on" } else { "tile" })

  show = (n: text, b: text, u: text) => {
    chosen = n
    blurb = b
    at = u
  }
}
