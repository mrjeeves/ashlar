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

    // Unknown space, colliding space, illegal name.
    assert!(refactor::plan_rename_space(&ok, "nope", "b").is_err());
    assert!(refactor::plan_rename_space(&ok, "a", "std").is_err());
    assert!(refactor::plan_rename_space(&ok, "a", "9x").is_err());

    // Move to a space that does not exist, and onto a name collision.
    assert!(refactor::plan_move(&ok, "a.W", "nowhere").is_err());
    let two = sources(&[
        ("a.ash", "space a\n\npart W {\n  x: text\n}\n"),
        ("b.ash", "space b\n\npart W {\n  y: text\n}\n"),
    ]);
    let r = refactor::plan_move(&two, "a.W", "b");
    assert!(r.err().unwrap().0.contains("already exists"));
}

#[test]
fn t_e_rename_data_shape_field_via_site_index_and_roundtrip() {
    // covers: E1, E2, E4, E5-closed (ADR-0009): the checker's field-site
    // index makes the field rename's radius computable — declaration,
    // literal construction keys, and known-base accesses alike.
    let srcs = sources(&[("base.ash", BASE), ("audit.ash", AUDIT), ("api.ash", API)]);
    let plan =
        refactor::plan_rename_prop(&srcs, "chat.data.Message", "body", "content").unwrap();
    // The field declaration and the literal key in the api call.
    assert!(
        plan.changes
            .iter()
            .any(|c| c.file == "base.ash" && c.span.start.line == 5),
        "declaration missing: {:#?}",
        plan.changes
    );
    assert!(
        plan.changes
            .iter()
            .any(|c| c.file == "api.ash" && c.old == "body"),
        "literal key missing: {:#?}",
        plan.changes
    );

    let after = refactor::execute(&srcs, &plan).unwrap();
    for (path, text) in &after {
        let (toks, _) = ashlar::lexer::lex(path, text);
        assert!(
            !toks
                .iter()
                .any(|t| matches!(&t.tok, ashlar::tokens::Tok::Ident(s) if s == "body")),
            "{} still references `body`",
            path
        );
    }

    let after_vec: Vec<(String, String)> =
        after.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
    let back =
        refactor::plan_rename_prop(&after_vec, "chat.data.Message", "content", "body").unwrap();
    let restored = refactor::execute(&after_vec, &back).unwrap();
    for (path, original) in &srcs {
        assert_eq!(&restored[path], original, "{}: field roundtrip", path);
    }
}

#[test]
fn t_e_rename_view_field_covers_el_keys() {
    // covers: E2 for view fields — `el(Part, { field: ... })` keys are
    // construction sites the index supplies.
    let app = r#"space ui

part counter {
  label: text
  state n: number = 0
  view = () => el(counter, { label: "hits" })
}
"#;
    let srcs = sources(&[("app.ash", app)]);
    let plan = refactor::plan_rename_prop(&srcs, "ui.counter", "label", "title").unwrap();
    let after = refactor::execute(&srcs, &plan).unwrap();
    assert!(after["app.ash"].contains("{ title: \"hits\" }"));
    assert!(after["app.ash"].contains("title: text"));

    let after_vec: Vec<(String, String)> =
        after.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
    let back = refactor::plan_rename_prop(&after_vec, "ui.counter", "title", "label").unwrap();
    let restored = refactor::execute(&after_vec, &back).unwrap();
    assert_eq!(restored["app.ash"], app);
}

#[test]
fn t_e_rename_space_rewrites_everything_and_roundtrips() {
    // covers: E1, E2, E3, E4 for spaces (reference §11: rename covers
    // spaces) — headers, `use` lines, dotted layer declarations, chain
    // references, and shape positions.
    let srcs = sources(&[("base.ash", BASE), ("audit.ash", AUDIT), ("api.ash", API)]);
    let plan = refactor::plan_rename_space(&srcs, "chat.data", "chat.store").unwrap();
    assert!(
        plan.changes
            .iter()
            .any(|c| c.file == "base.ash" && c.span.start.line == 1),
        "header missing: {:#?}",
        plan.changes
    );
    assert!(
        plan.changes
            .iter()
            .any(|c| c.file == "audit.ash" && c.span.start.line == 2),
        "use line missing: {:#?}",
        plan.changes
    );

    let after = refactor::execute(&srcs, &plan).unwrap();
    for (path, text) in &after {
        assert!(
            !text.contains("chat.data"),
            "{} still references `chat.data`:\n{}",
            path,
            text
        );
    }
    assert!(after["base.ash"].starts_with("space chat.store\n"));
    assert!(after["audit.ash"].contains("part chat.store.Store"));

    // The stored part's keys migrate with the space.
    assert!(plan
        .state_part_renames
        .contains(&("chat.data.Store".to_string(), "chat.store.Store".to_string())));

    let after_vec: Vec<(String, String)> =
        after.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
    let back = refactor::plan_rename_space(&after_vec, "chat.store", "chat.data").unwrap();
    let restored = refactor::execute(&after_vec, &back).unwrap();
    for (path, original) in &srcs {
        assert_eq!(&restored[path], original, "{}: space roundtrip", path);
    }
}

