//! KIR: Kalam's typed, target-neutral intermediate representation, the lowering from
//! typed HIR, the IR passes, and the [`Backend`] trait that every code generator implements.
//!
//! KIR is what the **reference interpreter** runs (ROADMAP M2) and what the JS and C
//! emitters consume. Its invariants (ARCHITECTURE.md §10.2) are the contract that keeps a
//! future C backend possible and keeps JS semantics out of the language:
//! 1. Every expression has an explicit, fully inferred type: no `Ty::Var`, no `Ty::Error`
//!    ([`verify::verify`] checks this). Generics are kept (`Call::type_args`); C runs
//!    `monomorphize`.
//! 2. Every operation has spec semantics ([`PrimOp`] documents each), never "whatever the
//!    host does". `IntAdd` traps outside [−2⁶³, 2⁶³−1] (D18), `IntFloorDiv` floors and
//!    panics on zero, `FloatEq` is IEEE.
//! 3. Evaluation order is strictly left to right (§10.6). Where source order and KIR order
//!    differ (named arguments, record fields written out of order) lowering binds the
//!    values to temporaries first.
//! 4. Every suspension point is marked (`ExprKind::Call::suspends`, `Coerce::SyncToSusp`).
//!    Until `kalam-effects` runs in the pipeline these are `false`.
//! 5. No sugar: `match` is a chain of tests ([`ExprKind::IsVariant`], literal comparisons)
//!    ending in a panicking default, `?` is an `IsVariant` test plus an early
//!    [`Stmt::Return`], `and`/`or` are [`ExprKind::If`], interpolation is `*Show` plus `StrConcat`,
//!    compound assignment is a plain rebinding, `while` is a [`Stmt::Loop`] with a
//!    conditional `break`, and named arguments are positional.
//! 6. No JavaScript concepts (`undefined`, `Promise`, prototypes).
//!
//! Conventions for built-in sum types: `Option` is `None` = variant 0, `Some` = variant 1;
//! `Result` is `Ok` = variant 0, `Err` = variant 1. A user enum's variants are numbered in
//! declaration order.

pub mod lower;
pub mod verify;

use kalam_base::{FileId, FileSpan, Span};
use kalam_resolve::DefId;
use kalam_typeck::Ty;

pub type FnId = u32;
pub type LocalId = u32;

#[derive(Clone, Debug, Default)]
pub struct Program {
    pub package: String,
    pub functions: Vec<Function>,
    pub records: Vec<RecordDecl>,
    pub enums: Vec<EnumDecl>,
    /// Module constants (`let NAME = …` at module level), read with [`ExprKind::Const`],
    /// in dependency order (a constant comes after every constant its initializer names).
    /// Every initializer is evaluated once, **eagerly, before the entry point runs**
    /// (§13.4, Q33): a constant that panics stops the program before `main` starts.
    pub consts: Vec<ConstDecl>,
    /// The function that `main` lowered to, if this package is an executable.
    pub entry: Option<FnId>,
    /// When `main` returns `Result[(), E]` and `E` has an `Error` impl with a body: that
    /// impl's `message` function, used to report `Err` from `main` (§14.3).
    pub entry_error_message: Option<FnId>,
}

impl Program {
    pub fn function(&self, id: FnId) -> Option<&Function> {
        self.functions.iter().find(|f| f.id == id)
    }

    pub fn record(&self, def: DefId) -> Option<&RecordDecl> {
        self.records.iter().find(|r| r.def == def)
    }

    pub fn enum_decl(&self, def: DefId) -> Option<&EnumDecl> {
        self.enums.iter().find(|e| e.def == def)
    }

    /// The enum `name` declared in `module` (the runtime builds stdlib enum values such as
    /// `std/string.ParseError` and `std/prelude.Ordering`).
    pub fn enum_named(&self, module: &str, name: &str) -> Option<&EnumDecl> {
        self.enums
            .iter()
            .find(|e| e.module == module && e.name == name)
    }

    pub fn constant(&self, def: DefId) -> Option<&ConstDecl> {
        self.consts.iter().find(|c| c.def == def)
    }
}

/// A module constant: a pure initializer with no locals.
#[derive(Clone, Debug)]
pub struct ConstDecl {
    pub def: DefId,
    pub name: String,
    /// The file that declares it (where a panic in the initializer is reported).
    pub file: FileId,
    pub ty: Ty,
    pub value: Expr,
}

/// A record type: fields in declaration order (field `i` is `Field { index: i }`).
#[derive(Clone, Debug)]
pub struct RecordDecl {
    pub def: DefId,
    pub name: String,
    /// The defining module path (`main`, `std/prelude`).
    pub module: String,
    pub generics: u32,
    pub fields: Vec<(String, Ty)>,
}

#[derive(Clone, Debug)]
pub struct VariantDecl {
    pub def: DefId,
    pub name: String,
    pub fields: Vec<Ty>,
}

