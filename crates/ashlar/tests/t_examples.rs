//! The examples directory is showcase AND corpus: every example project
//! must check with zero diagnostics and already be in canonical format.
//! An example that stops compiling is a broken shop window — this suite
//! makes that a test failure, not a discovery.

use std::path::PathBuf;

fn examples_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples")
}

#[test]
fn t_examples_all_check_clean() {
    let root = examples_root();
    let mut seen = 0;
    for entry in std::fs::read_dir(&root).expect("examples/ exists") {
        let dir = entry.unwrap().path();
        if !dir.is_dir() {
            continue;
        }
        seen += 1;
        let r = ashlar::check_project(&dir);
        assert!(
            r.diags.is_empty(),
            "example `{}` has diagnostics:\n{}",
            dir.display(),
            r.diags.iter().map(|d| d.human()).collect::<Vec<_>>().join("\n")
        );
        assert!(
            !r.program.parts.is_empty(),
            "example `{}` declares no parts",
            dir.display()
        );
    }
    assert!(seen >= 3, "expected at least hello/counter/chat, found {}", seen);
}

#[test]
fn t_examples_are_canonically_formatted() {
    let root = examples_root();
    for entry in std::fs::read_dir(&root).expect("examples/ exists") {
        let dir = entry.unwrap().path();
        if !dir.is_dir() {
            continue;
        }
        for file in ashlar::find_ash_files(&dir) {
            let src = std::fs::read_to_string(&file).unwrap();
            let rel = file.to_string_lossy().to_string();
            let formatted = ashlar::fmt::format_source(&rel, &src)
                .unwrap_or_else(|d| panic!("{} does not format: {:?}", rel, d));
            assert_eq!(
                formatted, src,
                "{} is not canonically formatted; run `ashlar fmt examples`",
                rel
            );
        }
    }
}
