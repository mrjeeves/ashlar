//! CONTRACT FILE — owned by the integrator. Module implementors: do not edit.
//! AST produced by the parser, consumed by the resolver and composer.
//!
//! Design notes that are part of the contract:
//!
//! * Dotted chains of identifiers in expression position (`chat.data.Store.messages`)
//!   parse as a single `Expr::NameRef` holding all segments. The parser cannot
//!   distinguish name-dots from field-dots; the resolver resolves the longest
//!   prefix as a visible name (local, parameter, enclosing-part property, part,
//!   or std name) and treats remaining segments as field accesses.
//! * `.field` after any non-identifier expression (`f(x).y`) parses as
//!   `Expr::Field`.

use crate::tokens::Span;

/// A dotted name as written: one or more segments.
pub type Name = Vec<String>;

pub fn name_to_string(n: &[String]) -> String {
    n.join(".")
}

/// One parsed `.ash` file.
#[derive(Debug, Clone, PartialEq)]
pub struct SrcFile {
    pub space: Name,
    pub space_span: Span,
    pub uses: Vec<(Name, Span)>,
    pub parts: Vec<PartDecl>,
    pub foreigns: Vec<ForeignDecl>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PartDecl {
    /// Bare (1 segment: introduces in the current space) or dotted
    /// (must match an existing visible part: declares a layer).
    pub name: Name,
    pub name_span: Span,
    /// The whole declaration, `part` keyword through closing `}` —
    /// the block `ashlar move` excises.
    pub span: Span,
    pub props: Vec<Prop>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Storage {
    State,
    Stored,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MergeKind {
    Append,
    Deep,
    Stack,
    Pipe,
}

#[derive(Debug, Clone, PartialEq)]
pub struct KindDecl {
    pub kind: MergeKind,
    /// `reverse` modifier; parser enforces it only follows stack/pipe (E020).
    pub reverse: bool,
    pub span: Span,
}

/// `[owned] [storage] name [kind [reverse]] [: shape] [= expression]`
///
/// `owned` scopes a `state`/`stored` property to the current user: each
/// authenticated user has their own value, isolated from every other
/// user (reference §9.3, ADR-0015).
#[derive(Debug, Clone, PartialEq)]
pub struct Prop {
    pub name: String,
    pub name_span: Span,
    /// The `owned` scope modifier: per-user storage. Only meaningful with a
    /// `storage` class; the parser rejects `owned` on a value property.
    pub owned: bool,
    pub storage: Option<(Storage, Span)>,
    pub kind: Option<KindDecl>,
    pub shape: Option<SShape>,
    pub value: Option<SExpr>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ForeignDecl {
    pub name: String,
    pub name_span: Span,
    /// Parameter shapes; names optional (`(url: text)` or `(text)`).
    pub params: Vec<(Option<String>, SShape)>,
    pub ret: SShape,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SShape {
    pub shape: Shape,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Shape {
    Text,
    Number,
    Bool,
    Data,
    List(Box<SShape>),
    /// Map with text keys: `{shape}`.
    Map(Box<SShape>),
    /// A part name used as a shape.
    Part(Name),
    /// `shape?`
    Opt(Box<SShape>),
    /// `(shapes) -> shape` — used by foreign declarations.
    Fn(Vec<(Option<String>, SShape)>, Box<SShape>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct SExpr {
    pub expr: Expr,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Text(String),
    Number(f64),
    Bool(bool),
    NoneLit,
    List(Vec<ListItem>),
    MapLit(Vec<MapItem>),
    /// Maximal dotted identifier chain; see module docs.
    NameRef(Name),
    Field(Box<SExpr>, String, Span),
    Index(Box<SExpr>, Box<SExpr>),
    Call(Box<SExpr>, Vec<SExpr>),
    /// `not x`, `-x`
    Unary(UnOp, Box<SExpr>),
    /// Postfix `x!`
    Assert(Box<SExpr>),
    Binary(BinOp, Box<SExpr>, Box<SExpr>),
    /// `if cond { .. } else { .. }` in expression position. Branch blocks;
    /// a branch's value is its final expression statement.
    IfExpr(Box<SExpr>, Vec<Stmt>, Vec<Stmt>),
    /// Function literal. Legal only as a property value or inside a call
    /// argument (reference §7); the parser accepts it anywhere an expression
    /// is legal and the RESOLVER rejects other positions.
    FnLit(Vec<Param>, Box<FnBody>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum ListItem {
    Item(SExpr),
    Spread(SExpr),
}

#[derive(Debug, Clone, PartialEq)]
pub enum MapItem {
    /// Key is a bare identifier or text literal.
    Entry(String, Span, SExpr),
    Spread(SExpr),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Param {
    pub name: String,
    pub name_span: Span,
    pub shape: SShape,
}

#[derive(Debug, Clone, PartialEq)]
pub enum FnBody {
    Expr(SExpr),
    Block(Vec<Stmt>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum UnOp {
    Not,
    Neg,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Or,
    And,
    EqEq,
    NotEq,
    Lt,
    LtEq,
    Gt,
    GtEq,
    Coalesce,
    Add,
    Sub,
    Mul,
    Div,
    Rem,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Stmt {
    Let(String, Span, SExpr),
    /// Assignment to a state property of the enclosing part (bare name only).
    Assign(String, Span, SExpr),
    If(SExpr, Vec<Stmt>, Option<Vec<Stmt>>),
    /// `for x in xs` (one var) or `for k, v in m` (two vars).
    For(Vec<(String, Span)>, SExpr, Vec<Stmt>),
    Return(Option<SExpr>, Span),
    Expr(SExpr),
}
