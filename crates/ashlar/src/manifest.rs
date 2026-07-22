//! Manifest writer (reference §10, requirements F2/F3).
//!
//! `ashlar build` writes the derived state of a program — the format
//! version, the space graph, every part's layers in composition order, the
//! use graph, foreign bindings, and asset locations — as one deterministic
//! JSON document. The manifest is state, never hand-edited; deleting it and
//! rebuilding reproduces it exactly (F2), and moving a source file changes
//! only the recorded locations (F3).
//!
//! There is no serde (zero external dependencies, G1): `J` below is a tiny
//! private JSON tree with a fixed 2-space pretty-printer, and every object's
//! key order is decided by the caller — populated straight from `Program`'s
//! and `ComposedPart`'s `BTreeMap`s, which already iterate in sorted order,
//! plus the fixed top-level field order the reference specifies.

use crate::ast;
use crate::diag::push_json_str;
use crate::resolved::{ComposedPart, ComposedProp, MergedValue, Program};
use std::collections::BTreeMap;

/// Render `ashlar.manifest`: a single JSON document, stable key order,
/// 2-space indented, trailing newline. See module docs for the shape.
pub fn render(program: &Program, composed: &BTreeMap<String, ComposedPart>) -> String {
    let mut root: Vec<(String, J)> = Vec::new();

    root.push(("format".to_string(), J::Int(1)));
    root.push((
        "order".to_string(),
        J::Arr(program.order.iter().map(|s| J::Str(s.clone())).collect()),
    ));
    root.push(("spaces".to_string(), render_spaces(program)));
    root.push(("parts".to_string(), render_parts(program)));
    root.push(("foreigns".to_string(), render_foreigns(program)));
    root.push(("assets".to_string(), render_assets(program, composed)));

    let mut out = String::new();
    J::Obj(root).write_pretty(0, &mut out);
    out.push('\n');
    out
}

fn render_spaces(program: &Program) -> J {
    let mut obj = Vec::new();
    for (name, info) in &program.spaces {
        let files = J::Arr(info.files.iter().map(|f| J::Str(f.clone())).collect());
        let uses = J::Arr(info.uses.iter().map(|u| J::Str(u.clone())).collect());
        obj.push((
            name.clone(),
            J::Obj(vec![("files".to_string(), files), ("uses".to_string(), uses)]),
        ));
    }
    J::Obj(obj)
}

fn render_parts(program: &Program) -> J {
    let mut obj = Vec::new();
    for (full_name, info) in &program.parts {
        let layers: Vec<J> = info
            .layers
            .iter()
            .map(|layer| {
                let decl = program.part_decl(layer);
                let file = program.file_path(layer).to_string();
                let line = decl.name_span.start.line;
                J::Obj(vec![
                    ("space".to_string(), J::Str(layer.space.clone())),
                    ("file".to_string(), J::Str(file)),
                    ("line".to_string(), J::Int(line as i64)),
                ])
            })
            .collect();
        obj.push((
            full_name.clone(),
            J::Obj(vec![
                ("home".to_string(), J::Str(info.home.clone())),
                ("layers".to_string(), J::Arr(layers)),
            ]),
        ));
    }
    J::Obj(obj)
}

fn render_foreigns(program: &Program) -> J {
    let mut obj = Vec::new();
    for (full_name, info) in &program.foreigns {
        let file = program.files[info.file_idx].path.clone();
        let binding = format!("foreign/{}", info.space);
        obj.push((
            full_name.clone(),
            J::Obj(vec![
                ("space".to_string(), J::Str(info.space.clone())),
                ("file".to_string(), J::Str(file)),
                ("binding".to_string(), J::Str(binding)),
            ]),
        ));
    }
    J::Obj(obj)
}

fn render_assets(program: &Program, composed: &BTreeMap<String, ComposedPart>) -> J {
    let mut obj = Vec::new();
    for (full_name, part) in composed {
        if let Some(prop) = part.props.get("files") {
            if let Some(value) = literal_text_value(program, prop) {
                obj.push((full_name.clone(), J::Str(format!("assets/{}", value))));
            }
        }
    }
    J::Obj(obj)
}

