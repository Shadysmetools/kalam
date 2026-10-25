//! The resolved program the type checker consumes: a small, skin-free HIR in which every
//! name is already resolved, plus the [`ResolvedProgram`] trait that serves it.
//!
//! **Contract with `kalam-resolve` (ARCHITECTURE.md §8.1).** `kalam-typeck` depends on
//! `kalam-resolve`, so resolve cannot implement a trait defined here without a dependency
//! cycle. The adapter is therefore `impl ResolvedProgram for <resolve's output>`, written in
//! this crate (allowed: the trait is local) or in `kalam-driver`. Resolve's obligations:
//!
//! 1. Every item, impl/trait method, enum variant and record gets a [`DefId`]; every local
//!    and parameter gets a [`LocalId`] unique within its body (the owning item's body).
//! 2. Every name use is an [`ExprKind::Local`], [`ExprKind::Def`] or [`ExprKind::Lang`]
//!    (core names: `print`, `panic`, `assert`, `Some`, `None`, `Ok`, `Err`); unresolved
//!    names become [`ExprKind::Error`] after resolve has reported K0401.
//! 3. Implicit members (`.Circle`) stay [`ExprKind::ImplicitMember`]/[`PatKind::Variant`]
//!    with `def: None`; typeck resolves them against the expected type (K0803).
//! 4. Type syntax is lowered to [`Type`]: core and fixed-ASCII types to
//!    [`BuiltinType`], user types to `TypeKind::Def`, generic parameters to
//!    `TypeKind::Param(i)` (index into the item's generics; an impl's generics come first,
//!    then the method's), `Self` to `TypeKind::SelfType`.
//! 5. Record constructors `User(name: …)` are calls whose callee is `ExprKind::Def(record)`.
//! 6. Every [`Expr`] carries an [`ExprId`] unique within its body; typeck keys its results
//!    by `(owner DefId, ExprId)`.
//! 7. No-shadowing, one-namespace, visibility and `break`-outside-loop are resolve's checks;
//!    typeck assumes them.
//!
//! [`HirProgram`] is a plain in-memory implementation, used by unit tests and usable by
//! the adapter.

use std::collections::HashMap;

use kalam_base::{FileId, Span};
use kalam_resolve::{DefId, ModulePath};

use crate::traits::{BuiltinTrait, TraitId};
use crate::ty::{Capability, SinkType};

