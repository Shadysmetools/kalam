//! A random **AST** generator (not a text generator): it builds `kalam_ast` trees directly,
//! which [`crate::print`] then writes out in a skin. Comparing the generated tree with what
//! the parser reads back is the P1 property: print → parse is the identity on ASTs.
//!
//! The generator only builds trees the grammar can express (spec §8.2): wherever operator
//! precedence (§10.6) requires parentheses it inserts an explicit [`ExprKind::Paren`], because
//! the parser keeps parentheses in the tree. The printer never adds any. Statement-level
//! blocks are built with `inline: false`; the printer decides their layout, and the
//! comparison goes through `kalam_fmt::normalized_shape`, which ignores that flag.

use std::cell::RefCell;
use std::collections::BTreeSet;

use kalam_ast::*;
use kalam_base::{Skin, Span};
use kalam_skins::{Concept, KeywordTable};

use crate::rng::Rng;

pub(crate) const SP: Span = Span { start: 0, end: 0 };

// Precedence levels (higher binds tighter). `expr` is level 0 (lambda, inline `if`).
pub(crate) const L_TOP: u8 = 0;
pub(crate) const L_OR: u8 = 1;
const L_AND: u8 = 2;
const L_NOT: u8 = 3;
const L_CMP: u8 = 4;
const L_RANGE: u8 = 5;
const L_ADD: u8 = 6;
const L_MUL: u8 = 7;
const L_NEG: u8 = 8;
const L_POST: u8 = 9;

pub(crate) const PLAIN: &[&str] = &[
    "x", "y", "total", "items", "count", "user", "name", "value", "n1", "_tmp", "قيمة", "اسم",
    "عداد",
];

/// Words that are reserved in at least one skin (keywords, core names, Franco variants,
/// Arabic fold variants, Persian/Urdu letter forms).
pub(crate) const CROSSY: &[&str] = &[
    "law",
    "walla",
    "fi",
    "lekol",
    "hasab",
    "lama",
    "khali",
    "dala",
    "ektb",
    "adad",
    "lw",
    "fe",
    "wala",
    "low",
    "sa7",
    "if",
    "for",
    "in",
    "match",
    "print",
    "let",
    "else",
    "None",
    "Some",
    "إذا",
    "اذا",
    "لكل",
    "في",
    "فى",
    "عدد",
    "اطبع",
    "و",
    "ليكن",
    "ل\u{06A9}ل",
    "ف\u{06CC}",
    "mafeesh",
];

const TYPES: &[&str] = &[
    "Shape", "User", "Point", "Box", "Item", "نوع", "adad", "Some",
];
const VARIANTS: &[&str] = &["Circle", "Empty", "Rect", "law", "Some", "حالة", "None"];
const GENERICS: &[&str] = &["T", "U", "عنصر"];
const MODULES: &[&str] = &["std", "http", "notes", "db", "نصوص"];
const TAGS: &[&str] = &["html", "sql", "url"];

const INTS: &[&str] = &[
    "0", "1", "2", "7", "15", "42", "100", "1000", "65535", "1_000", "0x1F",
];
const FLOATS: &[&str] = &["1.5", "0.25", "2e3", "3.0e-2", "10.0"];

/// String text pieces: no unescaped `{`, `}`, `"` or line break.
const TEXT: &[&str] = &[
    "hello",
    " ",
    "سلام عليكم",
    "a, b",
    "x?",
    "\\n",
    "\\t",
    "\\{",
    "\\}",
    "\\\"",
    "\\\\",
    "\\u{41}",
    "if",
    "law",
    ": ",
    "١٢٣",
    "؟،",
];

thread_local! {
    /// Identifiers built since the last [`Gen::program`] start that are reserved in some
    /// skin (they must be declared in the file, spec §5.4: an undefined one is K0201).
    static CROSS_SKIN: RefCell<BTreeSet<String>> = const { RefCell::new(BTreeSet::new()) };
}

fn note(name: &str) {
    let table = KeywordTable::builtin();
    if Skin::ALL.iter().any(|&s| table.skin(s).is_reserved(name)) {
        CROSS_SKIN.with(|c| c.borrow_mut().insert(name.to_string()));
    }
}

pub(crate) fn ident(name: &str) -> Ident {
    note(name);
    Ident {
        name: name.to_string(),
        span: SP,
    }
}

fn name_ident(name: &str) -> Name {
    note(name);
    Name {
        kind: NameKind::Ident(name.to_string()),
        span: SP,
    }
}

fn core_name(c: Concept) -> Name {
    Name {
        kind: NameKind::Core(c),
        span: SP,
    }
}

