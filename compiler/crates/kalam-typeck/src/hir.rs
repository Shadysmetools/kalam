//! The type checker's input: a small, skin-free HIR in which every name is already
//! resolved.
//!
//! **Where it comes from.** [`crate::lower::lower_program`] builds it from the real output
//! of name resolution, `kalam_resolve::Program`, together with each module's `kalam_ast`
//! tree (ARCHITECTURE.md §8.1: "the adapter walks `kalam_ast` together with resolve's
//! `Program` (its `uses`/`decls` tables keyed by name spans) and emits HIR"). Resolve keeps
//! owning `DefId` and `ModulePath`; HIR reuses them unchanged. [`crate::builder`] builds HIR
//! by hand for unit tests of later passes.
//!
//! Resolve's obligations, which the lowering relies on:
//!
//! 1. Every item, impl/trait method, enum variant and record has a [`DefId`]; every local
//!    and parameter of a body gets a [`LocalId`] unique within that body (the owning item's
//!    body; lambdas and local `fn`s share their enclosing body's table).
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
//! A `DefId` that names no item is treated by the checker as an already-reported error,
//! never a panic: the type checker is a fuzz target.

use std::collections::HashMap;

use kalam_base::{FileId, Skin, Span};
use kalam_resolve::{DefId, ModulePath};

