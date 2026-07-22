//! Fix applier. Implemented by the CLI agent — this scaffold only pins the
//! contracted signature so parallel work compiles. Replace this file whole.

use crate::diag::Diag;

/// Apply every machine-applicable fix in `diags` to the files under `root`.
/// Returns the files rewritten. Edits must be applied bottom-up per file so
/// earlier edits do not shift later spans; overlapping edits are skipped
/// with a warning on stderr.
pub fn apply_fixes(_root: &std::path::Path, _diags: &[Diag]) -> std::io::Result<Vec<String>> {
    todo!("cli agent: implement per reference §8/§11 (D2)")
}
