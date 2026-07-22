//! Composer. Implemented by the composer agent — this scaffold only pins the
//! contracted signature so parallel work compiles. Replace this file whole.

use crate::diag::Diag;
use crate::resolved::{ComposedPart, Program};
use std::collections::BTreeMap;

/// Flatten every part's layers per reference §4: enforce kind identity
/// (E004/E005/E013/E019), compute literal merges for append/deep, and build
/// ordered chains for everything else.
pub fn compose(_program: &Program) -> (BTreeMap<String, ComposedPart>, Vec<Diag>) {
    todo!("composer agent: implement per reference §4 and docs/diagnostics.md")
}
