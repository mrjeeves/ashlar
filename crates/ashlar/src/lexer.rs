//! Lexer. Implemented by the lexer agent — this scaffold only pins the
//! contracted signature so parallel work compiles. Replace this file whole.

use crate::diag::Diag;
use crate::tokens::Token;

pub fn lex(_file: &str, _src: &str) -> (Vec<Token>, Vec<Diag>) {
    todo!("lexer agent: implement per reference §1/§5 and docs/diagnostics.md")
}
