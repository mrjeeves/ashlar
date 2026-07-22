//! T-A1 — reference size gate.

mod support;

#[test]
fn t_a1_reference_size_gate() {
    // covers: A1
    let root = support::repo_root();
    let path = root.join("reference/ashlar.md");
    let text = support::read_text(&path);
    let len = text.len(); // UTF-8 byte length

    assert!(
        len <= 40_000,
        "reference/ashlar.md is {} bytes — over the 40_000 hard cap (A1); the reference must stay a \
         single readable document, not grow without bound",
        len
    );
    assert!(
        len >= 10_000,
        "reference/ashlar.md is only {} bytes — under the 10_000 sanity floor (A1); this looks like an \
         accidental truncation, not a real reference",
        len
    );
}
