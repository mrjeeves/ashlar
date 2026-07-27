//! Semantic delta against the last build (requirement C9).
//!
//! Determinism is not enough. A `use` edge added by hand is a one-line edit
//! that can reorder every layered property downstream, and `ashlar check`
//! answered it with silence: exit 0, no output, different program. ADR-0012
//! accepted that "deterministic but silent behavioral change remains a
//! failure" and nothing was built to catch it; ADR-0032 is the finding and
//! this module is the answer.
//!
//! The baseline is the previous `ashlar.manifest`, which already records every
//! part's layers in composition order (§10). Nothing new has to be derived —
//! only compared. There is no parser to add either: `eval::from_json` is how
//! `foreign.rs` and `settings.rs` already read the project's JSON files.
//!
//! **No baseline means no delta, and that is deliberate.** The manifest is
//! gitignored, so a fresh clone and a CI job have nothing to compare against
//! and this module stays quiet. The case it is built for is the one that
//! actually bites: an agent editing in a live working tree, where the previous
//! build is sitting right there.

use crate::diag::{Diag, Level, W002_ORDER_CHANGED};
use crate::eval::{from_json, V};
use crate::resolved::Program;
use std::collections::BTreeMap;
use std::path::Path;

/// The previous build's derived state, as far as a delta needs it.
#[derive(Debug, Default, Clone)]
pub struct Baseline {
    /// Full part name -> the spaces of its layers, in composition order.
    pub layers: BTreeMap<String, Vec<String>>,
}

/// Read `ashlar.manifest` from `root`. Returns `None` when it is absent or
/// unreadable — a missing baseline is not an error, it is the first build.
pub fn load(root: &Path) -> Option<Baseline> {
    let text = std::fs::read_to_string(root.join("ashlar.manifest")).ok()?;
    parse(&text)
}

/// Parse a manifest's `parts.*.layers[].space` into a `Baseline`. A manifest
/// this cannot understand yields `None` rather than a wrong answer: reporting
/// a reorder that did not happen would be worse than reporting nothing.
pub fn parse(text: &str) -> Option<Baseline> {
    let V::Map(root) = from_json(text)? else {
        return None;
    };
    let Some(V::Map(parts)) = root.get("parts") else {
        return None;
    };
    let mut layers = BTreeMap::new();
    for (full, info) in parts {
        let V::Map(info) = info else { continue };
        let Some(V::List(ls)) = info.get("layers") else {
            continue;
        };
        let mut spaces = Vec::new();
        for l in ls {
            let V::Map(l) = l else { continue };
            if let Some(V::Text(s)) = l.get("space") {
                spaces.push(s.clone());
            }
        }
        layers.insert(full.clone(), spaces);
    }
    Some(Baseline { layers })
}

/// One part whose layers changed position between builds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrderChange {
    pub part: String,
    pub before: Vec<String>,
    pub after: Vec<String>,
}

/// Parts present in both builds whose composition order differs.
///
/// A part that gained or lost a layer is NOT an order change: the author added
/// or removed a declaration and can see that in their own diff. What this
/// reports is the invisible case — the same layers, resequenced by an edit
/// somewhere else in the use graph.
pub fn order_changes(base: &Baseline, program: &Program) -> Vec<OrderChange> {
    let mut out = Vec::new();
    for (full, info) in &program.parts {
        let Some(before) = base.layers.get(full) else {
            continue;
        };
        let after: Vec<String> = info.layers.iter().map(|l| l.space.clone()).collect();
        if before == &after {
            continue;
        }
        let mut b = before.clone();
        let mut a = after.clone();
        b.sort();
        a.sort();
        if b != a {
            continue; // layers added or removed: visible in the author's diff
        }
        out.push(OrderChange {
            part: full.clone(),
            before: before.clone(),
            after,
        });
    }
    out
}

