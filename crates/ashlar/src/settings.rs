//! Deployment settings (reference §9.12, ADR-0020).
//!
//! A `setting` is a property whose NAME and SHAPE are source and whose VALUE is
//! a deployment fact. That split is what lets a program depend on something it
//! cannot know when it is written — an address, a key, a limit — without
//! writing a location into source (B5). Names still bind (B1), the toolchain
//! still sees every setting, and a missing one stops startup with the name in
//! hand rather than surfacing as a broken request later.
//!
//! Values live in `settings.json` at the project root, or wherever
//! `ASHLAR_SETTINGS` points — the same shape of relationship `foreign.json`
//! has to `ASHLAR_FOREIGN`, and `--port` has to `port`. Keys are full property
//! names, so there is nothing to resolve and nothing to guess:
//!
//! ```json
//! { "site.app.endpoint": "http://127.0.0.1:9000", "site.app.retries": 5 }
//! ```

use crate::ast::Shape;
use crate::eval::{from_json, V};
use crate::resolved::ComposedPart;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// One setting a program declares: where it is, what shape it must be, and
/// whether source supplied a default.
#[derive(Debug, Clone, PartialEq)]
pub struct Declared {
    /// Full property name — `space.Part.prop`.
    pub key: String,
    pub shape: Option<Shape>,
    pub has_default: bool,
}

/// Where the settings file lives: `ASHLAR_SETTINGS` if set, else
/// `<root>/settings.json`. Absent is fine when every setting has a default.
pub fn settings_path(root: &Path) -> PathBuf {
    match std::env::var("ASHLAR_SETTINGS") {
        Ok(p) if !p.is_empty() => PathBuf::from(p),
        _ => root.join("settings.json"),
    }
}

/// Every setting the composed program declares, in name order.
pub fn declared(composed: &BTreeMap<String, ComposedPart>) -> Vec<Declared> {
    let mut out = Vec::new();
    for (full, part) in composed {
        for (name, prop) in &part.props {
            if !prop.setting {
                continue;
            }
            debug_assert!(prop.storage.is_none(), "a setting is never a storage class");
            out.push(Declared {
                key: format!("{}.{}", full, name),
                shape: prop.shape.as_ref().map(|s| s.shape.clone()),
                has_default: !matches!(prop.value, crate::resolved::MergedValue::FieldOnly),
            });
        }
    }
    out
}

/// Parse the settings file: a flat JSON object of full property name -> value.
/// Malformed is loud, never a silent fall back to defaults — a deployment fact
/// that cannot be read is not the same as one that was not supplied.
pub fn parse(text: &str) -> Result<BTreeMap<String, V>, String> {
    match from_json(text) {
        Some(V::Map(m)) => Ok(m),
        Some(_) => Err("the settings file must be a JSON object of `space.Part.prop` -> value."
            .to_string()),
        None => Err("the settings file is not valid JSON.".to_string()),
    }
}

/// Read the settings file if it exists. A missing file is `Ok(empty)`: it means
/// nothing was supplied, which is only an error if something was required.
pub fn load(root: &Path) -> Result<BTreeMap<String, V>, String> {
    let path = settings_path(root);
    match std::fs::read_to_string(&path) {
        Ok(text) => parse(&text).map_err(|e| format!("{}: {}", path.display(), e)),
        Err(_) => Ok(BTreeMap::new()),
    }
}

/// Does this value satisfy the declared shape? Deliberately permissive where
/// the shape itself is permissive (`data` accepts anything), and silent about
/// shapes the checker resolves elsewhere — a wrong error here would be worse
/// than none (no false positives).
pub fn fits(shape: &Shape, v: &V) -> bool {
    match (shape, v) {
        (Shape::Data, _) => true,
        (Shape::Opt(_), V::None) => true,
        (Shape::Opt(inner), other) => fits(&inner.shape, other),
        (Shape::Text, V::Text(_)) => true,
        (Shape::Number, V::Number(_)) => true,
        (Shape::Bool, V::Bool(_)) => true,
        (Shape::List(inner), V::List(items)) => items.iter().all(|i| fits(&inner.shape, i)),
        (Shape::Map(inner), V::Map(m)) => m.values().all(|i| fits(&inner.shape, i)),
        // A part shape (a data shape) is checked structurally by the checker at
        // its use sites; accepting a map here keeps this from inventing errors.
        (Shape::Part(_), V::Map(_)) => true,
        // A function shape can never arrive as JSON, and anything else is a
        // genuine mismatch.
        _ => false,
    }
}

