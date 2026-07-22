//! CONTRACT FILE — owned by the integrator. Module implementors: do not edit.
//! Resolved program model: output of the resolver, input to the composer
//! and manifest writer. All maps are BTreeMaps so iteration order — and
//! therefore every downstream artifact — is deterministic (C2, F2).

use crate::ast;
use std::collections::{BTreeMap, BTreeSet};

/// One source file: path (relative to the project root, `/`-separated)
/// plus its parsed AST.
#[derive(Debug, Clone)]
pub struct FileEntry {
    pub path: String,
    pub ast: ast::SrcFile,
}

#[derive(Debug, Clone, Default)]
pub struct SpaceInfo {
    /// Files declaring into this space, sorted by path.
    pub files: Vec<String>,
    /// Direct `use` targets.
    pub uses: BTreeSet<String>,
    /// Transitive closure of `uses` (not including the space itself,
    /// always including "std").
    pub closure: BTreeSet<String>,
}

/// A layer of a part: which file/declaration contributes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layer {
    pub space: String,
    /// Index into `Program::files`.
    pub file_idx: usize,
    /// Index into that file's `ast.parts`.
    pub part_idx: usize,
}

#[derive(Debug, Clone)]
pub struct PartInfo {
    /// The space whose bare declaration introduced the part.
    pub home: String,
    /// Layers in composition order: base first (C2). The resolver computes
    /// this from the use graph, breaking genuine ties lexicographically by
    /// space name and emitting W001.
    pub layers: Vec<Layer>,
}

#[derive(Debug, Clone)]
pub struct ForeignInfo {
    pub space: String,
    pub file_idx: usize,
    pub foreign_idx: usize,
}

#[derive(Debug, Clone, Default)]
pub struct Program {
    pub files: Vec<FileEntry>,
    /// Space name -> info. Never contains "std".
    pub spaces: BTreeMap<String, SpaceInfo>,
    /// Full part name -> info.
    pub parts: BTreeMap<String, PartInfo>,
    /// Full foreign name (space.name) -> info.
    pub foreigns: BTreeMap<String, ForeignInfo>,
    /// All space names in composition order, base first.
    pub order: Vec<String>,
}

impl Program {
    pub fn part_decl<'a>(&'a self, layer: &Layer) -> &'a ast::PartDecl {
        &self.files[layer.file_idx].ast.parts[layer.part_idx]
    }
    pub fn file_path<'a>(&'a self, layer: &Layer) -> &'a str {
        &self.files[layer.file_idx].path
    }
}

// ---------------------------------------------------------------------------
// Composed model: output of the composer.
// ---------------------------------------------------------------------------

/// Where one property definition came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PropRef {
    pub space: String,
    pub file_idx: usize,
    pub part_idx: usize,
    pub prop_idx: usize,
}

/// A property's build-time value after flattening.
#[derive(Debug, Clone)]
pub enum MergedValue {
    /// No layer supplied a value (a pure field).
    FieldOnly,
    /// Replace semantics, or a single defining layer: the winning definition.
    Single(PropRef),
    /// append/deep where every operand was a literal: the computed literal.
    Literal(ast::SExpr),
    /// append/deep with non-literal operands, or stack/pipe: the ordered
    /// chain of definitions (base first; `reverse` is applied at run time,
    /// not here).
    Chain(Vec<PropRef>),
}

#[derive(Debug, Clone)]
pub struct ComposedProp {
    pub name: String,
    /// Fixed by the base-most declaring layer (C5).
    pub storage: Option<ast::Storage>,
    pub kind: Option<(ast::MergeKind, bool)>,
    /// Declared shape from the base-most layer that states one.
    pub shape: Option<ast::SShape>,
    /// Every definition, base first.
    pub defs: Vec<PropRef>,
    pub value: MergedValue,
}

#[derive(Debug, Clone, Default)]
pub struct ComposedPart {
    /// Property name -> composed property. BTreeMap for determinism.
    pub props: BTreeMap<String, ComposedProp>,
}

/// Builtin `std` names (reference §9.11 and §9.x). The resolver treats the
/// space "std" as implicitly used by every space; these are its members.
pub const STD_PARTS: &[&str] = &["Request", "Event", "User", "Element", "log"];

pub const STD_FNS: &[&str] = &[
    "el", "publish", "subscribe", "signup", "login", "logout", "spawn", "redirect", "fail",
    "len", "range", "keys", "put", "drop", "slice", "find", "map", "filter", "sort", "join",
    "split", "contains", "text", "number", "json", "now", "id",
];

/// Properties of `std.log`.
pub const STD_LOG_FNS: &[&str] = &["debug", "info", "warn", "error"];

/// Fields of builtin parts, for longest-prefix resolution through them.
pub const STD_REQUEST_FIELDS: &[&str] = &["path", "method", "params", "data", "headers", "user"];
pub const STD_EVENT_FIELDS: &[&str] = &["name", "data"];
pub const STD_USER_FIELDS: &[&str] = &["id", "email"];
