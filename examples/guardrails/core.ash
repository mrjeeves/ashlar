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
  view = () => el("div", { class: "stage" }, [
    el("title", {}, ["guardrails"]),
    el(checker, {}),
  ])
}

// One decision the gate has already made. A data shape: fields only (§5).
part Verdict {
  body: text
  allowed: bool
  why: text
}

// What the gate has decided lately, for everybody. The pipe itself is pure —
// it takes a Decision and returns one — so the record of its decisions is
// ordinary shared `state` beside it, written wherever a decision is FINAL:
// the page's submit, and the HTTP route. Every open page re-renders (§9.3).
part Log {
  state recent: [guardrails.core.Verdict] = []
  keep = (d: guardrails.core.Decision) => {
    let tail = slice(recent, 0, 5)
    recent = [{ body: d.body, allowed: d.allowed, why: join(d.notes, " · ") }, ...tail]
  }
}

part checker {
  state draft: text = "share the secret password"
  view = () => el("div", { class: "card" }, [
    el("p", { class: "kicker" }, ["typed policy pipe · §4"]),
    el("h1", {}, ["guardrails"]),
    el("p", { class: "lede" }, ["Each space layers a check onto one review pipe. Type a message; the composed policy decides live."]),
    el("form", { class: "row", onsubmit: send }, [
      el("input", { class: "field", oninput: typed, value: draft, placeholder: "a message to review", autocomplete: "off" }, []),
      el("button", { class: "primary" }, ["submit it"]),
    ]),
    verdict(),
    el("div", { class: "log" }, [
      el("p", { class: "logtitle" }, ["Submitted, by anyone on this page"]),
      el("ul", { class: "logrows" }, rows()),
    ]),
  ])
  // Typing decides for you alone; submitting decides for the record, and
  // every other window watching this page sees it land.
  send = () => {
    Log.keep(Gate.review({ body: draft, allowed: true, notes: [] }))
  }
  rows = () => {
    if len(Log.recent) == 0 {
      return [el("li", { class: "logempty" }, ["Nothing submitted yet. Open a second window and submit there."])]
    }
    return map(Log.recent, (v: guardrails.core.Verdict) => el("li", { class: if v.allowed { "logrow ok" } else { "logrow no" } }, [
      el("span", { class: "logmark" }, [if v.allowed { "✓" } else { "✕" }]),
      el("span", { class: "logbody" }, [v.body]),
      el("span", { class: "logwhy" }, [v.why]),
    ]))
  }
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
  review pipe = (d: guardrails.core.Decision) => d
}

part inspect {
  route = "/api/review"
  handle pipe = (req: std.Request) => {
    let d = fields(req.data) ?? fail(400, "send a JSON object: { body }")
    let decided = Gate.review({
      body: text(d["body"] ?? ""),
      allowed: true,
      notes: [],
    })
    Log.keep(decided)
    return decided
  }
}