/// A human name for a shape, for the one message a missing setting produces.
pub fn shape_name(shape: &Shape) -> String {
    match shape {
        Shape::Text => "text".to_string(),
        Shape::Number => "number".to_string(),
        Shape::Bool => "bool".to_string(),
        Shape::Data => "data".to_string(),
        Shape::List(inner) => format!("[{}]", shape_name(&inner.shape)),
        Shape::Map(inner) => format!("{{text: {}}}", shape_name(&inner.shape)),
        Shape::Opt(inner) => format!("{}?", shape_name(&inner.shape)),
        Shape::Part(n) => crate::ast::name_to_string(n),
        Shape::Fn(..) => "function".to_string(),
    }
}

/// What a program is missing before it can start. Every problem at once, not
/// the first one: an operator filling in settings wants the whole list.
pub struct Missing {
    pub required: Vec<Declared>,
    pub ill_shaped: Vec<(String, String)>,
}

impl Missing {
    pub fn is_empty(&self) -> bool {
        self.required.is_empty() && self.ill_shaped.is_empty()
    }

    /// One message naming every gap and where to fix it.
    pub fn report(&self, root: &Path) -> String {
        let path = settings_path(root);
        let mut s = String::new();
        if !self.required.is_empty() {
            s.push_str(&format!(
                "{} setting(s) have no value, and no default:\n",
                self.required.len()
            ));
            for d in &self.required {
                let shape = d
                    .shape
                    .as_ref()
                    .map(shape_name)
                    .unwrap_or_else(|| "?".to_string());
                s.push_str(&format!("  {} : {}\n", d.key, shape));
            }
        }
        for (key, why) in &self.ill_shaped {
            s.push_str(&format!("  {} — {}\n", key, why));
        }
        s.push_str(&format!(
            "Supply them in `{}` as a JSON object keyed by full property name.",
            path.display()
        ));
        s
    }
}

/// Resolve every declared setting against the supplied values, returning the
/// bindings to seed and whatever is still missing.
pub fn resolve(
    declared: &[Declared],
    supplied: &BTreeMap<String, V>,
) -> (BTreeMap<String, V>, Missing) {
    let mut bound = BTreeMap::new();
    let mut missing = Missing {
        required: Vec::new(),
        ill_shaped: Vec::new(),
    };
    for d in declared {
        match supplied.get(&d.key) {
            Some(v) => {
                if let Some(shape) = &d.shape {
                    if !fits(shape, v) {
                        missing.ill_shaped.push((
                            d.key.clone(),
                            format!("expected {}", shape_name(shape)),
                        ));
                        continue;
                    }
                }
                bound.insert(d.key.clone(), v.clone());
            }
            None if d.has_default => {}
            None => missing.required.push(d.clone()),
        }
    }
    (bound, missing)
}