#[derive(Clone, Debug)]
pub struct EnumDecl {
    pub def: DefId,
    pub name: String,
    /// The defining module path (`main`, `std/string`).
    pub module: String,
    pub generics: u32,
    pub variants: Vec<VariantDecl>,
}

#[derive(Clone, Debug)]
pub struct LocalDecl {
    /// Source name (for debuggers and source maps); temporaries are named `$tN`.
    pub name: String,
    pub ty: Ty,
    /// Rebound by `Stmt::Assign` (a source `var`, or a lowering temporary).
    pub mutable: bool,
}

#[derive(Clone, Debug)]
pub struct Function {
    pub id: FnId,
    pub def: DefId,
    /// The canonical name as declared (identifiers are never translated between skins);
    /// used for JS exports.
    pub name: String,
    /// Number of type parameters (`Ty::Param(0..generics)`).
    pub generics: u32,
    pub params: Vec<(LocalId, Ty)>,
    /// Every local of the body, indexed by `LocalId` (parameters included).
    pub locals: Vec<LocalDecl>,
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
    /// Initializes `local` (its type is `ty`, also recorded in `Function::locals`).
    Let {
        local: LocalId,
        ty: Ty,
        value: Expr,
    },
    Assign {
        local: LocalId,
        value: Expr,
    },
    Expr(Expr),
    If {
        cond: Expr,
        then: Block,
        otherwise: Block,
    },
    /// A decision tree produced from `match`. `default` runs when no key matches; lowering
    /// makes it a panic when the match was exhaustive (defensive, §11).
    Switch {
        scrutinee: Expr,
        arms: Vec<(SwitchKey, Block)>,
        default: Block,
    },
    Loop {
        body: Block,
    },
    /// `for local in lo..hi` / `lo..=hi` over `Int`. `lo` then `hi` are evaluated once.
    /// Iteration never overflows: `lo..=9223372036854775807` ends after its last value.
    ForRange {
        local: LocalId,
        lo: Expr,
        hi: Expr,
        inclusive: bool,
        body: Block,
    },
    /// `for local in xs` over a `List`, `Set` (insertion order) or `Map` (yielding
    /// `(key, value)` tuples in insertion order).
    ForEach {
        local: LocalId,
        iter: Expr,
        body: Block,
    },
    Break,
    Continue,
    Return(Expr),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SwitchKey {
    /// A variant number of the scrutinee's enum, `Option` or `Result` (see crate docs).
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
    /// Exact decimal literal text (`19.99`), parsed by the backend's decimal128 runtime.
    Decimal(String),
    /// Exact integer literal text of any size.
    BigInt(String),
    Str(String),
    Bool(bool),
    Unit,
    Prim {
        op: PrimOp,
        args: Vec<Expr>,
    },
    Call {
        callee: Callee,
        args: Vec<Expr>,
        /// Type arguments of a generic callee, in its generics order.
        type_args: Vec<Ty>,
        suspends: bool,
    },
    /// Record construction; `fields` in declaration order.
    Record {
        def: DefId,
        fields: Vec<Expr>,
    },
    /// User enum variant, `Option` or `Result` construction (`def` is the variant's
    /// `DefId` for user enums and `None` for the built-in ones).
    Variant {
        def: Option<DefId>,
        index: u32,
        fields: Vec<Expr>,
    },
    /// Record field `index` (declaration order).
    Field {
        base: Box<Expr>,
        index: u32,
    },
    /// `true` when `base` (an enum, `Option` or `Result` value) is variant `variant`.
    IsVariant {
        base: Box<Expr>,
        variant: u32,
    },
    /// Payload field `index` of variant `variant`. Only valid where an `IsVariant` test or
    /// a `Switch` established the tag; backends may assume the tag matches.
    VariantField {
        base: Box<Expr>,
        variant: u32,
        index: u32,
    },
    Tuple(Vec<Expr>),
    TupleField {
        base: Box<Expr>,
        index: u32,
    },
    List(Vec<Expr>),
    /// `{k: v, …}`: entries in source order; a repeated key keeps its first position and
    /// takes the last value (insertion order, §9.3).
    Map(Vec<(Expr, Expr)>),
    /// A function with a body used as a value (`xs.map(double)`).
    FnRef(FnId),
    /// A module constant (see [`Program::consts`]).
    Const(DefId),
    /// A conditional expression; also how `and`/`or` short-circuit.
    If {
        cond: Box<Expr>,
        then: Box<Expr>,
        otherwise: Box<Expr>,
    },
    /// Runs `stmts`, then yields `value` (value-form `if`/`match` arms with statements).
    Block {
        stmts: Vec<Stmt>,
        value: Box<Expr>,
    },
    /// A closure: a lambda or a local `fn` (§12.3). Its parameters and locals live in the
    /// enclosing function's local table. It captures the enclosing locals **by value** when
    /// it is created (K1110 rules out capturing a `var`). A local `fn` names itself with
    /// `self_local`, which is bound to the closure inside its own body (recursion).
    Lambda {
        params: Vec<(LocalId, Ty)>,
        body: Box<Block>,
        suspends: bool,
        self_local: Option<LocalId>,
    },
    Coerce {
        kind: Coerce,
        value: Box<Expr>,
    },
    /// A tagged literal: the template's literal parts (provenance-checked) plus hole values.
    Template {
        tag: String,
        parts: Vec<String>,
        holes: Vec<Expr>,
    },
    /// `panic(msg)`: type `Never`.
    Panic(Box<Expr>),
}

impl Expr {
    pub fn new(kind: ExprKind, ty: Ty, span: Span) -> Expr {
        Expr { kind, ty, span }
    }
}

#[derive(Clone, Debug)]
pub enum Callee {
    Fn(FnId),
    Value(Box<Expr>),
    /// A core prelude function.
    Prelude(PreludeFn),
    /// A stdlib `@builtin` function, provided by each backend's runtime. The key is
    /// `Type.method` for a method or associated function of a built-in type
    /// (`String.to_int`, `Result.map_err`, `Decimal.from`) and `module.name` for a module
    /// function (`std/math.round`). Arguments are positional, the receiver first.
    Intrinsic(String),
}

/// Core functions every backend provides (spec §7.2, §16.4: ambient).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PreludeFn {
    /// `print(s: String)`: writes `s` and a newline to stdout. Lowering applies `Show`
    /// first, so the argument is always a `String`.
    Print,
    /// `assert(cond, msg)`: panics with `msg` when `cond` is false.
    Assert,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Coerce {
    /// A sync function stored where a suspending one is expected. No-op in JS; an adapter in C.
    SyncToSusp,
}

/// Primitive operations with spec-defined semantics (invariant 2). Each takes the number
/// of arguments [`PrimOp::arity`] gives, evaluated left to right.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrimOp {
    // Int: 64-bit two's complement range, checked (D18, §9.2).
    /// Panics on overflow.
    IntAdd,
    /// Panics on overflow.
    IntSub,
    /// Panics on overflow.
    IntMul,
    /// Floors (Python `//`); panics on zero; `−2⁶³ // −1` overflows and panics.
    IntFloorDiv,
    /// Sign of the divisor (Python `%`); panics on zero.
    IntMod,
    /// `/` on two `Int`s: converts both to `Float` (round to nearest) and divides.
    IntToFloatDiv,
    /// Panics on `−(−2⁶³)`.
    IntNeg,
    IntEq,
    IntLt,
    IntLe,
    IntGt,
    IntGe,
    /// Decimal digits, `-` sign, no separators.
    IntShow,
    // Float: IEEE-754 binary64.
    FloatAdd,
    FloatSub,
    FloatMul,
    FloatDiv,
    FloatNeg,
    /// IEEE equality (`NaN == NaN` is false; `!=` is `Not(FloatEq)`).
    FloatEq,
    FloatLt,
    FloatLe,
    FloatGt,
    FloatGe,
    /// Shortest round-trip decimal text (spec §20 `std/string` formatting is M4).
    FloatShow,
    // Decimal: decimal128, 34 significant digits, exact; traps on overflow.
    DecAdd,
    DecSub,
    DecMul,
    DecNeg,
    DecEq,
    DecLt,
    DecLe,
    DecGt,
    DecGe,
    DecShow,
    // BigInt: arbitrary precision, same operators as Int without overflow.
    BigAdd,
    BigSub,
    BigMul,
    BigFloorDiv,
    BigMod,
    BigNeg,
    BigEq,
    BigLt,
    BigLe,
    BigGt,
    BigGe,
    BigShow,
    // String: Unicode scalar values; equality and order are by code point.
    StrConcat,
    StrEq,
    StrLt,
    StrLe,
    StrGt,
    StrGe,
    // Bool.
    BoolEq,
    /// `true` / `false`.
    BoolShow,
    Not,
    /// Structural equality of two values of one `Eq` type (records, enums, tuples,
    /// collections). Never runs user code (§11 rule 11).
    StructEq,
    /// `xs[i]`: panics when `i` is out of bounds or negative (§10.8).
    ListGet,
    ListLen,
    ListConcat,
    /// `xs[from..to]` for list patterns (`[first, ..rest]`): the elements from index
    /// `from` up to, not including, `to`. Panics when `0 <= from <= to <= len` fails.
    ListSlice,
    /// `m[k]`: panics when the key is absent (§10.8).
    MapGet,
}

impl PrimOp {
    pub fn arity(self) -> usize {
        use PrimOp::*;
        match self {
            IntNeg | IntShow | FloatNeg | FloatShow | DecNeg | DecShow | BigNeg | BigShow
            | BoolShow | Not | ListLen => 1,
            ListSlice => 3,
            _ => 2,
        }
    }
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
/// reference interpreter (`kalam-interp`, which runs a [`Program`] directly) and the C
/// emitter (future).
pub trait Backend {
    fn name(&self) -> &'static str;
    fn targets(&self) -> &'static [Target];
    fn emit(&self, program: &Program, options: &EmitOptions)
        -> Result<Vec<Artifact>, BackendError>;
}
