//! CONTRACT FILE — owned by the integrator. Module implementors: do not edit.
//! Diagnostics: machine-readable first (JSONL), human-rendered second (D4).
//! The stable id catalog lives in docs/diagnostics.md; the constants here
//! are the single source of truth in code. Every diagnostic must use one.

use crate::tokens::{Pos, Span};

/// (stable id, requirement enforced)
pub type Code = (&'static str, &'static str);

pub const E001_UNKNOWN_NAME: Code = ("E001", "B3");
pub const E002_AMBIGUOUS_NAME: Code = ("E002", "B3");
pub const E003_CASE_COLLISION: Code = ("E003", "B4");
pub const E004_KIND_CHANGED: Code = ("E004", "C5");
pub const E005_KIND_OMITTED: Code = ("E005", "C5");
pub const E006_SHAPE: Code = ("E006", "A4");
pub const E007_PARSE: Code = ("E007", "A4");
pub const E008_USE_NOT_SPACE: Code = ("E008", "B7");
pub const E009_INTERPOLATION: Code = ("E009", "A4");
pub const E010_SEMICOLON: Code = ("E010", "A4");
pub const E011_HASH_COMMENT: Code = ("E011", "A4");
pub const E012_NEWLINE_IN_TEXT: Code = ("E012", "A4");
pub const E013_DUP_PROP: Code = ("E013", "C5");
pub const E014_DUP_LAYER: Code = ("E014", "C2");
pub const E015_USE_CYCLE: Code = ("E015", "C2");
pub const E016_RESERVED_WORD: Code = ("E016", "A4");
pub const E017_STD_LAYER: Code = ("E017", "B3");
pub const E018_FOREIGN_TOPLEVEL: Code = ("E018", "A4");
pub const E019_STACK_PIPE_ARITY: Code = ("E019", "C4");
pub const E020_BAD_REVERSE: Code = ("E020", "C4");
// E021 reserved: route conflict (needs evaluated route values).
pub const E022_SPACE_HEADER: Code = ("E022", "B6");
pub const E023_FOREIGN_STMT: Code = ("E023", "A4");
pub const E024_FNLIT_POSITION: Code = ("E024", "E2");
pub const E025_BAD_ASSIGN: Code = ("E025", "A4");
pub const E026_EVERY_NO_RUN: Code = ("E026", "G4");
pub const E027_STORAGE_CHANGED: Code = ("E027", "C5");
pub const E028_UNMERGEABLE: Code = ("E028", "C4");
pub const W001_UNORDERED_LAYERS: Code = ("W001", "C3");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Error,
    Warn,
}

/// One text replacement. `start`/`end` follow `Span` semantics
/// (1-based, end-exclusive). Insertion: start == end. Deletion: text == "".
#[derive(Debug, Clone, PartialEq)]
pub struct Edit {
    pub file: String,
    pub start: Pos,
    pub end: Pos,
    pub text: String,
}

/// A machine-applicable correction. Requirement D2: applying `edits` resolves
/// the diagnostic it is attached to and introduces no new error. Only attach
/// `edits` when that is actually true; otherwise set `edits: vec![]` and let
/// `note` carry the instruction (which D1 still requires to be specific
/// enough to apply without judgment).
#[derive(Debug, Clone, PartialEq)]
pub struct Fix {
    pub note: String,
    pub edits: Vec<Edit>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Diag {
    pub id: &'static str,
    pub req: &'static str,
    pub level: Level,
    pub file: String,
    pub span: Span,
    /// One sentence (D1).
    pub cause: String,
    pub fix: Option<Fix>,
}

impl Diag {
    pub fn new(code: Code, level: Level, file: &str, span: Span, cause: String) -> Diag {
        Diag {
            id: code.0,
            req: code.1,
            level,
            file: file.to_string(),
            span,
            cause,
            fix: None,
        }
    }

    pub fn with_fix(mut self, note: String, edits: Vec<Edit>) -> Diag {
        self.fix = Some(Fix { note, edits });
        self
    }

    pub fn is_error(&self) -> bool {
        self.level == Level::Error
    }

    /// One JSON object, no trailing newline. Key order is fixed:
    /// id, req, level, loc, cause, fix.
    pub fn jsonl(&self) -> String {
        let mut s = String::new();
        s.push_str("{\"id\":");
        push_json_str(&mut s, self.id);
        s.push_str(",\"req\":");
        push_json_str(&mut s, self.req);
        s.push_str(",\"level\":");
        push_json_str(&mut s, if self.level == Level::Error { "error" } else { "warn" });
        s.push_str(",\"loc\":{\"file\":");
        push_json_str(&mut s, &self.file);
        s.push_str(&format!(
            ",\"line\":{},\"col\":{},\"end_line\":{},\"end_col\":{}}}",
            self.span.start.line, self.span.start.col, self.span.end.line, self.span.end.col
        ));
        s.push_str(",\"cause\":");
        push_json_str(&mut s, &self.cause);
        if let Some(fix) = &self.fix {
            s.push_str(",\"fix\":{\"note\":");
            push_json_str(&mut s, &fix.note);
            s.push_str(",\"edits\":[");
            for (i, e) in fix.edits.iter().enumerate() {
                if i > 0 {
                    s.push(',');
                }
                s.push_str("{\"file\":");
                push_json_str(&mut s, &e.file);
                s.push_str(&format!(
                    ",\"line\":{},\"col\":{},\"end_line\":{},\"end_col\":{},\"text\":",
                    e.start.line, e.start.col, e.end.line, e.end.col
                ));
                push_json_str(&mut s, &e.text);
                s.push('}');
            }
            s.push_str("]}");
        }
        s.push('}');
        s
    }

    /// Prose rendering for `--human`.
    pub fn human(&self) -> String {
        let level = if self.level == Level::Error { "error" } else { "warning" };
        let mut s = format!(
            "{}[{}] {}:{}:{} {}",
            level, self.id, self.file, self.span.start.line, self.span.start.col, self.cause
        );
        if let Some(fix) = &self.fix {
            s.push_str(&format!("\n  fix: {}", fix.note));
        }
        s
    }
}

pub fn push_json_str(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}
