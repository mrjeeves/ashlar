space guardrails.core

part app {
  port = 8080
  style = "guardrails"
}

// A live window onto the policy pipe. Whatever you type is run through
// `Gate.review` — the core pass plus every layered policy (§4) — and the
// composed decision renders as you type, over the socket (§9.4).
part page {
  route = "/"
  view = () => el("div", { class: "stage" }, [el(checker, {})])
}

part checker {
  state draft: text = "share the secret password"
  view = () => el("div", { class: "card" }, [
    el("p", { class: "kicker" }, ["typed policy pipe · §4"]),
    el("h1", {}, ["guardrails"]),
    el("p", { class: "lede" }, ["Each space layers a check onto one review pipe. Type a message; the composed policy decides live."]),
    el("input", { class: "field", oninput: typed, value: draft, placeholder: "a message to review" }, []),
    verdict(),
  ])
  verdict = () => {
    let d = Gate.review({ body: draft, allowed: true, notes: [] })
    return if d.allowed { el("div", { class: "verdict ok" }, [
      el("span", { class: "mark" }, ["✓"]),
      el("span", {}, ["allowed"]),
    ]) } else { el("div", { class: "verdict no" }, [
      el("div", { class: "vhead" }, [el("span", { class: "mark" }, ["✕"]), el("span", {}, ["blocked"])]),
      el("ul", { class: "notes" }, map(d.notes, (n: text) => el("li", {}, [n]))),
    ]) }
  }
  typed = (e: std.Event) => {
    draft = text(e.data.value)
  }
}

part Decision {
  body: text
  allowed: bool
  notes: [text]
}

// `review` is a typed extension point. It starts by accepting a request;
// later spaces can add policy by layering this pipe.
part Gate {
  keep = (d: guardrails.core.Decision) => d
  review pipe = (d: guardrails.core.Decision) => d
}

part inspect {
  route = "/api/review"
  handle pipe = (req: std.Request) => {
    return Gate.review({
      body: text(req.data.body),
      allowed: true,
      notes: [],
    })
  }
}
