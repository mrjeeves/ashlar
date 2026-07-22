//! Manifest writer. Implemented by the CLI agent — this scaffold only pins
//! the contracted signature so parallel work compiles. Replace this file whole.

use crate::resolved::{ComposedPart, Program};
use std::collections::BTreeMap;

/// Render `ashlar.manifest` (JSON) from the resolved program: format
/// version, spaces (with files), parts (with layers in composition order:
/// space, file, line), the use graph, foreign bindings (space `s` binds to
/// `foreign/s`), and asset locations (parts with a literal `files` property
/// map to `assets/<value>`). Deterministic: same program -> byte-identical
/// output (F2), and file moves change only recorded locations (F3).
pub fn render(_program: &Program, _composed: &BTreeMap<String, ComposedPart>) -> String {
    todo!("cli agent: implement per reference §10")
}
