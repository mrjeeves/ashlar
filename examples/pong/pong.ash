space pong

part app {
  port = 8080
}

// The whole game runs on the server: a 20fps schedule advances the
// ball, sliders steer the paddles, and every connected page re-renders
// from the same synced state (§9.3 + §9.7). Nobody's browser runs a
// line of game code. Field 400x240, ball 10, paddles 8x60 at the walls.
part Game {
  synced ball_x: number = 195
  synced ball_y: number = 115
  state dx: number = 6
  state dy: number = 4
  synced paddle_l: number = 90
  synced paddle_r: number = 90
  synced score_l: number = 0
  synced score_r: number = 0
  synced running: bool = false
  every = "50ms"
  run = () => {
    if running {
      step()
    }
  }
  step = () => {
    ball_x = ball_x + dx
    ball_y = ball_y + dy
    if ball_y <= 0 or ball_y >= 230 {
      dy = -dy
    }
    if ball_x <= 16 and ball_y >= paddle_l - 10 and ball_y <= paddle_l + 60 {
      dx = -dx
      ball_x = 16
    }
    if ball_x >= 374 and ball_y >= paddle_r - 10 and ball_y <= paddle_r + 60 {
      dx = -dx
      ball_x = 374
    }
    if ball_x < 0 {
      score_r = score_r + 1
      serve(6)
    }
    if ball_x > 390 {
      score_l = score_l + 1
      serve(-6)
    }
  }
  serve = (toward: number) => {
    ball_x = 195
    ball_y = 115
    dx = toward
    dy = 4
  }
  steer_l = (y: number) => {
    paddle_l = y
  }
  steer_r = (y: number) => {
    paddle_r = y
  }
  toggle = () => {
    running = not running
  }
}

part page {
  route = "/"
  view = () => el(board, {})
}

// Each control is its OWN view instance on purpose: the field re-renders
// twenty times a second, but your slider only re-renders when a paddle
// moves — so a drag in progress is never replaced mid-gesture.
part board {
  view = () => el("div", {}, [
    el(field, {}),
    el("div", {}, [el(lefthand, {}), el(switch, {}), el(righthand, {})]),
  ])
}

part field {
  view = () => el("div", {}, [
    el("h2", {}, ["pong — " + text(Game.score_l) + " : " + text(Game.score_r)]),
    el("div", { style: box() }, [
      el("div", { style: ball() }, []),
      el("div", { style: paddle(4, Game.paddle_l) }, []),
      el("div", { style: paddle(388, Game.paddle_r) }, []),
    ]),
  ])
  box = () => "position:relative;width:400px;height:240px;background:#123;overflow:hidden"
  ball = () => "position:absolute;width:10px;height:10px;border-radius:5px;background:#fff;left:" + text(Game.ball_x) + "px;top:" + text(Game.ball_y) + "px"
  paddle = (x: number, y: number) => "position:absolute;width:8px;height:60px;background:#6cf;left:" + text(x) + "px;top:" + text(y) + "px"
}

part lefthand {
  view = () => el("input", { type: "range", min: "0", max: "180", value: text(Game.paddle_l), oninput: steer }, [])
  steer = (e: std.Event) => {
    Game.steer_l(number(text(e.data.value)) ?? 90)
  }
}

part righthand {
  view = () => el("input", { type: "range", min: "0", max: "180", value: text(Game.paddle_r), oninput: steer }, [])
  steer = (e: std.Event) => {
    Game.steer_r(number(text(e.data.value)) ?? 90)
  }
}

part switch {
  view = () => el("button", { onclick: flip }, [caption()])
  caption = () => (if Game.running { "pause" } else { "start" })
  flip = () => {
    Game.toggle()
  }
}

part state_api {
  route = "/api/state"
  handle pipe = (req: std.Request) => {
    return {
      x: Game.ball_x,
      y: Game.ball_y,
      pl: Game.paddle_l,
      pr: Game.paddle_r,
      l: Game.score_l,
      r: Game.score_r,
      running: Game.running,
    }
  }
}