/// The text a `files` property's value literal names, if it resolves to one.
/// Per the manifest contract: a computed `append`/`deep` literal (`Literal`),
/// or a single defining layer (`Single`) resolved back through `program` to
/// the source expression it points at (`PropRef` indexes into `program.files`).
/// Anything else (a field with no value, or a stack/pipe/non-literal chain)
/// is not an asset location and is skipped.
fn literal_text_value(program: &Program, prop: &ComposedProp) -> Option<String> {
    match &prop.value {
        MergedValue::Literal(expr) => text_of(expr),
        MergedValue::Single(prop_ref) => {
            let decl = &program.files[prop_ref.file_idx].ast.parts[prop_ref.part_idx];
            decl.props[prop_ref.prop_idx].value.as_ref().and_then(text_of)
        }
        MergedValue::FieldOnly | MergedValue::Chain(_) => None,
    }
}

fn text_of(e: &ast::SExpr) -> Option<String> {
    match &e.expr {
        ast::Expr::Text(s) => Some(s.clone()),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// A tiny private JSON tree with a fixed 2-space pretty printer. No external
// crate: this is the entire "serialization layer" the manifest needs.
// ---------------------------------------------------------------------------

enum J {
    Int(i64),
    Str(String),
    Arr(Vec<J>),
    /// Insertion-ordered key/value pairs. Callers are responsible for
    /// feeding entries in the order the manifest wants them rendered
    /// (BTreeMap iteration for derived maps, fixed order for the top level).
    Obj(Vec<(String, J)>),
}

impl J {
    fn write_pretty(&self, level: usize, out: &mut String) {
        match self {
            J::Int(n) => out.push_str(&n.to_string()),
            J::Str(s) => push_json_str(out, s),
            J::Arr(items) => write_seq(out, level, items.len(), '[', ']', |out, i| {
                items[i].write_pretty(level + 1, out)
            }),
            J::Obj(entries) => write_seq(out, level, entries.len(), '{', '}', |out, i| {
                let (k, v) = &entries[i];
                push_json_str(out, k);
                out.push_str(": ");
                v.write_pretty(level + 1, out);
            }),
        }
    }
}

/// Shared body for `[...]` and `{...}`: empty renders inline, non-empty
/// renders one entry per line at `level + 1`, closing delimiter at `level`.
fn write_seq(
    out: &mut String,
    level: usize,
    len: usize,
    open: char,
    close: char,
    mut write_item: impl FnMut(&mut String, usize),
) {
    if len == 0 {
        out.push(open);
        out.push(close);
        return;
    }
    out.push(open);
    out.push('\n');
    for i in 0..len {
        push_indent(out, level + 1);
        write_item(out, i);
        if i + 1 < len {
            out.push(',');
        }
        out.push('\n');
    }
    push_indent(out, level);
    out.push(close);
}

fn push_indent(out: &mut String, level: usize) {
    for _ in 0..level {
        out.push_str("  ");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{Expr, PartDecl, Prop, SExpr, SrcFile};
    use crate::resolved::{FileEntry, Layer, PartInfo, PropRef, SpaceInfo};
    use crate::tokens::Span;
    use std::collections::BTreeSet;

    /// One space (`site`), one file, one part (`site.static`) with a
    /// `route` and a `files` property — enough to exercise every top-level
    /// manifest field except `foreigns` (left empty; foreign binding
    /// derivation has no branching worth a hand-built fixture beyond the
    /// straight-line code in `render_foreigns`).
    fn build_program(file_path: &str) -> (Program, BTreeMap<String, ComposedPart>) {
        let part = PartDecl {
            name: vec!["static".to_string()],
            name_span: Span::point(3, 6),
            span: Span::point(3, 6),
            props: vec![
                Prop {
                    name: "route".to_string(),
                    name_span: Span::point(4, 3),
                    storage: None,
                    kind: None,
                    shape: None,
                    value: Some(SExpr {
                        expr: Expr::Text("/static".to_string()),
                        span: Span::point(4, 11),
                    }),
                },
                Prop {
                    name: "files".to_string(),
                    name_span: Span::point(5, 3),
                    storage: None,
                    kind: None,
                    shape: None,
                    value: Some(SExpr {
                        expr: Expr::Text("public".to_string()),
                        span: Span::point(5, 11),
                    }),
                },
            ],
        };
        let file = FileEntry {
            path: file_path.to_string(),
            ast: SrcFile {
                space: vec!["site".to_string()],
                space_span: Span::point(1, 1),
                uses: vec![],
                parts: vec![part],
                foreigns: vec![],
            },
        };

        let mut spaces = BTreeMap::new();
        spaces.insert(
            "site".to_string(),
            SpaceInfo {
                files: vec![file_path.to_string()],
                uses: BTreeSet::new(),
                closure: BTreeSet::new(),
            },
        );

        let mut parts = BTreeMap::new();
        parts.insert(
            "site.static".to_string(),
            PartInfo {
                home: "site".to_string(),
                layers: vec![Layer {
                    space: "site".to_string(),
                    file_idx: 0,
                    part_idx: 0,
                }],
            },
        );

        let program = Program {
            files: vec![file],
            spaces,
            parts,
            foreigns: BTreeMap::new(),
            order: vec!["site".to_string()],
        };

        let mut props = BTreeMap::new();
        props.insert(
            "route".to_string(),
            ComposedProp {
                name: "route".to_string(),
                storage: None,
                kind: None,
                shape: None,
                defs: vec![PropRef {
                    space: "site".to_string(),
                    file_idx: 0,
                    part_idx: 0,
                    prop_idx: 0,
                }],
                value: MergedValue::Single(PropRef {
                    space: "site".to_string(),
                    file_idx: 0,
                    part_idx: 0,
                    prop_idx: 0,
                }),
            },
        );
        props.insert(
            "files".to_string(),
            ComposedProp {
                name: "files".to_string(),
                storage: None,
                kind: None,
                shape: None,
                defs: vec![PropRef {
                    space: "site".to_string(),
                    file_idx: 0,
                    part_idx: 0,
                    prop_idx: 1,
                }],
                value: MergedValue::Single(PropRef {
                    space: "site".to_string(),
                    file_idx: 0,
                    part_idx: 0,
                    prop_idx: 1,
                }),
            },
        );

        let mut composed = BTreeMap::new();
        composed.insert("site.static".to_string(), ComposedPart { props });

        (program, composed)
    }

    const GOLDEN: &str = r#"{
  "format": 1,
  "order": [
    "site"
  ],
  "spaces": {
    "site": {
      "files": [
        "site.ash"
      ],
      "uses": []
    }
  },
  "parts": {
    "site.static": {
      "home": "site",
      "layers": [
        {
          "space": "site",
          "file": "site.ash",
          "line": 3
        }
      ]
    }
  },
  "foreigns": {},
  "assets": {
    "site.static": "assets/public"
  }
}
"#;

    #[test]
    fn golden_string_on_small_hand_built_program() {
        let (program, composed) = build_program("site.ash");
        assert_eq!(render(&program, &composed), GOLDEN);
    }

    #[test]
    fn deterministic_same_input_same_bytes() {
        // F2: same program + composed renders byte-identically. Build the
        // fixture twice independently (not just re-render the same value)
        // so the test actually exercises determinism, not mere idempotence.
        let (p1, c1) = build_program("site.ash");
        let (p2, c2) = build_program("site.ash");
        assert_eq!(render(&p1, &c1), render(&p2, &c2));
    }

    #[test]
    fn relocation_changes_only_recorded_locations() {
        // F3: two Programs identical except for a source file's recorded
        // path must render identically once that path is substituted back.
        let (p_here, c_here) = build_program("site.ash");
        let (p_moved, c_moved) = build_program("moved/deep/site.ash");

        let rendered_here = render(&p_here, &c_here);
        let rendered_moved = render(&p_moved, &c_moved);

        assert_ne!(rendered_here, rendered_moved, "the fixture should actually move");
        assert_eq!(rendered_here.replace("site.ash", "moved/deep/site.ash"), rendered_moved);
    }

    #[test]
    fn assets_skip_non_literal_files_value() {
        // A `files` property whose composed value isn't a resolvable text
        // literal (here: a FieldOnly stand-in, as if declared with no
        // value) must not appear in `assets` — no panics, no bogus entries.
        let (program, mut composed) = build_program("site.ash");
        {
            let part = composed.get_mut("site.static").unwrap();
            let prop = part.props.get_mut("files").unwrap();
            prop.value = MergedValue::FieldOnly;
        }
        let rendered = render(&program, &composed);
        assert!(!rendered.contains("\"assets\": {\n"));
        assert!(rendered.ends_with("\"assets\": {}\n}\n"));
    }
}
