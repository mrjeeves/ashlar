//! Repo policy: stub macros never survive to a commit. Not one of the
//! lettered requirements, but enforced here because nothing else in the
//! suite would catch it.

mod support;

#[test]
fn t_no_stub_macros_in_src() {
    // covers: repo policy (no todo!()/unimplemented!() left in crates/ashlar/src)
    let root = support::repo_root();
    let src_dir = root.join("crates/ashlar/src");

    let mut files: Vec<std::path::PathBuf> = Vec::new();
    let mut stack = vec![src_dir.clone()];
    while let Some(dir) = stack.pop() {
        let entries = std::fs::read_dir(&dir)
            .unwrap_or_else(|e| panic!("cannot read {}: {}", dir.display(), e));
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().map(|e| e == "rs").unwrap_or(false) {
                files.push(path);
            }
        }
    }
    files.sort();
    assert!(!files.is_empty(), "found no .rs files under {}", src_dir.display());

    let mut offenders = Vec::new();
    for f in &files {
        let content = support::read_text(f);
        if content.contains("todo!(") || content.contains("unimplemented!(") {
            offenders.push(f.display().to_string());
        }
    }

    assert!(
        offenders.is_empty(),
        "stub macros (todo!()/unimplemented!()) found in {} file(s) — these must not survive to a \
         commit:\n{}",
        offenders.len(),
        offenders.join("\n")
    );
}
