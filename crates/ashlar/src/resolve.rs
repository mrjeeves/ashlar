//! Resolver. Implemented by the resolver agent — this scaffold only pins the
//! contracted signature so parallel work compiles. Replace this file whole.

use crate::diag::Diag;
use crate::resolved::{FileEntry, Program};

/// Build the space graph, use closures, composition order, and part/foreign
/// tables; resolve every name reference in every expression body.
pub fn resolve(_files: Vec<FileEntry>) -> (Program, Vec<Diag>) {
    todo!("resolver agent: implement per reference §2-§3 and docs/diagnostics.md")
}
