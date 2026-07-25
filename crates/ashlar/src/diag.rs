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
pub const E021_ROUTE_CONFLICT: Code = ("E021", "A4");
pub const E022_SPACE_HEADER: Code = ("E022", "B6");
pub const E023_FOREIGN_STMT: Code = ("E023", "A4");
pub const E024_FNLIT_POSITION: Code = ("E024", "E2");
pub const E025_BAD_ASSIGN: Code = ("E025", "A4");
pub const E026_EVERY_NO_RUN: Code = ("E026", "G4");
pub const E027_STORAGE_CHANGED: Code = ("E027", "C5");
pub const E028_UNMERGEABLE: Code = ("E028", "C4");
pub const E029_PERUSER_NEEDS_STORAGE: Code = ("E029", "A4");
pub const E030_SETTING_RULES: Code = ("E030", "A4");
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

#[cfg(test)]
mod tests {
    use super::*;

    fn at(line: u32, col: u32, end_col: u32) -> Span {
        Span {
            start: Pos { line, col },
            end: Pos { line, col: end_col },
        }
    }

    #[test]
    fn jsonl_matches_the_shape_the_reference_documents() {
        // D4: machine-readable FIRST. Reference §8 prints an exact envelope;
        // this asserts the binary emits that envelope, key order included, so
        // the documented example cannot drift from the implementation.
        let d = Diag::new(
            E002_AMBIGUOUS_NAME,
            Level::Error,
            "chat/ui.ash",
            at(4, 10, 17),
            "`Message` resolves to chat.data.Message and note.Message.".to_string(),
        )
        .with_fix(
            "Qualify the reference.".to_string(),
            vec![Edit {
                file: "chat/ui.ash".to_string(),
                start: Pos { line: 4, col: 10 },
                end: Pos { line: 4, col: 17 },
                text: "chat.data.Message".to_string(),
            }],
        );
        assert_eq!(
            d.jsonl(),
            concat!(
                r#"{"id":"E002","req":"B3","level":"error","#,
                r#""loc":{"file":"chat/ui.ash","line":4,"col":10,"end_line":4,"end_col":17},"#,
                r#""cause":"`Message` resolves to chat.data.Message and note.Message.","#,
                r#""fix":{"note":"Qualify the reference.","edits":[{"file":"chat/ui.ash","#,
                r#""line":4,"col":10,"end_line":4,"end_col":17,"text":"chat.data.Message"}]}}"#,
            )
        );
        // One line, always: JSONL is only parseable if nothing embeds a newline.
        assert_eq!(d.jsonl().lines().count(), 1);
    }

    #[test]
    fn a_diagnostic_without_a_fix_omits_the_fix_key() {
        let d = Diag::new(
            E003_CASE_COLLISION,
            Level::Error,
            "a.ash",
            at(1, 1, 2),
            "Two names collide.".to_string(),
        );
        let line = d.jsonl();
        assert!(!line.contains("\"fix\""), "{}", line);
        assert!(line.ends_with("\"cause\":\"Two names collide.\"}"), "{}", line);
    }

    #[test]
    fn warnings_serialize_as_warn_and_never_claim_error() {
        let d = Diag::new(
            W001_UNORDERED_LAYERS,
            Level::Warn,
            "a.ash",
            at(2, 1, 4),
            "Two spaces layer one part.".to_string(),
        );
        assert!(d.jsonl().contains("\"level\":\"warn\""));
        assert!(!d.is_error());
        assert!(d.human().starts_with("warning[W001] a.ash:2:1 "));
    }

    #[test]
    fn causes_escape_everything_that_would_break_a_parser() {
        // A cause quoting source text can carry any byte. If escaping were
        // wrong, one diagnostic would corrupt the whole JSONL stream.
        let nasty = format!(
            "expected {}text{}, found{}a{}num{}{}",
            '"', '"', '\t', '\n', '\r', '\u{1}'
        );
        let d = Diag::new(
            E006_SHAPE,
            Level::Error,
            "q\\dir/a.ash",
            at(1, 1, 2),
            nasty,
        );
        let line = d.jsonl();
        assert!(line.contains("\"file\":\"q\\\\dir/a.ash\""), "{}", line);
        assert!(line.contains("expected \\\"text\\\", found\\ta\\nnum\\r"), "{}", line);
        assert!(line.contains("\\u0001"), "control chars need \\u escapes: {}", line);
        // Still exactly one physical line — the escaping is what guarantees it.
        assert_eq!(line.lines().count(), 1);
    }

    #[test]
    fn every_catalog_code_carries_a_requirement_and_a_unique_id() {
        // The catalog here is the single source of truth in code
        // (docs/diagnostics.md is its prose twin). A code with an empty `req`
        // would publish a diagnostic that enforces nothing, and a duplicate id
        // would silently retire one of them.
        let all: &[Code] = &[
            E001_UNKNOWN_NAME,
            E002_AMBIGUOUS_NAME,
            E003_CASE_COLLISION,
            E004_KIND_CHANGED,
            E005_KIND_OMITTED,
            E006_SHAPE,
            E007_PARSE,
            E008_USE_NOT_SPACE,
            E009_INTERPOLATION,
            E010_SEMICOLON,
            E011_HASH_COMMENT,
            E012_NEWLINE_IN_TEXT,
            E013_DUP_PROP,
            E014_DUP_LAYER,
            E015_USE_CYCLE,
            E016_RESERVED_WORD,
            E017_STD_LAYER,
            E018_FOREIGN_TOPLEVEL,
            E019_STACK_PIPE_ARITY,
            E020_BAD_REVERSE,
            E021_ROUTE_CONFLICT,
            E022_SPACE_HEADER,
            E023_FOREIGN_STMT,
            E024_FNLIT_POSITION,
            E025_BAD_ASSIGN,
            E026_EVERY_NO_RUN,
            E027_STORAGE_CHANGED,
            E028_UNMERGEABLE,
            E029_PERUSER_NEEDS_STORAGE,
            E030_SETTING_RULES,
            W001_UNORDERED_LAYERS,
        ];
        for (id, req) in all {
            assert!(!id.is_empty() && !req.is_empty(), "{} has no requirement", id);
            assert!(
                id.starts_with('E') || id.starts_with('W'),
                "unexpected id prefix: {}",
                id
            );
        }
        let mut ids: Vec<&str> = all.iter().map(|(id, _)| *id).collect();
        ids.sort();
        let before = ids.len();
        ids.dedup();
        assert_eq!(before, ids.len(), "duplicate diagnostic id in the catalog");
    }
}