pub(crate) fn ex(kind: ExprKind) -> Expr {
    Expr { kind, span: SP }
}

fn st(kind: StmtKind) -> Stmt {
    Stmt { kind, span: SP }
}

fn pt(kind: PatKind) -> Pat {
    Pat { kind, span: SP }
}

fn ty(kind: TypeKind) -> Type {
    Type { kind, span: SP }
}

fn int_lit(text: &str) -> IntLit {
    IntLit {
        text: text.to_string(),
        value: kalam_lexer::int_literal_value(text),
    }
}

/// The precedence level of an expression as the printer writes it.
pub(crate) fn level(e: &Expr) -> u8 {
    match &e.kind {
        ExprKind::Lambda(_) | ExprKind::If(_) | ExprKind::Match(_) => L_TOP,
        ExprKind::Binary { op, .. } => match op {
            BinOp::Or => L_OR,
            BinOp::And => L_AND,
            BinOp::Eq | BinOp::NotEq | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => L_CMP,
            BinOp::Add | BinOp::Sub => L_ADD,
            BinOp::Mul | BinOp::Div | BinOp::IntDiv | BinOp::Rem => L_MUL,
        },
        ExprKind::Not(_) => L_NOT,
        ExprKind::Range { .. } => L_RANGE,
        ExprKind::Neg(_) => L_NEG,
        _ => L_POST,
    }
}

/// True if the printed expression starts with `.` (an implicit member such as `.Circle`).
fn starts_with_dot(e: &Expr) -> bool {
    match &e.kind {
        ExprKind::ImplicitMember(_) => true,
        ExprKind::Call { callee: b, .. }
        | ExprKind::Index { base: b, .. }
        | ExprKind::Member { base: b, .. }
        | ExprKind::Try(b) => starts_with_dot(b),
        ExprKind::Binary { lhs: b, .. } | ExprKind::Range { lo: b, .. } => starts_with_dot(b),
        _ => false,
    }
}

/// `e`, or `(e)` if it binds looser than `min`.
fn wrap(e: Expr, min: u8) -> Expr {
    if level(&e) < min {
        ex(ExprKind::Paren(Box::new(e)))
    } else {
        e
    }
}

/// Where an expression goes; restricts what it may contain.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Ctx {
    Code,
    /// Inside an interpolation hole: no strings, no braces, no `:`-forms.
    Hole,
}

/// Generator limits, scaled by a per-program size class so most programs are small and a
/// few are large.
pub struct Gen {
    rng: Rng,
    expr_depth: u32,
    block_depth: u32,
}

impl Gen {
    pub fn new(seed: u64) -> Gen {
        let mut rng = Rng::new(seed);
        let big = rng.chance(10);
        Gen {
            expr_depth: if big { 3 } else { 2 },
            block_depth: if big { 3 } else { 2 },
            rng,
        }
    }

    // ----- names ----------------------------------------------------------------------------