/// The interface between name resolution and type checking.
pub trait ResolvedProgram {
    /// Every item, in any order: functions (top-level, impl and trait methods, signature
    /// stubs), records, enums, aliases, traits, impls and constants. Variants are not
    /// items; they are listed inside their enum.
    fn item_ids(&self) -> Vec<DefId>;
    /// `None` when `def` is not an item (a local, a variant, a prelude name the adapter
    /// does not model). The checker treats that as an already-reported error, never a
    /// panic, so malformed input cannot crash it.
    fn item(&self, def: DefId) -> Option<&Item>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct LocalId(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ExprId(pub u32);

/// Core value names (spec §7.2). They are reserved in every skin, so resolve maps them
/// directly.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LangItem {
    Print,
    Panic,
    Assert,
    Some,
    None,
    Ok,
    Err,
}

/// Built-in type constructors: the core type names (§7.2) and the fixed-ASCII prelude types.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BuiltinType {
    Int,
    BigInt,
    Float,
    Decimal,
    Bool,
    String,
    Bytes,
    Never,
    List,
    Map,
    Set,
    Option,
    Result,
    Range,
    Secret,
    AnyError,
    JsValue,
    Capability(Capability),
    Sink(SinkType),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Type {
    pub kind: TypeKind,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub enum TypeKind {
    Builtin(BuiltinType, Vec<Type>),
    /// A record, enum or alias.
    Def(DefId, Vec<Type>),
    Param(u32),
    SelfType,
    Unit,
    Tuple(Vec<Type>),
    Fn {
        /// `@sync fn(…)`
        sync: bool,
        params: Vec<Type>,
        ret: Box<Type>,
    },
    Error,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TraitBound {
    pub id: TraitId,
    pub args: Vec<Type>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GenericParamDecl {
    pub name: String,
    pub bounds: Vec<TraitBound>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    pub def: DefId,
    /// Exact declared spelling (identifiers are never translated).
    pub name: String,
    pub module: ModulePath,
    pub file: FileId,
    pub span: Span,
    pub exported: bool,
    pub kind: ItemKind,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ItemKind {
    Fn(FnDecl),
    Record(RecordDecl),
    Enum(EnumDecl),
    Alias(AliasDecl),
    Trait(TraitItem),
    Impl(ImplItem),
    Const(ConstDecl),
}

#[derive(Clone, Debug, PartialEq)]
pub struct ParamDecl {
    pub name: String,
    pub local: LocalId,
    pub ty: Type,
    pub default: Option<Expr>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FnDecl {
    pub generics: Vec<GenericParamDecl>,
    /// `self` is the first parameter of a method, with `ty` = `TypeKind::SelfType`.
    pub has_self: bool,
    pub params: Vec<ParamDecl>,
    /// `None` means `()` (§9.9 rule 1).
    pub ret: Option<Type>,
    /// `@sync` on the declaration (§15.4, K1402 is checked by `kalam-effects`).
    pub sync_attr: bool,
    /// The impl or trait this method belongs to.
    pub parent: Option<DefId>,
    /// `None` for a signature stub (stdlib stubs in M2, extern, required trait method).
    pub body: Option<Body>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LocalDecl {
    pub name: String,
    /// `var` (rebindable, §10.2).
    pub mutable: bool,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Body {
    /// Indexed by `LocalId`. Parameters are locals too.
    pub locals: Vec<LocalDecl>,
    pub value: BodyValue,
}

#[derive(Clone, Debug, PartialEq)]
pub enum BodyValue {
    /// `fn f(): <block>`: a non-`()` return needs `return` on every path (K1101).
    Block(Block),
    /// `fn f() => expr`
    Expr(Expr),
}

#[derive(Clone, Debug, PartialEq)]
pub struct FieldDecl {
    pub name: String,
    pub ty: Type,
    pub default: Option<Expr>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RecordDecl {
    pub generics: Vec<GenericParamDecl>,
    pub fields: Vec<FieldDecl>,
    pub opaque: bool,
    pub derives: Vec<BuiltinTrait>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct VariantFieldDecl {
    pub label: Option<String>,
    pub ty: Type,
}

#[derive(Clone, Debug, PartialEq)]
pub struct VariantDecl {
    pub def: DefId,
    pub name: String,
    pub fields: Vec<VariantFieldDecl>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EnumDecl {
    pub generics: Vec<GenericParamDecl>,
    pub variants: Vec<VariantDecl>,
    pub opaque: bool,
    pub derives: Vec<BuiltinTrait>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AliasDecl {
    pub generics: Vec<GenericParamDecl>,
    pub target: Type,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TraitItem {
    pub generics: Vec<GenericParamDecl>,
    pub methods: Vec<DefId>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ImplItem {
    pub generics: Vec<GenericParamDecl>,
    pub self_ty: Type,
    /// `impl T as Trait:`
    pub as_trait: Option<TraitBound>,
    pub methods: Vec<DefId>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ConstDecl {
    pub ty: Option<Type>,
    /// A body with no locals and an expression value.
    pub value: Expr,
}

// ---------------------------------------------------------------------------------------
// Statements and expressions
// ---------------------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
pub struct Block {
    pub stmts: Vec<Stmt>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Stmt {
    pub kind: StmtKind,
    pub span: Span,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AssignOp {
    Assign,
    Add,
    Sub,
    Mul,
    Div,
}

#[derive(Clone, Debug, PartialEq)]
pub enum StmtKind {
    /// An expression statement; `if`/`match` here are statement forms.
    Expr(Expr),
    Let {
        pat: Pat,
        ty: Option<Type>,
        value: Expr,
    },
    Var {
        local: LocalId,
        ty: Option<Type>,
        value: Expr,
    },
    /// `place op= value`; `place` is a local, field chain or index (§10.4).
    Assign {
        place: Expr,
        op: AssignOp,
        value: Expr,
    },
    Return(Option<Expr>),
    Break,
    Continue,
    Pass,
    For {
        pat: Pat,
        iter: Expr,
        body: Block,
    },
    While {
        cond: Expr,
        body: Block,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinOp {
    Or,
    And,
    Eq,
    NotEq,
    Lt,
    Le,
    Gt,
    Ge,
    Add,
    Sub,
    Mul,
    Div,
    IntDiv,
    Rem,
}

impl BinOp {
    pub fn symbol(self) -> &'static str {
        match self {
            BinOp::Or => "or",
            BinOp::And => "and",
            BinOp::Eq => "==",
            BinOp::NotEq => "!=",
            BinOp::Lt => "<",
            BinOp::Le => "<=",
            BinOp::Gt => ">",
            BinOp::Ge => ">=",
            BinOp::Add => "+",
            BinOp::Sub => "-",
            BinOp::Mul => "*",
            BinOp::Div => "/",
            BinOp::IntDiv => "//",
            BinOp::Rem => "%",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnOp {
    Neg,
    Not,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Arg {
    pub label: Option<String>,
    pub value: Expr,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub enum StrPart {
    /// Literal text with escapes already decoded.
    Text(String),
    Hole(Expr),
}

#[derive(Clone, Debug, PartialEq)]
pub struct IfBranch {
    pub cond: Expr,
    pub body: Block,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Arm {
    pub pat: Pat,
    pub guard: Option<Expr>,
    pub body: Block,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LambdaParam {
    pub local: LocalId,
    pub ty: Option<Type>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Expr {
    pub id: ExprId,
    pub kind: ExprKind,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ExprKind {
    /// An integer literal; `None` when it does not fit in `u128` (always K0126).
    Int(Option<u128>),
    /// A float literal: ASCII-normalized source text (exact for `Decimal` context).
    Float(String),
    Str(String),
    /// `"a {x} b"`: holes must be `Show` (§9.7).
    Interp(Vec<StrPart>),
    /// `sql"…"` etc.: literal parts plus holes (§17). Hole requirements come from
    /// `kalam-sinks` once its parsers land.
    Tagged {
        tag: kalam_sinks::Tag,
        parts: Vec<String>,
        holes: Vec<Expr>,
    },
    Bool(bool),
    Unit,
    Local(LocalId),
    /// A function, constant, record (constructor), enum variant (qualified, `Shape.Circle`)
    /// or trait method used by name (`Trait.m`).
    Def(DefId),
    Lang(LangItem),
    /// `.Circle`, resolved against the expected type.
    ImplicitMember(String),
    Call {
        callee: Box<Expr>,
        args: Vec<Arg>,
    },
    MethodCall {
        receiver: Box<Expr>,
        name: String,
        args: Vec<Arg>,
    },
    Field {
        base: Box<Expr>,
        name: String,
    },
    TupleField {
        base: Box<Expr>,
        index: u32,
    },
    Index {
        base: Box<Expr>,
        index: Box<Expr>,
    },
    Unary {
        op: UnOp,
        operand: Box<Expr>,
    },
    Binary {
        op: BinOp,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
    },
    Range {
        lo: Box<Expr>,
        hi: Box<Expr>,
        inclusive: bool,
    },
    Tuple(Vec<Expr>),
    List(Vec<Expr>),
    /// Both the statement form and the value form (§10.3); `Stmt::Expr` position means
    /// statement form.
    If {
        branches: Vec<IfBranch>,
        else_body: Option<Block>,
    },
    Match {
        scrutinee: Box<Expr>,
        arms: Vec<Arm>,
    },
    /// `fn(x) => body` (one expression, DR-15).
    Lambda {
        params: Vec<LambdaParam>,
        body: Box<Expr>,
    },
    Error,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Pat {
    pub kind: PatKind,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub enum PatKind {
    Wild,
    Bind(LocalId),
    /// An `Int` literal pattern; `negative` for `-5`.
    Int {
        negative: bool,
        value: u128,
    },
    /// `1..=9` (`Int` ranges only, §11 rule 6).
    Range {
        lo: i128,
        hi: i128,
        inclusive: bool,
    },
    /// A `Float` literal pattern is K1005; resolve keeps it so typeck can say so.
    Float(String),
    Str(String),
    Bool(bool),
    Tuple(Vec<Pat>),
    /// `Some(p)`, `None`, `Ok(p)`, `Err(p)`.
    Lang {
        item: LangItem,
        inner: Option<Box<Pat>>,
    },
    /// `.V(…)` (`def: None`) or `Shape.V(…)`.
    Variant {
        def: Option<DefId>,
        name: String,
        fields: Vec<Pat>,
        rest: bool,
    },
    Or(Vec<Pat>),
    Error,
}

impl Pat {
    /// Irrefutable patterns may appear in `let` and `for` (K1003).
    pub fn is_irrefutable(&self) -> bool {
        match &self.kind {
            PatKind::Wild | PatKind::Bind(_) | PatKind::Error => true,
            PatKind::Tuple(ps) => ps.iter().all(Pat::is_irrefutable),
            _ => false,
        }
    }
}

// ---------------------------------------------------------------------------------------
// In-memory program
// ---------------------------------------------------------------------------------------

/// A plain in-memory [`ResolvedProgram`].
#[derive(Clone, Debug, Default)]
pub struct HirProgram {
    items: Vec<Item>,
    index: HashMap<DefId, usize>,
}

impl HirProgram {
    pub fn new() -> HirProgram {
        HirProgram::default()
    }

    pub fn add(&mut self, item: Item) {
        self.index.insert(item.def, self.items.len());
        self.items.push(item);
    }

    /// Replaces the item with the same `DefId`.
    pub fn replace(&mut self, item: Item) {
        if let Some(&i) = self.index.get(&item.def) {
            self.items[i] = item;
        }
    }

    pub fn items(&self) -> &[Item] {
        &self.items
    }
}

impl ResolvedProgram for HirProgram {
    fn item_ids(&self) -> Vec<DefId> {
        self.items.iter().map(|i| i.def).collect()
    }

    fn item(&self, def: DefId) -> Option<&Item> {
        self.index.get(&def).map(|&i| &self.items[i])
    }
}
