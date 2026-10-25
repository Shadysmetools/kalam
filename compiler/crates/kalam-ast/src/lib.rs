//! The shared, **skin-free** abstract syntax tree of Kalam (spec §8.2).
//!
//! Every skin (`en`, `ar`, `franco`) parses to this one tree (DECISIONS D6, spec §4.3):
//! - keywords are structure, never text;
//! - core names (`Int`, `print`, `Some`, `main`, …) are stored as their [`Concept`];
//! - identifiers are stored as their exact NFC text, with raw-identifier backticks removed;
//! - numbers are stored ASCII-normalized (`١٢` → `12`, `٫` → `.`) plus their exact value;
//! - `،` and `؟` are just `,` and `?`, so they leave no trace;
//! - string and tagged-literal text is kept byte for byte, and each interpolation hole is a
//!   parsed [`Expr`] (holes are code and convert like code, spec §4.3/§4.4).
//!
//! Spans are byte ranges into the file, so they differ between skins (Arabic letters are
//! multi-byte). Structural comparison across skins therefore goes through [`shape`], a
//! span-free S-expression dump, never through `PartialEq` (which also compares spans).
//!
//! Trivia (comments, doc comments and blank lines) is kept in [`Ast::trivia`] with spans and
//! line numbers, so the formatter and the skin converter can re-attach it. The byte-exact
//! lossless tree is the CST that `kalam-parser` builds next to this AST.

use std::fmt;

use kalam_base::Span;
use kalam_skins::Concept;

pub mod shape;
pub mod stack;

pub use shape::{shape, ShapeOptions};

/// A parsed file: the tree plus its trivia.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ast {
    pub file: File,
    /// Comments, doc comments and blank lines in source order.
    pub trivia: Vec<Trivia>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TriviaKind {
    /// `# …`
    Comment,
    /// `## …` (attaches to the next item, spec §5.5)
    DocComment,
    /// An empty or space-only line outside any string literal.
    BlankLine,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Trivia {
    pub kind: TriviaKind,
    /// Exact source text (empty for blank lines). Comments are never translated (§5.5).
    pub text: String,
    /// 1-based line. Conversion between skins keeps line numbers.
    pub line: u32,
    pub span: Span,
}

// ---------------------------------------------------------------------------------------
// Names
// ---------------------------------------------------------------------------------------

/// An identifier: exact NFC text, backticks of a raw identifier removed (§5.3 rule 7).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ident {
    pub name: String,
    pub span: Span,
}

/// A name that may be either an identifier or a core name (`main`, `Int`, `World`, …).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Name {
    pub kind: NameKind,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NameKind {
    Ident(String),
    Core(Concept),
}

// ---------------------------------------------------------------------------------------
// File and items
// ---------------------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct File {
    pub uses: Vec<UseDecl>,
    pub items: Vec<Item>,
    pub span: Span,
}