const MOVE_A: &str = r#"space a

part One {
  greet = () => "hi"
}

part Two {
  n = 1
}
"#;

const MOVE_B: &str = r#"space b
use a

part Three {
  m = () => Two.n + 1
}
"#;

#[test]
fn t_e_move_canonical_roundtrip_is_byte_identical() {
    // covers: E4, E6 — moving `a.Two` to `b` (already-visible spaces,
    // part at canonical end-of-file position, no `use` additions needed
    // either way) then back restores every byte (ADR-0009's contract).
    let srcs = sources(&[("a.ash", MOVE_A), ("b.ash", MOVE_B)]);
    let plan = refactor::plan_move(&srcs, "a.Two", "b").unwrap();
    let after = refactor::execute(&srcs, &plan).unwrap();
    assert!(!after["a.ash"].contains("part Two"));
    assert!(after["b.ash"].contains("part Two"));
    assert!(after["b.ash"].ends_with("part Two {\n  n = 1\n}\n"));

    let after_vec: Vec<(String, String)> =
        after.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
    let back = refactor::plan_move(&after_vec, "b.Two", "a").unwrap();
    let restored = refactor::execute(&after_vec, &back).unwrap();
    for (path, original) in &srcs {
        assert_eq!(&restored[path], original, "{}: move roundtrip", path);
    }
}

#[test]
fn t_e_move_adds_the_use_lines_both_sides_need() {
    // covers: E2/E6 — a third space that saw the part through the OLD
    // home gains `use` of the new home; the moved body's own
    // dependencies come along too.
    let c = r#"space c
use a

part Four {
  k = () => Two.n + One.greet()
}
"#;
    let b_no_use = r#"space b

part Three {
  m = 2
}
"#;
    // Two's body references One (same old space), so the target space
    // must gain `use a`; c references Two, so c must gain `use b`.
    let a2 = r#"space a

part One {
  greet = () => "hi"
}

part Two {
  n = len(One.greet())
}
"#;
    let srcs = sources(&[("a.ash", a2), ("b.ash", b_no_use), ("c.ash", c)]);
    let plan = refactor::plan_move(&srcs, "a.Two", "b").unwrap();
    let after = refactor::execute(&srcs, &plan).unwrap();
    assert!(
        after["b.ash"].contains("use a"),
        "target space missing the body's dependency:\n{}",
        after["b.ash"]
    );
    assert!(
        after["c.ash"].contains("use b"),
        "referencing space missing `use b`:\n{}",
        after["c.ash"]
    );
    // The whole result checks clean (execute verified it).
    let after_vec: Vec<(String, String)> =
        after.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
    let r = ashlar::check_sources(after_vec);
    assert!(r.diags.is_empty(), "{:?}", r.diags);
}

#[test]
fn t_e_rename_stored_prop_and_part_carry_state_migrations() {
    // covers: the ADR-0007 orphaned-rows note, closed — plans name the
    // stored keys they migrate, and the CLI applies them to
    // `.ashlar-state.json`.
    let srcs = sources(&[("base.ash", BASE), ("audit.ash", AUDIT), ("api.ash", API)]);
    let plan =
        refactor::plan_rename_prop(&srcs, "chat.data.Store", "messages", "msgs").unwrap();
    assert_eq!(
        plan.state_prop_renames,
        vec![(
            "chat.data.Store.messages".to_string(),
            "chat.data.Store.msgs".to_string()
        )]
    );

    let plan = refactor::plan_rename_part(&srcs, "MessageStore", "chat.data.Store").unwrap();
    assert_eq!(
        plan.state_part_renames,
        vec![(
            "chat.data.Store".to_string(),
            "chat.data.MessageStore".to_string()
        )]
    );

    // Non-stored props carry no migration.
    let app = sources(&[(
        "app.ash",
        "space srv\n\npart Server {\n  state ready: bool = false\n  start stack = () => {\n    return { ready: true }\n  }\n}\n",
    )]);
    let plan = refactor::plan_rename_prop(&app, "srv.Server", "ready", "prepared").unwrap();
    assert!(plan.state_prop_renames.is_empty());
}