/// Diagnose the settings file against what the program declares: a key naming
/// no declared setting is a name resolving to nothing (B3, `E001`), and a value
/// of the wrong shape is `E006`. Deliberately silent when the file is absent —
/// that is deployment's business, and the runtime refuses to start without a
/// required value anyway.
pub fn check_file(root: &Path, declared: &[Declared]) -> Vec<crate::diag::Diag> {
    use crate::diag::{Diag, Level, E001_UNKNOWN_NAME, E006_SHAPE};
    use crate::tokens::{Pos, Span};
    let path = settings_path(root);
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };
    let file = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "settings.json".to_string());
    let at = Span { start: Pos { line: 1, col: 1 }, end: Pos { line: 1, col: 1 } };
    let supplied = match parse(&text) {
        Ok(m) => m,
        Err(e) => {
            return vec![Diag::new(
                E001_UNKNOWN_NAME,
                Level::Error,
                &file,
                at,
                format!("the settings file is unreadable: {}", e),
            )
            .with_fix(
                "Correct the file, or delete it if no setting needs a value yet.".to_string(),
                vec![],
            )]
        }
    };
    let mut out = Vec::new();
    for (key, value) in &supplied {
        match declared.iter().find(|d| &d.key == key) {
            None => {
                let near = declared
                    .iter()
                    .map(|d| d.key.as_str())
                    .find(|k| k.rsplit('.').next() == key.rsplit('.').next());
                let note = match near {
                    Some(n) => format!("Use the full property name `{}`.", n),
                    None => "Delete the entry, or declare that setting.".to_string(),
                };
                out.push(
                    Diag::new(
                        E001_UNKNOWN_NAME,
                        Level::Error,
                        &file,
                        at,
                        format!("`{}` names no declared setting in this program.", key),
                    )
                    .with_fix(note, vec![]),
                );
            }
            Some(d) => {
                if let Some(shape) = &d.shape {
                    if !fits(shape, value) {
                        out.push(
                            Diag::new(
                                E006_SHAPE,
                                Level::Error,
                                &file,
                                at,
                                format!(
                                    "setting `{}` expects {}, and the supplied value is not one.",
                                    key,
                                    shape_name(shape)
                                ),
                            )
                            .with_fix(
                                format!("Supply a {} for `{}`.", shape_name(shape), key),
                                vec![],
                            ),
                        );
                    }
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::SShape;
    use crate::tokens::{Pos, Span};

    fn at() -> Span {
        Span { start: Pos { line: 1, col: 1 }, end: Pos { line: 1, col: 1 } }
    }
    fn wrap(shape: Shape) -> Box<SShape> {
        Box::new(SShape { shape, span: at() })
    }
    fn opt_text() -> Shape {
        Shape::Opt(wrap(Shape::Text))
    }
    fn list_number() -> Shape {
        Shape::List(wrap(Shape::Number))
    }

    #[test]
    fn a_flat_object_parses_and_anything_else_is_loud() {
        let m = parse("{\"a.B.c\": \"x\", \"a.B.n\": 2}").expect("object");
        assert_eq!(m.get("a.B.c"), Some(&V::Text("x".to_string())));
        assert!(parse("[1,2]").is_err());
        assert!(parse("not json").is_err());
    }

    #[test]
    fn shapes_are_checked_but_permissive_where_the_shape_is() {
        assert!(fits(&Shape::Text, &V::Text("x".into())));
        assert!(!fits(&Shape::Text, &V::Number(1.0)));
        assert!(fits(&Shape::Data, &V::List(vec![V::Bool(true)])));
        assert!(fits(&opt_text(), &V::None));
        assert!(fits(
            &list_number(),
            &V::List(vec![V::Number(1.0)])
        ));
        assert!(!fits(
            &list_number(),
            &V::List(vec![V::Text("no".into())])
        ));
        // `data` absorbs anything, which is what makes it the payload shape.
        assert!(fits(&Shape::Data, &V::Bool(false)));
    }

    #[test]
    fn resolve_reports_every_gap_at_once_not_the_first() {
        let declared = vec![
            Declared { key: "a.B.one".into(), shape: Some(Shape::Text), has_default: false },
            Declared { key: "a.B.two".into(), shape: Some(Shape::Number), has_default: false },
            Declared { key: "a.B.three".into(), shape: Some(Shape::Text), has_default: true },
            Declared { key: "a.B.four".into(), shape: Some(Shape::Number), has_default: false },
        ];
        let mut supplied = BTreeMap::new();
        supplied.insert("a.B.four".to_string(), V::Text("wrong shape".into()));
        let (bound, missing) = resolve(&declared, &supplied);
        assert!(bound.is_empty());
        // Both unsupplied-and-required, not just the first.
        let keys: Vec<&str> = missing.required.iter().map(|d| d.key.as_str()).collect();
        assert_eq!(keys, vec!["a.B.one", "a.B.two"]);
        // The defaulted one is not missing; the ill-shaped one is separate.
        assert_eq!(missing.ill_shaped.len(), 1);
        assert_eq!(missing.ill_shaped[0].0, "a.B.four");
        assert!(!missing.is_empty());
        let report = missing.report(Path::new("/proj"));
        assert!(report.contains("a.B.one : text"), "{}", report);
        assert!(report.contains("a.B.two : number"), "{}", report);
        assert!(!report.contains("a.B.three"), "defaulted settings are not missing: {}", report);
        assert!(report.contains("settings.json"), "{}", report);
    }

    #[test]
    fn a_supplied_value_binds_and_a_default_stays_unbound() {
        let declared = vec![
            Declared { key: "a.B.one".into(), shape: Some(Shape::Text), has_default: false },
            Declared { key: "a.B.two".into(), shape: Some(Shape::Number), has_default: true },
        ];
        let mut supplied = BTreeMap::new();
        supplied.insert("a.B.one".to_string(), V::Text("here".into()));
        let (bound, missing) = resolve(&declared, &supplied);
        assert!(missing.is_empty());
        assert_eq!(bound.get("a.B.one"), Some(&V::Text("here".into())));
        // Unbound means "fall through to the source default", not "absent".
        assert!(!bound.contains_key("a.B.two"));
    }
}
