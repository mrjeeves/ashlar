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

    for f in support::ash_files_sorted(&root.join("suites/t_a3")) {
        checked_any = true;
        let content = support::read_text(&f);
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
