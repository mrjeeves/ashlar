space guardrails.core

part app {
  port = 8080
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
