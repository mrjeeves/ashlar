//! T-E — refactor commands (E1–E6): blast radius, absence of the prior
//! state, roundtrip byte-identity, and refusal on incomplete radius.

use ashlar::refactor;

fn sources(v: &[(&str, &str)]) -> Vec<(String, String)> {
    v.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect()
}

const BASE: &str = r#"space chat.data

part Message {
  id: text
  body: text
}

part Store {
  stored messages: {text: chat.data.Message} = {}
  add = (m: chat.data.Message) => {
    messages = put(messages, m.id, m)
  }
}
"#;

const AUDIT: &str = r#"space chat.audit
use chat.data

part chat.data.Store {
  add = (m: chat.data.Message) => {
    log.info("adding", { id: m.id })
    messages = put(messages, m.id, m)
  }
}
"#;

const API: &str = r#"space chat.api
use chat.audit

part api {
  route = "/api/messages"
  handle pipe = (req: std.Request) => {
    chat.data.Store.add({ id: id(), body: "hello" })
    return chat.data.Store.messages
  }
}
"#;

#[test]
fn t_e_rename_part_radius_absence_and_roundtrip() {
    // covers: E1, E2, E3, E4
    let srcs = sources(&[("base.ash", BASE), ("audit.ash", AUDIT), ("api.ash", API)]);

    let plan = refactor::plan_rename_part(&srcs, "MessageStore", "chat.data.Store").unwrap();
    // E3: the radius names every touch point before anything applies —
    // the bare decl, the dotted layer decl, and both chain references.
    assert!(
        plan.changes.len() >= 4,
        "radius too small: {:#?}",
        plan.changes
    );
    assert!(plan.changes.iter().any(|c| c.file == "base.ash" && c.old == "Store"));
    assert!(plan
        .changes
        .iter()
        .any(|c| c.file == "audit.ash" && c.old == "chat.data.Store"));
    assert!(plan.changes.iter().filter(|c| c.file == "api.ash").count() >= 2);

    let after = refactor::execute(&srcs, &plan).unwrap();

    // E2: no stale reference — the old name is gone at the token level.
    for (path, text) in &after {
        let (toks, _) = ashlar::lexer::lex(path, text);
        assert!(
            !toks
                .iter()
                .any(|t| matches!(&t.tok, ashlar::tokens::Tok::Ident(s) if s == "Store")),
            "{} still references `Store`",
            path
        );
    }
    // The renamed program is clean (execute verified it), and the new
    // name is present.
    assert!(after["base.ash"].contains("part MessageStore"));
    assert!(after["audit.ash"].contains("part chat.data.MessageStore"));

    // E4: forward then back yields byte-identical sources.
    let after_vec: Vec<(String, String)> =
        after.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
    let back_plan =
        refactor::plan_rename_part(&after_vec, "Store", "chat.data.MessageStore").unwrap();
    let restored = refactor::execute(&after_vec, &back_plan).unwrap();
    for (path, original) in &srcs {
        assert_eq!(
            &restored[path], original,
            "{}: roundtrip is not byte-identical",
            path
        );
    }
}

#[test]
fn t_e_refusals_are_total() {
    // covers: E5
    // Broken project: refuse, apply nothing.
    let broken = sources(&[("a.ash", "space a\n\npart W {\n  x: text\n  y = nulll\n}\n")]);
    let r = refactor::plan_rename_part(&broken, "V", "a.W");
    assert!(r.is_err());
    assert!(r.err().unwrap().0.contains("diagnostic"));

    // Unknown part.
    let ok = sources(&[("a.ash", "space a\n\npart W {\n  x: text\n}\n")]);
    assert!(refactor::plan_rename_part(&ok, "V", "a.Nope").is_err());

    // Data-shape field rename: radius not computable yet — refuse with
    // the reason, not a partial application.
    let r = refactor::plan_rename_prop(
        &sources(&[("base.ash", BASE), ("audit.ash", AUDIT), ("api.ash", API)]),
        "chat.data.Message",
        "body",
        "content",
    );
    let msg = r.err().unwrap().0;
    assert!(msg.contains("field"), "{}", msg);
    assert!(msg.contains("cannot be computed"), "{}", msg);
}

#[test]
fn t_e_rename_state_prop_rewrites_stack_keys_and_roundtrips() {
    // covers: E1, E2, E4 (the stack-return-key case)
    let app = r#"space srv

part Server {
  port = 8080
  state ready: bool = false
  start stack = () => {
    return { ready: true }
  }
  check = () => ready
}
"#;
    let srcs = sources(&[("app.ash", app)]);
    let plan = refactor::plan_rename_prop(&srcs, "srv.Server", "ready", "prepared").unwrap();
    // Declaration, stack-return key, and the bare read in `check`.
    assert_eq!(plan.changes.len(), 3, "{:#?}", plan.changes);

    let after = refactor::execute(&srcs, &plan).unwrap();
    assert!(after["app.ash"].contains("state prepared: bool"));
    assert!(after["app.ash"].contains("{ prepared: true }"));
    assert!(after["app.ash"].contains("=> prepared"));

    let after_vec: Vec<(String, String)> =
        after.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
    let back = refactor::plan_rename_prop(&after_vec, "srv.Server", "prepared", "ready").unwrap();
    let restored = refactor::execute(&after_vec, &back).unwrap();
    assert_eq!(restored["app.ash"], app);
}

#[test]
fn t_e_rekind_roundtrip_and_rollback() {
    // covers: E1, E4, E5 (post-verify rollback)
    let app = r#"space cfg

part Config {
  tags append: [text] = ["core"]
}

// second space layers it
"#;
    let ext = r#"space cfg.ext
use cfg

part cfg.Config {
  tags append = ["extra"]
}
"#;
    let srcs = sources(&[
        ("a.ash", &app.replace("\n// second space layers it\n", "")),
        ("b.ash", ext),
    ]);

    // append -> deep on every layer, and back.
    let plan = refactor::plan_rekind(&srcs, "cfg.Config", "tags", "deep").unwrap();
    assert_eq!(plan.changes.len(), 2);
    let after = refactor::execute(&srcs, &plan).unwrap();
    assert!(after["a.ash"].contains("tags deep: [text]"));
    assert!(after["b.ash"].contains("tags deep ="));

    let after_vec: Vec<(String, String)> =
        after.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
    let back = refactor::plan_rekind(&after_vec, "cfg.Config", "tags", "append").unwrap();
    let restored = refactor::execute(&after_vec, &back).unwrap();
    for (path, original) in &srcs {
        assert_eq!(&restored[path], original, "{}: rekind roundtrip", path);
    }

    // Rolling a number-valued property to `append` cannot check (E028):
    // the execute step must refuse and produce nothing.
    let num = sources(&[("n.ash", "space n\n\npart W {\n  count = 3\n}\n")]);
    let plan = refactor::plan_rekind(&num, "n.W", "count", "append").unwrap();
    let r = refactor::execute(&num, &plan);
    let msg = r.err().unwrap().0;
    assert!(msg.contains("rolled back"), "{}", msg);
}
