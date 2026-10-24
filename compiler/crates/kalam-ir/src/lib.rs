//! KIR: Kalam's typed, target-neutral intermediate representation, the lowering from
//! typed HIR, the IR passes, and the [`Backend`] trait that every code generator implements.
//!
//! KIR invariants (ARCHITECTURE.md §10.2), which are the contract that keeps a future C
//! backend possible:
//! 1. Every expression has an explicit type. Generics are kept; C runs `monomorphize`.
//! 2. Every operation has spec semantics (`IntAdd` traps outside ±(2^53 − 1), `IntFloorDiv`
//!    floors and panics on zero, …), never "whatever the host does".
//! 3. Evaluation order is explicit (temporaries where a backend could reorder).
//! 4. Every suspension point is marked (`Call::suspends`, `Coerce::SyncToSusp`).
//! 5. No sugar: `?`, `match`, value-semantic assignment, interpolation, tagged literals,
//!    defaults and named arguments are all desugared.
//! 6. No JavaScript concepts (`undefined`, `Promise`, prototypes).
//!
//! Status: data types and the backend seam. Lowering and passes are Phase 1.

use kalam_base::{FileSpan, Span};
use kalam_resolve::DefId;
use kalam_typeck::Ty;

pub type FnId = u32;
pub type LocalId = u32;

#[derive(Clone, Debug)]
pub struct Program {
    pub package: String,
    pub functions: Vec<Function>,
    /// The function that `main` lowered to, if this package is an executable.
    pub entry: Option<FnId>,
}

#[derive(Clone, Debug)]
pub struct Function {
    pub id: FnId,
    pub def: DefId,
    /// The canonical name as declared (identifiers are never translated between skins);
    /// used for JS exports.
    pub name: String,
    pub params: Vec<(LocalId, Ty)>,
    pub ret: Ty,
    pub body: Block,
    /// Result of suspension inference for this instance.
    pub suspends: bool,
    /// `Sync` or `Susp` instance of an effect-polymorphic function (`f` / `f$s`).
    pub instance: Instance,
    pub exported: bool,
    pub span: FileSpan,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Instance {
    Sync,
    Susp,
}

#[derive(Clone, Debug, Default)]
pub struct Block {
    pub stmts: Vec<Stmt>,
}

#[derive(Clone, Debug)]
pub enum Stmt {
    Let { local: LocalId, ty: Ty, value: Expr },
    Assign { local: LocalId, value: Expr },
    Expr(Expr),
    If { cond: Expr, then: Block, otherwise: Block },
    /// A decision tree produced from `match` (defensive panicking default included).
    Switch { scrutinee: Expr, arms: Vec<(SwitchKey, Block)>, default: Block },
    Loop { body: Block },
    Break,
    Continue,
    Return(Expr),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SwitchKey {
    Variant(u32),
    Int(i64),
    Str(String),
    Bool(bool),
}

#[derive(Clone, Debug)]
pub struct Expr {
    pub kind: ExprKind,
    pub ty: Ty,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub enum ExprKind {
    Local(LocalId),
    Int(i64),
    Float(f64),
    Str(String),
    Bool(bool),
    Unit,
    Prim { op: PrimOp, args: Vec<Expr> },
    Call { callee: Callee, args: Vec<Expr>, suspends: bool },
    Record { def: DefId, fields: Vec<Expr> },
    Variant { def: DefId, index: u32, fields: Vec<Expr> },
    Field { base: Box<Expr>, index: u32 },
    Lambda { params: Vec<(LocalId, Ty)>, body: Box<Block>, suspends: bool },
    Coerce { kind: Coerce, value: Box<Expr> },
    /// A tagged literal: the template's literal parts (provenance-checked) plus hole values.
    Template { tag: String, parts: Vec<String>, holes: Vec<Expr> },
    Panic(Box<Expr>),
}

#[derive(Clone, Debug)]
pub enum Callee {
    Fn(FnId),
    Value(Box<Expr>),
    /// A runtime primitive provided per target (`std/_platform/<target>`).
    Intrinsic(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Coerce {
    /// A sync function stored where a suspending one is expected. No-op in JS; an adapter in C.
    SyncToSusp,
}

/// Primitive operations with spec-defined semantics (invariant 2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrimOp {
    IntAdd,
    IntSub,
    IntMul,
    IntFloorDiv,
    IntMod,
    IntToFloatDiv,
    IntNeg,
    IntEq,
    IntLt,
    FloatAdd,
    FloatSub,
    FloatMul,
    FloatDiv,
    FloatEq,
    FloatLt,
    StrConcat,
    StrEq,
    StructEq,
    Not,
}

/// Compilation targets (spec §13.5 `targets`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Target {
    JsNode,
    JsBrowser,
    /// Future native target (ARCHITECTURE.md §20).
    C,
}

#[derive(Clone, Debug, Default)]
pub struct EmitOptions {
    pub release: bool,
    pub source_maps: bool,
}

/// One output file.
#[derive(Clone, Debug)]
pub struct Artifact {
    pub path: String,
    pub contents: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BackendError {
    Unimplemented(&'static str),
    UnsupportedTarget(Target),
}

/// The seam between the shared frontend and every code generator: the JS emitter today, the
/// reference interpreter (planned) and the C emitter (future).
pub trait Backend {
    fn name(&self) -> &'static str;
    fn targets(&self) -> &'static [Target];
    fn emit(&self, program: &Program, options: &EmitOptions) -> Result<Vec<Artifact>, BackendError>;
}
