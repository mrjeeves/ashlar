//! T-F — build determinism: repeated checks of identical input are
//! byte-identical (F2), and relocating files without changing their
//! contents changes nothing but recorded locations (F3, and the layer
//! order invariant B1 that F3 depends on).

mod support;

/// Three spaces, one layered part (`Widget`, base-layered in `base`,
/// layered again in `mid`), at the three given paths.
fn fixture(paths: [&str; 3]) -> Vec<(String, String)> {
    let base = "space base\n\npart Widget {\n  color = \"blue\"\n}\n";
    let mid = "space mid\nuse base\n\npart base.Widget {\n  size = \"large\"\n}\n";
    let top = "space top\nuse mid\n\npart Root {\n  ready = true\n}\n";
    vec![
        (paths[0].to_string(), base.to_string()),
        (paths[1].to_string(), mid.to_string()),
        (paths[2].to_string(), top.to_string()),
    ]
}

#[test]
fn t_f2_repeated_check_is_byte_identical() {
    // covers: F2
    let paths = ["x1/base.ash", "x2/mid.ash", "x3/top.ash"];
    let sources = fixture(paths);

    let r1 = ashlar::check_sources(sources.clone());
    let r2 = ashlar::check_sources(sources);

    let m1 = ashlar::manifest::render(&r1.program, &r1.composed);
    let m2 = ashlar::manifest::render(&r2.program, &r2.composed);
    assert_eq!(
        m1, m2,
        "F2: checking identical input twice produced two different manifests"
    );
}

#[test]
fn t_f3_relocation_invariance_and_b1_layer_order() {
    // covers: F3, B1
    let paths1 = ["x1/base.ash", "x2/mid.ash", "x3/top.ash"];
    let paths2 = [
        "y1/base_relocated.ash",
        "y2/mid_relocated.ash",
        "y3/top_relocated.ash",
    ];

    let sources1 = fixture(paths1);
    let sources2 = fixture(paths2);

    let r1 = ashlar::check_sources(sources1);
    let r2 = ashlar::check_sources(sources2);

    // B1: layer order comes from the use graph (declarations), never from
    // file paths, so it must survive relocation unchanged.
    let layers_of = |program: &ashlar::resolved::Program| -> Vec<String> {
        program
            .parts
            .get("base.Widget")
            .unwrap_or_else(|| {
                panic!(
                    "expected a composed part `base.Widget`; parts present: {:?}",
                    program.parts.keys().collect::<Vec<_>>()
                )
            })
            .layers
            .iter()
            .map(|l| l.space.clone())
            .collect()
    };
    let layers1 = layers_of(&r1.program);
    let layers2 = layers_of(&r2.program);
    assert_eq!(
        layers1, layers2,
        "B1: `base.Widget`'s layer order changed when its files were relocated"
    );
    assert_eq!(
        layers1,
        vec!["base".to_string(), "mid".to_string()],
        "B1: expected base-then-mid composition order (mid uses base)"
    );

    // F3: manifests must match once file-path text is normalized away.
    let m1 = ashlar::manifest::render(&r1.program, &r1.composed);
    let m2 = ashlar::manifest::render(&r2.program, &r2.composed);

    let paths1_owned: Vec<String> = paths1.iter().map(|s| s.to_string()).collect();
    let paths2_owned: Vec<String> = paths2.iter().map(|s| s.to_string()).collect();
    let n1 = support::normalize_paths(&m1, &paths1_owned);
    let n2 = support::normalize_paths(&m2, &paths2_owned);
    assert_eq!(
        n1, n2,
        "F3: manifests differ after normalizing recorded file paths to F0/F1/F2 placeholders — \
         relocating files with unchanged contents must not change program meaning"
    );
}
