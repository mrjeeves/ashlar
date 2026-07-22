//! Parser. Implemented by the parser agent — this scaffold only pins the
//! contracted signature so parallel work compiles. Replace this file whole.

use crate::ast::SrcFile;
use crate::diag::Diag;
use crate::tokens::Token;

/// Returns the parsed file (None only when the file is unrecoverable, e.g.
/// no space header) plus diagnostics. Recover at declaration boundaries so
/// one bad property does not hide the rest of the file.
pub fn parse(_file: &str, _tokens: &[Token]) -> (Option<SrcFile>, Vec<Diag>) {
    todo!("parser agent: implement per reference §1-§7 and docs/diagnostics.md")
}