/// W002 for each reordered part, anchored at the layer that moved earliest.
///
/// The anchor is the first layer whose position changed, which is the one that
/// gained precedence over something it did not have before — the declaration a
/// reader needs to look at. Naming the `use` edge responsible would be a
/// stronger report and is not always derivable: a reorder can follow from a
/// path through several spaces, none of which the author touched.
pub fn diagnostics(base: &Baseline, program: &Program) -> Vec<Diag> {
    let mut out = Vec::new();
    for change in order_changes(base, program) {
        let info = &program.parts[&change.part];
        let moved = change
            .before
            .iter()
            .zip(change.after.iter())
            .position(|(b, a)| b != a)
            .unwrap_or(0);
        let layer = &info.layers[moved];
        let decl = program.part_decl(layer);
        out.push(
            Diag::new(
                W002_ORDER_CHANGED,
                Level::Warn,
                program.file_path(layer),
                decl.name_span,
                format!(
                    "composition order of `{}` changed since the last build: {} -> {}.",
                    change.part,
                    change.before.join(", "),
                    change.after.join(", ")
                ),
            )
            .with_fix(
                format!(
                    "Nothing is broken; confirm this was intended. `{}` now runs its layers in \
                     the new order, so every `stack`, `pipe`, `append` and `deep` property on it \
                     composes differently. Run `ashlar delta` for the full report.",
                    change.part
                ),
                vec![],
            ),
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const MANIFEST: &str = r#"{
      "format": 1,
      "order": ["std", "base", "alpha", "zulu"],
      "spaces": {},
      "parts": {
        "base.Chain": { "home": "base", "layers": [
          {"space":"base","file":"base.ash","line":3},
          {"space":"alpha","file":"alpha.ash","line":4},
          {"space":"zulu","file":"zulu.ash","line":4}
        ]}
      },
      "foreigns": {},
      "assets": {}
    }"#;

    #[test]
    fn parses_layer_order_out_of_a_manifest() {
        let b = parse(MANIFEST).expect("parses");
        assert_eq!(
            b.layers.get("base.Chain").unwrap(),
            &vec!["base".to_string(), "alpha".to_string(), "zulu".to_string()]
        );
    }

    #[test]
    fn a_manifest_it_cannot_read_is_none_not_a_wrong_answer() {
        assert!(parse("not json").is_none());
        assert!(parse(r#"{"format":1}"#).is_none());
    }

    #[test]
    fn reordering_is_reported_and_resequencing_alone_is_what_counts() {
        let base = parse(MANIFEST).unwrap();
        // Same three layers, zulu now ahead of alpha.
        let mut flipped = base.clone();
        let sources = vec![
            ("base.ash".to_string(), "space base\n\npart Chain {\n  steps append: [text] = [\"base\"]\n}\n".to_string()),
            ("alpha.ash".to_string(), "space alpha\nuse base\nuse zulu\n\npart base.Chain {\n  steps append: [text] = [\"alpha\"]\n}\n".to_string()),
            ("zulu.ash".to_string(), "space zulu\nuse base\n\npart base.Chain {\n  steps append: [text] = [\"zulu\"]\n}\n".to_string()),
        ];
        let r = crate::check_sources(sources);
        assert!(!r.has_errors(), "{:?}", r.diags);
        let changes = order_changes(&base, &r.program);
        assert_eq!(changes.len(), 1, "{:?}", changes);
        assert_eq!(changes[0].part, "base.Chain");
        assert_eq!(changes[0].after, vec!["base", "zulu", "alpha"]);

        // A layer that disappeared is the author's own diff, not a reorder.
        flipped
            .layers
            .insert("base.Chain".to_string(), vec!["base".to_string()]);
        assert!(order_changes(&flipped, &r.program).is_empty());
    }

    #[test]
    fn an_unchanged_program_reports_nothing() {
        let base = parse(MANIFEST).unwrap();
        let sources = vec![
            ("base.ash".to_string(), "space base\n\npart Chain {\n  steps append: [text] = [\"base\"]\n}\n".to_string()),
            ("alpha.ash".to_string(), "space alpha\nuse base\n\npart base.Chain {\n  steps append: [text] = [\"alpha\"]\n}\n".to_string()),
            ("zulu.ash".to_string(), "space zulu\nuse base\nuse alpha\n\npart base.Chain {\n  steps append: [text] = [\"zulu\"]\n}\n".to_string()),
        ];
        let r = crate::check_sources(sources);
        assert!(!r.has_errors(), "{:?}", r.diags);
        assert!(order_changes(&base, &r.program).is_empty());
        assert!(diagnostics(&base, &r.program).is_empty());
    }
}