use crate::traits::{BuiltinTrait, TraitId};
use crate::ty::{Capability, SinkType};

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
    /// A record, enum, alias or extern type.
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
    /// `extern type Db` (a type with no visible structure, spec §19.1), including the
    /// stdlib's opaque built-in stubs that are not core types (`Cell[T]`, `Instant`, …).
    Extern(ExternTypeDecl),
    /// `test "name": body` (checked like the body of `fn() -> ()`).
    Test(Body),
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
    /// `@positional` (§12.1): exempt from K1104.
    pub positional: bool,
    /// The impl or trait this method belongs to.
    pub parent: Option<DefId>,
    /// `None` for a signature stub (extern, required trait method).
    pub body: Option<Body>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LocalDecl {
    pub name: String,
    /// `var` (rebindable, §10.2).
    pub mutable: bool,
    pub span: Span,
    /// The closure (lambda or local `fn`) that declares it: 0 for the item's own body, then
    /// one id per closure in the body. A use from another closure is a capture (§12.3).
    pub closure: u32,
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
    /// Where each of `derives` is written (`Ord` in `@derive(Eq, Ord)`), in the same
    /// order; empty when unknown (hand-built HIR). A derive whose fields do not all
    /// implement the trait is reported there (K0603).
    pub derive_spans: Vec<Span>,
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
    /// As [`RecordDecl::derive_spans`].
    pub derive_spans: Vec<Span>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AliasDecl {
    pub generics: Vec<GenericParamDecl>,
    pub target: Type,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ExternTypeDecl {
    pub generics: Vec<GenericParamDecl>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TraitItem {
    pub generics: Vec<GenericParamDecl>,
    pub methods: Vec<DefId>,
    /// The stdlib's declaration of a built-in trait (`trait Show:` in `std/prelude`): the
    /// trait is [`TraitId::Builtin`] and this item only supplies its methods.
    pub builtin: Option<BuiltinTrait>,
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
    /// The column the statement starts at when it begins its own line (4 spaces per
    /// block level, §5.8 rule 1); `None` in a one-line block (`if c: x = 1`) and in
    /// hand-built HIR. Fixes that insert a statement before this one need it (K1110).
    pub indent: Option<u32>,
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
    /// A local `fn` (§12.3). Its parameters and locals live in the enclosing body's table,
    /// declared with this closure id.
    LocalFn {
        local: LocalId,
        closure: u32,
        decl: Box<FnDecl>,
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

    pub fn is_comparison(self) -> bool {
        matches!(
            self,
            BinOp::Eq | BinOp::NotEq | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge
        )
    }

    pub fn is_arithmetic(self) -> bool {
        matches!(
            self,
            BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::IntDiv | BinOp::Rem
        )
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
    /// The column of the arm's `case` (4 spaces per block level, §5.8 rule 1); `None` in
    /// hand-built HIR. K1001's fix inserts the missing arms there (§11 rule 1).
    pub indent: Option<u32>,
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
    /// `kalam-sinks` once its parsers land; `path"…"` holes are `PathSegment` (K1641).
    Tagged {
        tag: kalam_sinks::Tag,
        parts: Vec<String>,
        holes: Vec<Expr>,
    },
    Bool(bool),
    Unit,
    Local(LocalId),
    /// A function, constant, record (constructor), enum variant (qualified, `Shape.Circle`),
    /// an associated function (`Email.parse`) or a trait method used by name (`Trait.m`).
    Def(DefId),
    Lang(LangItem),
    /// `.Circle`, resolved against the expected type.
    ImplicitMember(String),
    /// `Type.name` that name resolution could not settle (a member of a built-in type such
    /// as `Decimal.from`, or a trait method reached through a type): an inherent or trait
    /// associated function, chosen by DR-27's exactly-one rule.
    AssocFn {
        ty: Type,
        name: String,
    },
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
    /// `{k: v, …}`; `{}` needs an expected type (K0802).
    Map(Vec<(Expr, Expr)>),
    /// `e?` (§14.3).
    Try(Box<Expr>),
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
    /// `fn(x) => body` (one expression, DR-15). Its parameters are declared with this
    /// closure id (§12.3: a use of a `var` from outside is K1110).
    Lambda {
        params: Vec<LambdaParam>,
        ret: Option<Type>,
        body: Box<Expr>,
        closure: u32,
    },
    /// `unsafe.raw_sql(…)` and the other escape hatches (§17.1). Typed with the capability
    /// and sink rules of the next iteration; until then it is poison.
    Unsafe(String),
    /// A name that denotes a type (not a record constructor) used where a value goes:
    /// `first[Int](xs)` (explicit type arguments, K0922). Elsewhere it is poison.
    TypeName(Type),
    Error,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Pat {
    pub kind: PatKind,
    pub span: Span,
}

/// One field of a variant or record pattern: `w`, `width: w`.
#[derive(Clone, Debug, PartialEq)]
pub struct PatField {
    pub label: Option<String>,
    pub pat: Pat,
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
    /// `.V(…)` (`def: None`) or `Shape.V(…)`. Fields are positional (declaration order) or
    /// labelled; `rest` is a trailing `..`.
    Variant {
        def: Option<DefId>,
        name: String,
        fields: Vec<PatField>,
        rest: bool,
    },
    /// `User(role: .Admin, name: n, ..)` (§11 rule 8).
    Record {
        def: DefId,
        fields: Vec<PatField>,
        rest: bool,
    },
    /// `[]`, `[x]`, `[first, ..rest]`, `[.., last]` (§11 rule 7). `rest` is `Some` when
    /// the pattern has `..`, with the binding of `..name`.
    List {
        before: Vec<Pat>,
        rest: Option<Option<LocalId>>,
        after: Vec<Pat>,
    },
    Or(Vec<Pat>),
    /// `p as name` (§11 rule 9).
    As {
        pat: Box<Pat>,
        local: LocalId,
    },
    Error,
}

impl Pat {
    /// A pattern that matches every value of a single-constructor type: a quick test only;
    /// the checker decides refutability with the usefulness algorithm (K1003).
    pub fn is_irrefutable(&self) -> bool {
        match &self.kind {
            PatKind::Wild | PatKind::Bind(_) | PatKind::Error => true,
            PatKind::Tuple(ps) => ps.iter().all(Pat::is_irrefutable),
            PatKind::As { pat, .. } => pat.is_irrefutable(),
            PatKind::Record { fields, .. } => fields.iter().all(|f| f.pat.is_irrefutable()),
            _ => false,
        }
    }
}

// ---------------------------------------------------------------------------------------
// The program
// ---------------------------------------------------------------------------------------

/// A whole program in HIR form.
#[derive(Clone, Debug, Default)]
pub struct HirProgram {
    items: Vec<Item>,
    index: HashMap<DefId, usize>,
    /// The members of built-in types are known: the stdlib signature stubs are part of the
    /// program, so an unknown method or field of a built-in type is K0420. Without them a
    /// member of a built-in type is poison (nothing is reported about it).
    pub api_complete: bool,
    /// The stdlib's `World` record stub, whose fields (`world.net`, `world.stdin`, …) are
    /// the fields of the capability type `World` (§16.1).
    pub world: Option<DefId>,
    /// The source text between the quotes of each plain one-line string literal (escapes
    /// as written), by `(owner, ExprId)`. K1601's fix rewrites `"…" + x` concatenation into
    /// a tagged literal from these texts (§17.1).
    pub raw_strings: HashMap<(DefId, ExprId), String>,
    /// For each body owner, how many times each local is used (K1601's fix may rewrite the
    /// initializer of a local used once).
    pub local_uses: HashMap<(DefId, LocalId), u32>,
    /// Each file's skin: a fix that inserts a keyword (K1101's `return`) spells it the way
    /// the file does.
    pub skins: HashMap<FileId, Skin>,
    /// The program's `main` functions (top-level functions declared with the core name
    /// `main` outside the stdlib): their signature is fixed by §14.3 and §16.1 rule 1.
    pub mains: Vec<DefId>,
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

    /// Every item, in program order: functions (top-level, impl and trait methods, signature
    /// stubs), records, enums, aliases, traits, impls, constants, extern types and tests.
    /// Variants are not items; they are listed inside their enum.
    pub fn item_ids(&self) -> Vec<DefId> {
        self.items.iter().map(|i| i.def).collect()
    }

    /// `None` when `def` is not an item (a local, a variant, a prelude name). The checker
    /// treats that as an already-reported error, never a panic.
    pub fn item(&self, def: DefId) -> Option<&Item> {
        self.index.get(&def).map(|&i| &self.items[i])
    }
}