#[test]
fn t_e_state_file_migrates_end_to_end() {
    // The CLI applies the migration to a real `.ashlar-state.json`.
    let dir = std::env::temp_dir().join(format!("ashlar_te_mig_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("base.ash"), BASE).unwrap();
    std::fs::write(
        dir.join(".ashlar-state.json"),
        r#"{"chat.data.Store.messages":{"a":{"id":"a","body":"hi"}},"__users":{}}"#,
    )
    .unwrap();
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_ashlar"))
        .args(["rename", "chat.data.Store.messages", "msgs"])
        .current_dir(&dir)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let state = std::fs::read_to_string(dir.join(".ashlar-state.json")).unwrap();
    assert!(state.contains("chat.data.Store.msgs"), "{}", state);
    assert!(!state.contains("chat.data.Store.messages"), "{}", state);
    assert!(state.contains("__users"), "{}", state);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn t_e_radius_prints_without_touching() {
    // covers: E3 — `ashlar radius` reports every site and changes nothing.
    let dir = std::env::temp_dir().join(format!("ashlar_te_radius_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("base.ash"), BASE).unwrap();
    std::fs::write(dir.join("audit.ash"), AUDIT).unwrap();
    std::fs::write(dir.join("api.ash"), API).unwrap();
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_ashlar"))
        .args(["radius", "chat.data.Store"])
        .current_dir(&dir)
        .output()
        .unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("radius of `chat.data.Store`"), "{}", stdout);
    assert!(stdout.contains("audit.ash"), "{}", stdout);
    // Nothing was touched.
    assert_eq!(std::fs::read_to_string(dir.join("base.ash")).unwrap(), BASE);
    assert_eq!(std::fs::read_to_string(dir.join("audit.ash")).unwrap(), AUDIT);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn t_e_vendor_copies_checks_and_refuses_collisions() {
    // covers: reference §11 `vendor` — copy-in, space collision refusal,
    // and rollback when the combined project cannot check.
    let proj = std::env::temp_dir().join(format!("ashlar_te_vendor_{}", std::process::id()));
    let ext = std::env::temp_dir().join(format!("ashlar_te_vendor_ext_{}", std::process::id()));
    for d in [&proj, &ext] {
        let _ = std::fs::remove_dir_all(d);
        std::fs::create_dir_all(d).unwrap();
    }
    std::fs::write(proj.join("app.ash"), "space app\n\npart Main {\n  x = 1\n}\n").unwrap();
    std::fs::write(ext.join("lib.ash"), "space extlib\n\npart Helper {\n  y = 2\n}\n").unwrap();

    let out = std::process::Command::new(env!("CARGO_BIN_EXE_ashlar"))
        .args(["vendor", ext.to_str().unwrap()])
        .current_dir(&proj)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let vendored = proj
        .join("vendor")
        .join(ext.file_name().unwrap())
        .join("lib.ash");
    assert!(vendored.exists());
    // The combined project checks clean and the vendored space resolves.
    let r = ashlar::check_project(&proj);
    assert!(r.diags.is_empty(), "{:?}", r.diags);
    assert!(r.program.spaces.contains_key("extlib"));

    // A second vendor of the same tree refuses (already vendored).
    let out2 = std::process::Command::new(env!("CARGO_BIN_EXE_ashlar"))
        .args(["vendor", ext.to_str().unwrap()])
        .current_dir(&proj)
        .output()
        .unwrap();
    assert!(!out2.status.success());

    // A tree colliding with an existing space refuses before copying.
    let ext2 = std::env::temp_dir().join(format!("ashlar_te_vendor_c_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&ext2);
    std::fs::create_dir_all(&ext2).unwrap();
    std::fs::write(ext2.join("evil.ash"), "space app\n\npart Sneak {\n  z = 3\n}\n").unwrap();
    let out3 = std::process::Command::new(env!("CARGO_BIN_EXE_ashlar"))
        .args(["vendor", ext2.to_str().unwrap()])
        .current_dir(&proj)
        .output()
        .unwrap();
    assert!(!out3.status.success());
    let msg = String::from_utf8_lossy(&out3.stderr).to_string();
    assert!(msg.contains("already has"), "{}", msg);
    assert!(!proj.join("vendor").join(ext2.file_name().unwrap()).exists());

    for d in [&proj, &ext, &ext2] {
        let _ = std::fs::remove_dir_all(d);
    }
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