    fn word(&mut self) -> &'static str {
        let w = self.word_unnoted();
        note(w);
        w
    }

    fn word_unnoted(&mut self) -> &'static str {
        if self.rng.chance(30) {
            self.rng.pick(CROSSY)
        } else {
            self.rng.pick(PLAIN)
        }
    }

    fn var(&mut self) -> Ident {
        let w = self.word();
        ident(w)
    }

    fn type_name(&mut self) -> Name {
        let t = *self.rng.pick(TYPES);
        name_ident(t)
    }

    // ----- program --------------------------------------------------------------------------

    /// A random file. Every identifier that is reserved in some skin is declared by a
    /// leading `use std.{…}` (a file-local declaration, so it is never K0201 in any skin).
    pub fn program(&mut self) -> Ast {
        CROSS_SKIN.with(|c| c.borrow_mut().clear());
        let mut uses = Vec::new();
        if self.rng.chance(20) {
            for _ in 0..1 + self.rng.below(2) {
                uses.push(self.use_decl());
            }
        }
        let mut items = Vec::new();
        let n = 1 + self.rng.below(4);
        let fn_at = self.rng.below(n);
        for i in 0..n {
            let kind = if i == fn_at {
                ItemKind::Fn(self.fn_decl(true, self.block_depth))
            } else {
                match self.rng.below(10) {
                    0..=3 => ItemKind::Fn(self.fn_decl(true, self.block_depth)),
                    4 => ItemKind::Const(self.const_decl()),
                    5 => ItemKind::Type(self.record()),
                    6 => ItemKind::Type(TypeDecl {
                        name: self.type_name(),
                        generics: self.generics(),
                        body: TypeBody::Alias(self.ty(2)),
                    }),
                    7 => ItemKind::Enum(self.enum_decl()),
                    _ => ItemKind::Test(TestDecl {
                        name: self.plain_str(),
                        body: self.block(self.block_depth),
                    }),
                }
            };
            let vis = match &kind {
                ItemKind::Test(_) => Visibility::Private,
                ItemKind::Type(_) | ItemKind::Enum(_) if self.rng.chance(10) => {
                    Visibility::ExportOpaque
                }
                _ if self.rng.chance(15) => Visibility::Export,
                _ => Visibility::Private,
            };
            items.push(Item {
                attrs: Vec::new(),
                vis,
                kind,
                span: SP,
            });
        }
        let cross: Vec<String> = CROSS_SKIN
            .with(|c| std::mem::take(&mut *c.borrow_mut()))
            .into_iter()
            .collect();
        if !cross.is_empty() {
            uses.insert(
                0,
                UseDecl {
                    export: false,
                    path: vec![Ident {
                        name: "std".into(),
                        span: SP,
                    }],
                    tail: UseTail::Items(
                        cross
                            .into_iter()
                            .map(|name| UseItem::Name {
                                name: Ident { name, span: SP },
                                alias: None,
                            })
                            .collect(),
                    ),
                    span: SP,
                },
            );
        }
        Ast {
            file: File {
                uses,
                items,
                span: SP,
            },
            trivia: Vec::new(),
        }
    }

    fn use_decl(&mut self) -> UseDecl {
        let path: Vec<Ident> = (0..1 + self.rng.below(2))
            .map(|_| ident(self.rng.pick(MODULES)))
            .collect();
        let tail = match self.rng.below(3) {
            0 => UseTail::None,
            1 => UseTail::Alias(ident(self.rng.pick(PLAIN))),
            _ => UseTail::Items(
                (0..1 + self.rng.below(3))
                    .map(|_| {
                        if self.rng.chance(15) {
                            UseItem::SelfModule(SP)
                        } else {
                            UseItem::Name {
                                name: ident(self.rng.pick(PLAIN)),
                                alias: if self.rng.chance(30) {
                                    Some(ident(self.rng.pick(PLAIN)))
                                } else {
                                    None
                                },
                            }
                        }
                    })
                    .collect(),
            ),
        };
        UseDecl {
            export: false,
            path,
            tail,
            span: SP,
        }
    }

    fn generics(&mut self) -> Vec<GenericParam> {
        if !self.rng.chance(20) {
            return Vec::new();
        }
        (0..1 + self.rng.below(2))
            .map(|_| GenericParam {
                name: ident(self.rng.pick(GENERICS)),
                bounds: (0..self.rng.below(3))
                    .map(|_| {
                        ty(TypeKind::Named {
                            path: vec![name_ident(self.rng.pick(&["Show", "Eq", "Ord"]))],
                            args: Vec::new(),
                        })
                    })
                    .collect(),
                span: SP,
            })
            .collect()
    }

    fn fn_decl(&mut self, top: bool, depth: u32) -> FnDecl {
        let name = if top && self.rng.chance(15) {
            core_name(Concept::CoreMain)
        } else {
            let w = self.word();
            name_ident(w)
        };
        let params = (0..self.rng.below(4))
            .map(|_| Param::Named {
                name: self.var(),
                ty: self.ty(2),
                default: if self.rng.chance(20) {
                    Some(self.leaf(Ctx::Code))
                } else {
                    None
                },
                span: SP,
            })
            .collect();
        let sig = FnSig {
            name,
            generics: self.generics(),
            params,
            ret: if self.rng.chance(40) {
                Some(self.ty(2))
            } else {
                None
            },
            span: SP,
        };
        let body = if self.rng.chance(15) {
            FnBody::Expr(self.expr(self.expr_depth, L_TOP, Ctx::Code))
        } else {
            FnBody::Block(self.block(depth))
        };
        FnDecl {
            sig,
            body,
            span: SP,
        }
    }

    fn const_decl(&mut self) -> ConstDecl {
        let w = self.word();
        ConstDecl {
            name: name_ident(w),
            ty: if self.rng.chance(40) {
                Some(self.ty(2))
            } else {
                None
            },
            value: self.expr(self.expr_depth, L_TOP, Ctx::Code),
        }
    }

    fn record(&mut self) -> TypeDecl {
        let fields = (0..1 + self.rng.below(3))
            .map(|_| FieldDecl {
                attrs: Vec::new(),
                name: self.var(),
                ty: self.ty(2),
                default: if self.rng.chance(25) {
                    Some(self.expr(2, L_TOP, Ctx::Code))
                } else {
                    None
                },
                span: SP,
            })
            .collect();
        TypeDecl {
            name: self.type_name(),
            generics: self.generics(),
            body: TypeBody::Record(fields),
        }
    }

    fn enum_decl(&mut self) -> EnumDecl {
        let variants = (0..1 + self.rng.below(3))
            .map(|_| Variant {
                attrs: Vec::new(),
                name: ident(self.rng.pick(VARIANTS)),
                fields: if self.rng.chance(50) {
                    Some(
                        (0..self.rng.below(3))
                            .map(|_| VariantField {
                                label: if self.rng.chance(40) {
                                    Some(self.var())
                                } else {
                                    None
                                },
                                ty: self.ty(2),
                                span: SP,
                            })
                            .collect(),
                    )
                } else {
                    None
                },
                span: SP,
            })
            .collect();
        EnumDecl {
            name: self.type_name(),
            generics: self.generics(),
            variants,
        }
    }

    // ----- types ----------------------------------------------------------------------------

    fn ty(&mut self, depth: u32) -> Type {
        if depth == 0 || self.rng.chance(50) {
            return match self.rng.below(6) {
                0 => ty(TypeKind::Named {
                    path: vec![self.type_name()],
                    args: Vec::new(),
                }),
                1 if self.rng.chance(30) => ty(TypeKind::Unit),
                _ => ty(TypeKind::Named {
                    path: vec![core_name(*self.rng.pick(&[
                        Concept::CoreInt,
                        Concept::CoreFloat,
                        Concept::CoreString,
                        Concept::CoreBool,
                        Concept::CoreDecimal,
                    ]))],
                    args: Vec::new(),
                }),
            };
        }
        match self.rng.below(6) {
            0 => ty(TypeKind::Tuple(
                (0..2 + self.rng.below(2))
                    .map(|_| self.ty(depth - 1))
                    .collect(),
            )),
            1 => ty(TypeKind::Fn {
                sync: self.rng.chance(15),
                params: (0..self.rng.below(3)).map(|_| self.ty(depth - 1)).collect(),
                ret: Box::new(self.ty(depth - 1)),
            }),
            2 => ty(TypeKind::Named {
                path: vec![core_name(Concept::CoreMap)],
                args: vec![self.ty(depth - 1), self.ty(depth - 1)],
            }),
            3 => ty(TypeKind::Named {
                path: vec![core_name(Concept::CoreResult)],
                args: vec![self.ty(depth - 1), self.ty(depth - 1)],
            }),
            4 => ty(TypeKind::Named {
                path: vec![self.type_name()],
                args: vec![self.ty(depth - 1)],
            }),
            _ => ty(TypeKind::Named {
                path: vec![core_name(*self.rng.pick(&[
                    Concept::CoreList,
                    Concept::CoreOption,
                    Concept::CoreSet,
                ]))],
                args: vec![self.ty(depth - 1)],
            }),
        }
    }

    // ----- blocks and statements ------------------------------------------------------------

    fn block(&mut self, depth: u32) -> Block {
        let n = 1 + self.rng.below(if depth > 1 { 3 } else { 4 });
        Block {
            stmts: (0..n).map(|_| self.stmt(depth)).collect(),
            inline: false,
            span: SP,
        }
    }

    fn stmt(&mut self, depth: u32) -> Stmt {
        let nested = depth > 0;
        let e = self.expr_depth;
        match self.rng.below(if nested { 16 } else { 9 }) {
            0 | 1 => st(StmtKind::Expr(self.call_stmt())),
            2 => {
                let value = self.rhs(depth);
                st(StmtKind::Let {
                    pat: self.simple_pat(),
                    ty: if self.rng.chance(25) {
                        Some(self.ty(2))
                    } else {
                        None
                    },
                    value,
                })
            }
            3 => {
                let value = self.rhs(depth);
                st(StmtKind::Var {
                    name: self.var(),
                    ty: if self.rng.chance(25) {
                        Some(self.ty(2))
                    } else {
                        None
                    },
                    value,
                })
            }
            4 => {
                let value = self.rhs(depth);
                st(StmtKind::Assign {
                    place: self.place(),
                    op: *self.rng.pick(&[
                        AssignOp::Assign,
                        AssignOp::Add,
                        AssignOp::Sub,
                        AssignOp::Mul,
                        AssignOp::Div,
                    ]),
                    value,
                })
            }
            5 => st(StmtKind::Return(if self.rng.chance(20) {
                None
            } else {
                Some(self.rhs(depth))
            })),
            6 => st(match self.rng.below(3) {
                0 => StmtKind::Break,
                1 => StmtKind::Continue,
                _ => StmtKind::Pass,
            }),
            7 => st(StmtKind::Expr(self.call_stmt())),
            8 => {
                let value = self.expr(e, L_TOP, Ctx::Code);
                st(StmtKind::Let {
                    pat: self.simple_pat(),
                    ty: None,
                    value,
                })
            }
            9 | 10 => st(StmtKind::Expr(self.if_form(depth - 1, false))),
            11 => st(StmtKind::Expr(self.match_form(depth - 1))),
            12 => st(StmtKind::For {
                pat: self.simple_pat(),
                iter: self.expr(e, L_OR, Ctx::Code),
                body: self.block(depth - 1),
            }),
            13 => st(StmtKind::While {
                cond: self.expr(e, L_OR, Ctx::Code),
                body: self.block(depth - 1),
            }),
            14 => st(StmtKind::Fn(self.fn_decl(false, depth - 1))),
            _ => st(StmtKind::Expr(self.call_stmt())),
        }
    }

    /// The value of `let`/`var`/assignment/`return`: an expression, or (rarely) a multi-line
    /// `if`/`match` value form.
    fn rhs(&mut self, depth: u32) -> Expr {
        if depth > 0 && self.rng.chance(10) {
            if self.rng.chance(50) {
                self.if_form(depth - 1, true)
            } else {
                self.match_form(depth - 1)
            }
        } else {
            self.expr(self.expr_depth, L_TOP, Ctx::Code)
        }
    }

    /// Statement-level `if`; a value form always has an `else` (K0731).
    fn if_form(&mut self, depth: u32, value: bool) -> Expr {
        let branches = (0..1 + self.rng.below(3))
            .map(|_| IfBranch {
                cond: self.expr(self.expr_depth, L_OR, Ctx::Code),
                body: self.block(depth),
            })
            .collect();
        let else_body = if value || self.rng.chance(50) {
            Some(self.block(depth))
        } else {
            None
        };
        ex(ExprKind::If(IfExpr {
            branches,
            else_body,
            inline: false,
        }))
    }

    fn match_form(&mut self, depth: u32) -> Expr {
        let arms = (0..1 + self.rng.below(3))
            .map(|_| CaseArm {
                pat: self.pat(2),
                guard: if self.rng.chance(20) {
                    Some(self.expr(2, L_OR, Ctx::Code))
                } else {
                    None
                },
                body: self.block(depth),
                span: SP,
            })
            .collect();
        ex(ExprKind::Match(MatchExpr {
            scrutinee: Box::new(self.expr(self.expr_depth, L_OR, Ctx::Code)),
            arms,
        }))
    }

    /// An expression statement: always a call whose first token is a name, so the line can
    /// never start with `.` (a continuation, spec §5.8), `fn (` or `if`.
    fn call_stmt(&mut self) -> Expr {
        let mut callee = if self.rng.chance(25) {
            ex(ExprKind::Name(NameKind::Core(*self.rng.pick(&[
                Concept::CorePrint,
                Concept::CoreAssert,
                Concept::CorePanic,
            ]))))
        } else {
            ex(ExprKind::Name(NameKind::Ident(self.word().to_string())))
        };
        for _ in 0..self.rng.below(3) {
            callee = if self.rng.chance(50) {
                ex(ExprKind::Member {
                    base: Box::new(callee),
                    member: Member::Name(self.var()),
                })
            } else {
                ex(ExprKind::Call {
                    callee: Box::new(callee),
                    args: self.args(1, Ctx::Code),
                })
            };
        }
        ex(ExprKind::Call {
            callee: Box::new(callee),
            args: self.args(self.expr_depth - 1, Ctx::Code),
        })
    }

    /// `IDENT { . member | [ expr ] }`
    fn place(&mut self) -> Expr {
        let mut p = ex(ExprKind::Name(NameKind::Ident(self.word().to_string())));
        for _ in 0..self.rng.below(3) {
            p = if self.rng.chance(60) {
                ex(ExprKind::Member {
                    base: Box::new(p),
                    member: Member::Name(self.var()),
                })
            } else {
                ex(ExprKind::Index {
                    base: Box::new(p),
                    index: Box::new(self.expr(1, L_TOP, Ctx::Code)),
                })
            };
        }
        p
    }

    // ----- expressions ----------------------------------------------------------------------

    fn plain_str(&mut self) -> StrLit {
        let n = self.rng.below(3);
        let mut text = String::new();
        for _ in 0..n {
            text.push_str(self.rng.pick(TEXT));
        }
        StrLit {
            parts: if text.is_empty() {
                Vec::new()
            } else {
                vec![StrPart::Text(text)]
            },
            triple: false,
            span: SP,
        }
    }

    fn str_lit(&mut self, depth: u32) -> StrLit {
        let mut parts: Vec<StrPart> = Vec::new();
        for _ in 0..self.rng.below(5) {
            if depth > 0 && self.rng.chance(35) {
                let e = self.expr(depth - 1, L_OR, Ctx::Hole);
                parts.push(StrPart::Hole(e));
            } else {
                let t = self.rng.pick(TEXT).to_string();
                match parts.last_mut() {
                    Some(StrPart::Text(prev)) => prev.push_str(&t),
                    _ => parts.push(StrPart::Text(t)),
                }
            }
        }
        StrLit {
            parts,
            triple: false,
            span: SP,
        }
    }

    fn leaf(&mut self, ctx: Ctx) -> Expr {
        match self.rng.below(12) {
            0 | 1 => ex(ExprKind::Int(int_lit(self.rng.pick(INTS)))),
            2 => ex(ExprKind::Float(FloatLit {
                text: self.rng.pick(FLOATS).to_string(),
            })),
            3 if ctx == Ctx::Code => {
                let d = self.expr_depth.min(2);
                ex(ExprKind::Str(self.str_lit(d)))
            }
            4 => ex(ExprKind::Bool(self.rng.chance(50))),
            5 => ex(ExprKind::Name(NameKind::Core(*self.rng.pick(&[
                Concept::CorePrint,
                Concept::CoreNone,
                Concept::CoreSome,
                Concept::CoreOk,
                Concept::CoreErr,
                Concept::CoreInt,
                Concept::CoreMain,
                Concept::CoreWorld,
            ])))),
            6 => ex(ExprKind::ImplicitMember(ident(self.rng.pick(VARIANTS)))),
            7 if self.rng.chance(30) => ex(ExprKind::Unit),
            8 if self.rng.chance(30) => ex(ExprKind::SelfValue),
            9 if ctx == Ctx::Code && self.rng.chance(30) => ex(ExprKind::Tagged {
                tag: self.rng.pick(TAGS).to_string(),
                lit: self.str_lit(1),
            }),
            _ => ex(ExprKind::Name(NameKind::Ident(self.word().to_string()))),
        }
    }

    fn args(&mut self, depth: u32, ctx: Ctx) -> Vec<Arg> {
        let n = self.rng.below(4);
        let named_from = self.rng.below(n + 1);
        (0..n)
            .map(|i| Arg {
                label: if i >= named_from {
                    Some(self.var())
                } else {
                    None
                },
                value: self.expr(depth, if ctx == Ctx::Hole { L_OR } else { L_TOP }, ctx),
                span: SP,
            })
            .collect()
    }

    /// An expression of level `>= min` (wrapped in parentheses if needed).
    fn expr(&mut self, depth: u32, min: u8, ctx: Ctx) -> Expr {
        let e = self.expr_any(depth, ctx);
        wrap(e, min)
    }

    fn postfix_base(&mut self, depth: u32, ctx: Ctx) -> Expr {
        let b = self.expr(depth, L_POST, ctx);
        match b.kind {
            // `1.x` and `1.5.x` would lex as number literals.
            ExprKind::Int(_) | ExprKind::Float(_) => ex(ExprKind::Paren(Box::new(b))),
            _ => b,
        }
    }

    fn expr_any(&mut self, depth: u32, ctx: Ctx) -> Expr {
        if depth == 0 || self.rng.chance(25) {
            return self.leaf(ctx);
        }
        let d = depth - 1;
        let code = ctx == Ctx::Code;
        match self.rng.below(22) {
            0 | 1 => {
                let op = *self.rng.pick(&[
                    BinOp::Add,
                    BinOp::Sub,
                    BinOp::Mul,
                    BinOp::Div,
                    BinOp::IntDiv,
                    BinOp::Rem,
                ]);
                let (l, r) = match op {
                    BinOp::Add | BinOp::Sub => (L_ADD, L_MUL),
                    _ => (L_MUL, L_NEG),
                };
                ex(ExprKind::Binary {
                    op,
                    lhs: Box::new(self.expr(d, l, ctx)),
                    rhs: Box::new(self.expr(d, r, ctx)),
                })
            }
            2 => {
                let op = *self.rng.pick(&[
                    BinOp::Eq,
                    BinOp::NotEq,
                    BinOp::Lt,
                    BinOp::Le,
                    BinOp::Gt,
                    BinOp::Ge,
                ]);
                ex(ExprKind::Binary {
                    op,
                    lhs: Box::new(self.expr(d, L_RANGE, ctx)),
                    rhs: Box::new(self.expr(d, L_RANGE, ctx)),
                })
            }
            3 => {
                let (op, l, r) = if self.rng.chance(50) {
                    (BinOp::And, L_AND, L_NOT)
                } else {
                    (BinOp::Or, L_OR, L_AND)
                };
                ex(ExprKind::Binary {
                    op,
                    lhs: Box::new(self.expr(d, l, ctx)),
                    rhs: Box::new(self.expr(d, r, ctx)),
                })
            }
            4 => ex(ExprKind::Not(Box::new(self.expr(d, L_NOT, ctx)))),
            5 => ex(ExprKind::Neg(Box::new(self.expr(d, L_POST, ctx)))),
            6 => {
                let lo = self.range_operand(d, ctx);
                let hi = self.range_operand(d, ctx);
                ex(ExprKind::Range {
                    lo: Box::new(lo),
                    hi: Box::new(hi),
                    inclusive: self.rng.chance(50),
                })
            }
            7 | 8 => ex(ExprKind::Call {
                callee: Box::new(self.postfix_base(d, ctx)),
                args: self.args(d, ctx),
            }),
            9 => ex(ExprKind::Index {
                base: Box::new(self.postfix_base(d, ctx)),
                index: Box::new(self.expr(d, if code { L_TOP } else { L_OR }, ctx)),
            }),
            10 | 11 => {
                let base = self.postfix_base(d, ctx);
                let member = if self.rng.chance(20) {
                    Member::Index {
                        text: self.rng.pick(&["0", "1", "2"]).to_string(),
                        span: SP,
                    }
                } else {
                    Member::Name(self.var())
                };
                // `t.0.1` would lex `0.1` as a float.
                let base = match (&base.kind, &member) {
                    (
                        ExprKind::Member {
                            member: Member::Index { .. },
                            ..
                        },
                        _,
                    ) => ex(ExprKind::Paren(Box::new(base))),
                    _ => base,
                };
                ex(ExprKind::Member {
                    base: Box::new(base),
                    member,
                })
            }
            12 => ex(ExprKind::Try(Box::new(self.postfix_base(d, ctx)))),
            13 => ex(ExprKind::List(
                (0..self.rng.below(4))
                    .map(|_| self.expr(d, if code { L_TOP } else { L_OR }, ctx))
                    .collect(),
            )),
            14 => {
                let n = self.rng.below(4);
                if n == 0 {
                    ex(ExprKind::Unit)
                } else {
                    ex(ExprKind::Tuple(
                        (0..n)
                            .map(|_| self.expr(d, if code { L_TOP } else { L_OR }, ctx))
                            .collect(),
                    ))
                }
            }
            15 => ex(ExprKind::Paren(Box::new(self.expr(
                d,
                if code { L_TOP } else { L_OR },
                ctx,
            )))),
            16 if code => ex(ExprKind::Map(
                (0..self.rng.below(3))
                    .map(|_| {
                        let k = if self.rng.chance(50) {
                            ex(ExprKind::Int(int_lit(self.rng.pick(INTS))))
                        } else {
                            ex(ExprKind::Str(self.plain_str()))
                        };
                        (k, self.expr(d, L_TOP, ctx))
                    })
                    .collect(),
            )),
            17 if code => {
                let params = (0..self.rng.below(3))
                    .map(|_| LambdaParam {
                        name: if self.rng.chance(15) {
                            ident("_")
                        } else {
                            self.var()
                        },
                        ty: if self.rng.chance(30) {
                            Some(self.ty(1))
                        } else {
                            None
                        },
                    })
                    .collect();
                ex(ExprKind::Lambda(Lambda {
                    params,
                    ret: if self.rng.chance(20) {
                        Some(self.ty(1))
                    } else {
                        None
                    },
                    body: Box::new(self.expr(d, L_TOP, ctx)),
                }))
            }
            18 if code => {
                let line = |e: Expr| Block {
                    stmts: vec![st(StmtKind::Expr(e))],
                    inline: true,
                    span: SP,
                };
                let branches = (0..1 + self.rng.below(2))
                    .map(|_| IfBranch {
                        cond: self.expr(d, L_OR, ctx),
                        body: line(self.expr(d, L_OR, ctx)),
                    })
                    .collect();
                let else_body = Some(line(self.expr(d, L_OR, ctx)));
                ex(ExprKind::If(IfExpr {
                    branches,
                    else_body,
                    inline: true,
                }))
            }
            _ => self.leaf(ctx),
        }
    }

    /// A `..` operand: an additive expression, never a float literal (`1.5..2`).
    /// (`lo...Circle` would lex `...`).
    fn range_operand(&mut self, depth: u32, ctx: Ctx) -> Expr {
        let e = self.expr(depth, L_ADD, ctx);
        if matches!(e.kind, ExprKind::Float(_)) || starts_with_dot(&e) {
            ex(ExprKind::Paren(Box::new(e)))
        } else {
            e
        }
    }

    // ----- patterns -------------------------------------------------------------------------

    /// Patterns for `let` and `for`: a binding, `_`, or a tuple of those.
    fn simple_pat(&mut self) -> Pat {
        match self.rng.below(10) {
            0 => pt(PatKind::Wild),
            1 => pt(PatKind::Tuple(
                (0..2 + self.rng.below(2))
                    .map(|_| {
                        if self.rng.chance(20) {
                            pt(PatKind::Wild)
                        } else {
                            pt(PatKind::Bind(self.var()))
                        }
                    })
                    .collect(),
            )),
            _ => pt(PatKind::Bind(self.var())),
        }
    }

    fn int_bound(&mut self) -> IntPatBound {
        IntPatBound {
            negative: self.rng.chance(25),
            lit: int_lit(self.rng.pick(INTS)),
        }
    }

    fn pat_fields(&mut self, depth: u32) -> PatFields {
        let n = self.rng.below(3);
        PatFields {
            fields: (0..n)
                .map(|_| PatField {
                    label: if self.rng.chance(40) {
                        Some(self.var())
                    } else {
                        None
                    },
                    pat: self.pat(depth),
                })
                .collect(),
            rest: n == 0 || self.rng.chance(20),
        }
    }

    /// `pattern = bind_pat { "|" bind_pat }`
    fn pat(&mut self, depth: u32) -> Pat {
        if depth > 0 && self.rng.chance(15) {
            return pt(PatKind::Or(
                (0..2 + self.rng.below(2))
                    .map(|_| self.bind_pat(depth - 1))
                    .collect(),
            ));
        }
        self.bind_pat(depth)
    }

    fn bind_pat(&mut self, depth: u32) -> Pat {
        let p = self.prim_pat(depth);
        if self.rng.chance(10) {
            pt(PatKind::As {
                pat: Box::new(p),
                name: self.var(),
            })
        } else {
            p
        }
    }

    fn prim_pat(&mut self, depth: u32) -> Pat {
        let d = depth.saturating_sub(1);
        let deep = depth > 0;
        match self.rng.below(if deep { 14 } else { 8 }) {
            0 => pt(PatKind::Wild),
            1 | 2 => pt(PatKind::Bind(self.var())),
            3 => pt(PatKind::Int(self.int_bound())),
            4 => pt(PatKind::Range {
                lo: self.int_bound(),
                hi: self.int_bound(),
                inclusive: self.rng.chance(50),
            }),
            5 => pt(PatKind::Str(self.plain_str())),
            6 => pt(PatKind::Bool(self.rng.chance(50))),
            7 => pt(PatKind::Ctor {
                ctor: Concept::CoreNone,
                inner: None,
            }),
            8 => pt(PatKind::Implicit {
                name: ident(self.rng.pick(VARIANTS)),
                fields: if self.rng.chance(50) {
                    Some(self.pat_fields(d))
                } else {
                    None
                },
            }),
            9 => {
                // `T.V`, `T.V(…)` or the record form `T(…)`.
                let t = self.type_name();
                if self.rng.chance(30) {
                    pt(PatKind::Path {
                        path: vec![t],
                        fields: Some(self.pat_fields(d)),
                    })
                } else {
                    let v = name_ident(self.rng.pick(VARIANTS));
                    pt(PatKind::Path {
                        path: vec![t, v],
                        fields: if self.rng.chance(50) {
                            Some(self.pat_fields(d))
                        } else {
                            None
                        },
                    })
                }
            }
            10 => pt(PatKind::Ctor {
                ctor: *self
                    .rng
                    .pick(&[Concept::CoreSome, Concept::CoreOk, Concept::CoreErr]),
                inner: Some(Box::new(self.pat(d))),
            }),
            11 => pt(PatKind::Tuple(
                (0..2 + self.rng.below(2)).map(|_| self.pat(d)).collect(),
            )),
            12 => {
                let mut elems: Vec<ListPatElem> = (0..self.rng.below(3))
                    .map(|_| ListPatElem::Pat(self.pat(d)))
                    .collect();
                if self.rng.chance(50) {
                    let at = self.rng.below(elems.len() + 1);
                    let rest = if self.rng.chance(50) {
                        Some(self.var())
                    } else {
                        None
                    };
                    elems.insert(at, ListPatElem::Rest(rest));
                }
                pt(PatKind::List(elems))
            }
            _ => pt(PatKind::Bind(self.var())),
        }
    }
}