/// `[export] use a/b [.{x, y as z, self} | as m]`
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UseDecl {
    pub export: bool,
    pub path: Vec<Ident>,
    pub tail: UseTail,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UseTail {
    None,
    Items(Vec<UseItem>),
    Alias(Ident),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UseItem {
    Name {
        name: Ident,
        alias: Option<Ident>,
    },
    /// `self`: binds the module name.
    SelfModule(Span),
}

/// `@a.b(args)`. Attribute names are fixed ASCII in every skin (§7.2).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Attribute {
    pub path: Vec<Ident>,
    pub args: Option<Vec<Arg>>,
    pub span: Span,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Visibility {
    Private,
    Export,
    ExportOpaque,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Item {
    pub attrs: Vec<Attribute>,
    pub vis: Visibility,
    pub kind: ItemKind,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ItemKind {
    Fn(FnDecl),
    Type(TypeDecl),
    Enum(EnumDecl),
    Trait(TraitDecl),
    Impl(ImplDecl),
    Const(ConstDecl),
    Extern(ExternDecl),
    Test(TestDecl),
    /// Unparseable item; the tokens are kept in the CST's `Error` node.
    Error,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GenericParam {
    pub name: Ident,
    pub bounds: Vec<Type>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Param {
    SelfParam(Span),
    Named {
        name: Ident,
        ty: Type,
        default: Option<Expr>,
        span: Span,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FnSig {
    pub name: Name,
    pub generics: Vec<GenericParam>,
    pub params: Vec<Param>,
    pub ret: Option<Type>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FnBody {
    Block(Block),
    /// `=> expr`
    Expr(Expr),
    /// Required trait member: signature only.
    None,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FnDecl {
    pub sig: FnSig,
    pub body: FnBody,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeDecl {
    pub name: Name,
    pub generics: Vec<GenericParam>,
    pub body: TypeBody,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TypeBody {
    Record(Vec<FieldDecl>),
    Alias(Type),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FieldDecl {
    pub attrs: Vec<Attribute>,
    pub name: Ident,
    pub ty: Type,
    pub default: Option<Expr>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EnumDecl {
    pub name: Name,
    pub generics: Vec<GenericParam>,
    pub variants: Vec<Variant>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Variant {
    pub attrs: Vec<Attribute>,
    pub name: Ident,
    /// `None` for a payload-free variant, `Some(vec![])` for `V()`.
    pub fields: Option<Vec<VariantField>>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VariantField {
    pub label: Option<Ident>,
    pub ty: Type,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TraitDecl {
    pub name: Name,
    pub generics: Vec<GenericParam>,
    pub members: Vec<FnDecl>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImplDecl {
    pub generics: Vec<GenericParam>,
    pub ty: Type,
    /// `impl T as Trait:`
    pub as_trait: Option<Type>,
    pub members: Vec<ImplMember>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImplMember {
    pub attrs: Vec<Attribute>,
    pub export: bool,
    pub func: FnDecl,
    pub span: Span,
}

/// Module-level `let NAME [: T] = expr`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConstDecl {
    pub name: Name,
    pub ty: Option<Type>,
    pub value: Expr,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExternDecl {
    Fn(FnSig),
    Type {
        name: Name,
        generics: Vec<GenericParam>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TestDecl {
    pub name: StrLit,
    pub body: Block,
}

// ---------------------------------------------------------------------------------------
// Blocks and statements
// ---------------------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Block {
    pub stmts: Vec<Stmt>,
    /// A one-line block (`: stmt` on the header line, DR-33).
    pub inline: bool,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StmtKind {
    /// An expression statement, including statement-level `if`/`match` forms.
    Expr(Expr),
    Let {
        pat: Pat,
        ty: Option<Type>,
        value: Expr,
    },
    Var {
        name: Ident,
        ty: Option<Type>,
        value: Expr,
    },
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
    /// A local function declaration.
    Fn(FnDecl),
    Error,
}

// ---------------------------------------------------------------------------------------
// Expressions
// ---------------------------------------------------------------------------------------

/// An expression. `Clone`, `PartialEq` and `Debug` are written by hand only to make them
/// stack-safe ([`stack::deeper`]): they behave exactly like the derived impls.
pub struct Expr {
    pub kind: ExprKind,
    pub span: Span,
}

impl Clone for Expr {
    fn clone(&self) -> Self {
        stack::deeper(|| Expr {
            kind: self.kind.clone(),
            span: self.span,
        })
    }
}

impl PartialEq for Expr {
    fn eq(&self, other: &Self) -> bool {
        stack::deeper(|| self.span == other.span && self.kind == other.kind)
    }
}

impl Eq for Expr {}

impl fmt::Debug for Expr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if stack::must_move() {
            // A `Formatter` cannot cross threads: print this subtree to a string on the big
            // stack, then write it here (inside `{:#?}` the caller's pad adapter indents it).
            let alternate = f.alternate();
            let fields = ExprFields(self);
            let text = stack::deeper(|| {
                if alternate {
                    format!("{fields:#?}")
                } else {
                    format!("{fields:?}")
                }
            });
            return f.write_str(&text);
        }
        stack::level(|| ExprFields(self).fmt(f))
    }
}

/// The derived-style `Debug` body of [`Expr`].
struct ExprFields<'a>(&'a Expr);

impl fmt::Debug for ExprFields<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Expr")
            .field("kind", &self.0.kind)
            .field("span", &self.0.span)
            .finish()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IntLit {
    /// ASCII-normalized literal text (digit glyphs converted, `_` kept).
    pub text: String,
    /// Exact value when it fits in `u128`; the type checker range-checks it (K0126).
    pub value: Option<u128>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FloatLit {
    /// ASCII-normalized literal text (`٫` → `.`).
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StrLit {
    pub parts: Vec<StrPart>,
    /// `"""…"""`
    pub triple: bool,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StrPart {
    /// Raw source text between holes: escapes are kept as written.
    Text(String),
    /// `{expr}`
    Hole(Expr),
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Arg {
    pub label: Option<Ident>,
    pub value: Expr,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Member {
    Name(Ident),
    /// `t.0` (spec §5.6 member rule); ASCII-normalized.
    Index {
        text: String,
        span: Span,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LambdaParam {
    /// `_` is kept as the identifier `_`.
    pub name: Ident,
    pub ty: Option<Type>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Lambda {
    pub params: Vec<LambdaParam>,
    pub ret: Option<Type>,
    pub body: Box<Expr>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IfBranch {
    pub cond: Expr,
    pub body: Block,
}

/// Both `if_form` (statement or multi-line value form) and `if_inline`.
/// For `if_inline` every arm is a one-line block holding one expression statement.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IfExpr {
    pub branches: Vec<IfBranch>,
    pub else_body: Option<Block>,
    pub inline: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CaseArm {
    pub pat: Pat,
    pub guard: Option<Expr>,
    pub body: Block,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MatchExpr {
    pub scrutinee: Box<Expr>,
    pub arms: Vec<CaseArm>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExprKind {
    Int(IntLit),
    Float(FloatLit),
    Str(StrLit),
    /// `TAG"…"`; tags are ASCII and identical in every skin.
    Tagged {
        tag: String,
        lit: StrLit,
    },
    Bool(bool),
    /// An identifier or core name (`print`, `Some`, `Int`, …).
    Name(NameKind),
    SelfValue,
    /// `.Circle`
    ImplicitMember(Ident),
    /// `unsafe.name`
    UnsafeMember(Ident),
    Unit,
    Paren(Box<Expr>),
    Tuple(Vec<Expr>),
    List(Vec<Expr>),
    Map(Vec<(Expr, Expr)>),
    Call {
        callee: Box<Expr>,
        args: Vec<Arg>,
    },
    Index {
        base: Box<Expr>,
        index: Box<Expr>,
    },
    Member {
        base: Box<Expr>,
        member: Member,
    },
    /// Postfix `?` (or `؟`).
    Try(Box<Expr>),
    Neg(Box<Expr>),
    Not(Box<Expr>),
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
    Lambda(Lambda),
    If(IfExpr),
    Match(MatchExpr),
    /// Unparseable expression (a diagnostic was reported).
    Error,
}

// ---------------------------------------------------------------------------------------
// Patterns
// ---------------------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pat {
    pub kind: PatKind,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IntPatBound {
    pub negative: bool,
    pub lit: IntLit,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PatField {
    pub label: Option<Ident>,
    pub pat: Pat,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PatFields {
    pub fields: Vec<PatField>,
    /// A trailing `..` (or `..` alone).
    pub rest: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ListPatElem {
    Pat(Pat),
    /// `..` or `..name`
    Rest(Option<Ident>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PatKind {
    Wild,
    /// A bare identifier always binds (§8.2 `prim_pat`).
    Bind(Ident),
    Int(IntPatBound),
    Range {
        lo: IntPatBound,
        hi: IntPatBound,
        inclusive: bool,
    },
    Str(StrLit),
    Bool(bool),
    /// `.Circle(r)` (implicit enum variant).
    Implicit {
        name: Ident,
        fields: Option<PatFields>,
    },
    /// `Shape.Circle(r)`, `Point(x: 1, ..)`, `a.B`: one production for variants and records
    /// (CRITIQUE P1.4); name resolution decides which it is.
    Path {
        path: Vec<Name>,
        fields: Option<PatFields>,
    },
    /// `Some(x)`, `None`, `Ok(v)`, `Err(e)`.
    Ctor {
        ctor: Concept,
        inner: Option<Box<Pat>>,
    },
    Tuple(Vec<Pat>),
    List(Vec<ListPatElem>),
    Or(Vec<Pat>),
    As {
        pat: Box<Pat>,
        name: Ident,
    },
    Error,
}

// ---------------------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Type {
    pub kind: TypeKind,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TypeKind {
    Named {
        path: Vec<Name>,
        args: Vec<Type>,
    },
    Unit,
    Tuple(Vec<Type>),
    Fn {
        /// `@sync fn(…) -> T`
        sync: bool,
        params: Vec<Type>,
        ret: Box<Type>,
    },
    Error,
}
