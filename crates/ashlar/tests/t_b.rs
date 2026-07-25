//! T-B — resolution: transitive visibility, the three name-resolution
//! errors (E001/E002/E003), and the no-locations-in-source invariant (B5).

mod support;

#[test]
fn t_b_transitive_visibility_resolves_bare_name_with_no_errors() {
    // covers: B3, B7
    let sources = vec![
        (
            "a.ash".to_string(),
            "space demo.a\n\npart X {\n  greeting = \"hi\"\n}\n".to_string(),
        ),
        ("b.ash".to_string(), "space demo.b\nuse demo.a\n".to_string()),
        (
            "c.ash".to_string(),
            "space demo.c\nuse demo.b\n\npart Checker {\n  value = X.greeting\n}\n".to_string(),
        ),
    ];

    let result = ashlar::check_sources(sources);
    assert!(
        !result.has_errors(),
        "space c uses b uses a; X is declared in a and referenced bare in c — this must resolve \
         through the transitive use closure with no errors, got: {:#?}",
        result.diags
    );
}

#[test]
fn t_b_zero_resolution_is_e001() {
    // covers: B3
    let sources = vec![(
        "z.ash".to_string(),
        "space demo.z\n\npart P {\n  value = ThisNameIsNotDeclaredAnywhere\n}\n".to_string(),
    )];

    let result = ashlar::check_sources(sources);
    assert!(
        result.diags.iter().any(|d| d.id == "E001"),
        "a name with zero resolutions must produce E001, got: {:#?}",
        result.diags
    );
}

#[test]
fn t_b_multi_resolution_is_e002() {
    // covers: B3
    let sources = vec![
        ("m1.ash".to_string(), "space demo.m1\n\npart Dup {\n  x = 1\n}\n".to_string()),
        ("m2.ash".to_string(), "space demo.m2\n\npart Dup {\n  y = 2\n}\n".to_string()),
        (
            "m3.ash".to_string(),
            "space demo.m3\nuse demo.m1\nuse demo.m2\n\npart Checker {\n  value = Dup\n}\n".to_string(),
        ),
    ];

    let result = ashlar::check_sources(sources);
    assert!(
        result.diags.iter().any(|d| d.id == "E002"),
        "`Dup` is visible from both demo.m1 and demo.m2 with no `use` ordering either over the \
         other — the bare reference in demo.m3 must produce E002, got: {:#?}",
        result.diags
    );
}

#[test]
fn t_b_case_collision_is_e003() {
    // covers: B4
    let sources = vec![(
        "cc.ash".to_string(),
        "space demo.cc\n\npart P {\n  userName = 1\n  user_name = 2\n}\n".to_string(),
    )];

    let result = ashlar::check_sources(sources);
    assert!(
        result.diags.iter().any(|d| d.id == "E003"),
        "`userName` and `user_name` differ only by separator convention in one scope — this must \
         produce E003, got: {:#?}",
        result.diags
    );
}

#[test]
fn t_b5_no_locations_in_fixtures_or_reference() {
    // covers: B5
    const FORBIDDEN: &[&str] = &["http://", "https://", "./", "../", ".ash"];
    let root = support::repo_root();
    let mut checked_any = false;

    // The t_a3 corpus models multi-file programs inside one snippet with a
    // `// file: x.ash` comment line (see suites/t_a3/PROTOCOL.md). That
    // marker is presentation, not source semantics — a comment binds
    // nothing — so exactly those lines are exempt; every other line of
    // every snippet is scanned at full strength.
    fn strip_file_markers(content: &str) -> String {
        content
            .lines()
            .filter(|l| !l.trim_start().starts_with("// file:"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    for f in support::ash_files_sorted(&root.join("suites/t_a3")) {
        checked_any = true;
        let content = strip_file_markers(&support::read_text(&f));
        for token in FORBIDDEN {
            assert!(
                !content.contains(token),
                "B5: {} contains forbidden substring `{}` — Ashlar source never encodes a location",
                f.display(),
                token
            );
        }
    }

    let reference_text = support::read_text(&root.join("reference/ashlar.md"));
    let blocks = support::extract_ash_blocks(&reference_text);
    for (i, block) in blocks.iter().enumerate() {
        checked_any = true;
        for token in FORBIDDEN {
            assert!(
                !block.contains(token),
                "B5: reference ```ash block #{} contains forbidden substring `{}`",
                i + 1,
                token
            );
        }
    }

    assert!(
        checked_any,
        "B5 checked nothing: expected at least the reference's ```ash blocks to exist"
    );
}

#[test]
fn t_b_foreign_binding_key_naming_no_space_is_e001() {
    // covers: B3 — `foreign.json` keys deployment facts by SPACE NAME, which
    // makes it the one non-`.ash` file carrying a name the compiler reasons
    // about. A key that names no space is a name resolving to nothing, and B3
    // says that is an error. It used to pass `check` in silence and quietly
    // fall back to the derived native library path.
    let proj = std::env::temp_dir().join(format!("ashlar_tb_fbind_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&proj);
    std::fs::create_dir_all(&proj).unwrap();
    std::fs::write(
        proj.join("app.ash"),
        "space tools\n\nforeign shout: (s: text) -> text\n\npart app {\n  port = 8080\n}\n",
    )
    .unwrap();

    // The correct key checks clean — no false positive on a real binding.
    std::fs::write(
        proj.join("foreign.json"),
        "{ \"tools\": { \"via\": \"worker\", \"run\": [\"python3\", \"w.py\"] } }\n",
    )
    .unwrap();
    let ok = ashlar::check_project(&proj);
    assert!(ok.diags.is_empty(), "a correct binding must check clean: {:?}", ok.diags);

    // A near-miss key is E001, and the correction names the space it meant.
    std::fs::write(
        proj.join("foreign.json"),
        "{ \"tool\": { \"via\": \"worker\", \"run\": [\"python3\", \"w.py\"] } }\n",
    )
    .unwrap();
    let bad = ashlar::check_project(&proj);
    let d = bad
        .diags
        .iter()
        .find(|d| d.id == "E001")
        .unwrap_or_else(|| panic!("expected E001 for an unbound key, got {:?}", bad.diags));
    assert_eq!(d.req, "B3");
    assert!(d.file.ends_with("foreign.json"), "loc must name the binding file: {}", d.file);
    assert!(d.cause.contains("`tool`"), "cause must name the key: {}", d.cause);
    assert!(
        d.fix.as_ref().map(|f| f.note.contains("tools")) == Some(true),
        "D1: the correction must name the space it meant: {:?}",
        d.fix
    );

    // A binding whose space exists but declares no `foreign` is inert, not
    // wrong — staying silent there is the no-false-positives rule.
    std::fs::write(proj.join("other.ash"), "space spare\n\npart Thing {\n  x = 1\n}\n").unwrap();
    std::fs::write(
        proj.join("foreign.json"),
        "{ \"spare\": { \"via\": \"worker\", \"run\": [\"python3\", \"w.py\"] } }\n",
    )
    .unwrap();
    let inert = ashlar::check_project(&proj);
    assert!(
        inert.diags.is_empty(),
        "an inert binding must not be a diagnostic: {:?}",
        inert.diags
    );

    // An unparseable binding file is loud rather than a silent derived default.
    std::fs::write(proj.join("foreign.json"), "{ \"spare\": { } }\n").unwrap();
    let broken = ashlar::check_project(&proj);
    assert!(
        broken.diags.iter().any(|d| d.id == "E001" && d.cause.contains("unreadable")),
        "a malformed binding file must be reported: {:?}",
        broken.diags
    );

    let _ = std::fs::remove_dir_all(&proj);
}
