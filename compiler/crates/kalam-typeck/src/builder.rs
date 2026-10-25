//! A small builder for hand-written HIR programs: unit tests here and tests of later
//! passes (lowering, the reference interpreter) that do not start from source text.
//!
//! Every expression gets a fresh [`ExprId`] and a distinct one-byte span, so tests can
//! tell nodes apart by span.

use kalam_base::{FileId, Span};
use kalam_resolve::{DefId, ModulePath};

use crate::hir::*;

pub struct Builder {
    pub module: ModulePath,
    pub file: FileId,
    next_def: u32,
    next_expr: u32,
    locals: Vec<LocalDecl>,
    pub program: HirProgram,
}

impl Builder {
    pub fn new(module: &str) -> Builder {
        Builder {
            module: ModulePath(module.to_string()),
            file: FileId(0),
            next_def: 0,
            next_expr: 0,
            locals: Vec::new(),
            program: HirProgram::new(),
        }
    }

    /// A fresh definition id.
    pub fn def(&mut self) -> DefId {
        self.next_def += 1;
        DefId(self.next_def - 1)
    }

    fn span(&mut self) -> Span {
        let n = self.next_expr * 2;
        Span::new(n, n + 1)
    }

    pub fn e(&mut self, kind: ExprKind) -> Expr {
        let span = self.span();
        self.next_expr += 1;
        Expr {
            id: ExprId(self.next_expr - 1),
            kind,
            span,
        }
    }

    // -- expressions ----------------------------------------------------------------------

    pub fn int(&mut self, v: u128) -> Expr {
        self.e(ExprKind::Int(Some(v)))
    }

    pub fn float(&mut self, text: &str) -> Expr {
        self.e(ExprKind::Float(text.into()))
    }

    pub fn str(&mut self, s: &str) -> Expr {
        self.e(ExprKind::Str(s.into()))
    }

    pub fn bool(&mut self, b: bool) -> Expr {
        self.e(ExprKind::Bool(b))
    }

    pub fn lang(&mut self, item: LangItem) -> Expr {
        self.e(ExprKind::Lang(item))
    }

    pub fn def_ref(&mut self, d: DefId) -> Expr {
        self.e(ExprKind::Def(d))
    }

    pub fn local_ref(&mut self, l: LocalId) -> Expr {
        self.e(ExprKind::Local(l))
    }

    pub fn implicit(&mut self, name: &str) -> Expr {
        self.e(ExprKind::ImplicitMember(name.into()))
    }

    pub fn neg(&mut self, operand: Expr) -> Expr {
        self.e(ExprKind::Unary {
            op: UnOp::Neg,
            operand: Box::new(operand),
        })
    }

    pub fn not(&mut self, operand: Expr) -> Expr {
        self.e(ExprKind::Unary {
            op: UnOp::Not,
            operand: Box::new(operand),
        })
    }

    pub fn bin(&mut self, op: BinOp, lhs: Expr, rhs: Expr) -> Expr {
        self.e(ExprKind::Binary {
            op,
            lhs: Box::new(lhs),
            rhs: Box::new(rhs),
        })
    }

    pub fn range(&mut self, lo: Expr, hi: Expr, inclusive: bool) -> Expr {
        self.e(ExprKind::Range {
            lo: Box::new(lo),
            hi: Box::new(hi),
            inclusive,
        })
    }

    pub fn list(&mut self, items: Vec<Expr>) -> Expr {
        self.e(ExprKind::List(items))
    }

    pub fn tuple(&mut self, items: Vec<Expr>) -> Expr {
        self.e(ExprKind::Tuple(items))
    }

    pub fn field(&mut self, base: Expr, name: &str) -> Expr {
        self.e(ExprKind::Field {
            base: Box::new(base),
            name: name.into(),
        })
    }

    pub fn index(&mut self, base: Expr, index: Expr) -> Expr {
        self.e(ExprKind::Index {
            base: Box::new(base),
            index: Box::new(index),
        })
    }

    /// `"text {hole} text"`
    pub fn interp(&mut self, parts: Vec<StrPart>) -> Expr {
        self.e(ExprKind::Interp(parts))
    }

    fn args(args: Vec<(Option<&str>, Expr)>) -> Vec<Arg> {
        args.into_iter()
            .map(|(label, value)| Arg {
                label: label.map(str::to_string),
                span: value.span,
                value,
            })
            .collect()
    }

    pub fn call(&mut self, callee: Expr, args: Vec<Expr>) -> Expr {
        let args = Self::args(args.into_iter().map(|a| (None, a)).collect());
        self.e(ExprKind::Call {
            callee: Box::new(callee),
            args,
        })
    }

    pub fn call_named(&mut self, callee: Expr, args: Vec<(&str, Expr)>) -> Expr {
        let args = Self::args(args.into_iter().map(|(l, a)| (Some(l), a)).collect());
        self.e(ExprKind::Call {
            callee: Box::new(callee),
            args,
        })
    }

    pub fn method_call(&mut self, receiver: Expr, name: &str, args: Vec<Expr>) -> Expr {
        let args = Self::args(args.into_iter().map(|a| (None, a)).collect());
        self.e(ExprKind::MethodCall {
            receiver: Box::new(receiver),
            name: name.into(),
            args,
        })
    }

    /// `print(arg)`
    pub fn print(&mut self, arg: Expr) -> Stmt {
        let p = self.lang(LangItem::Print);
        let call = self.call(p, vec![arg]);
        self.expr_stmt(call)
    }

    pub fn if_expr(&mut self, branches: Vec<(Expr, Block)>, else_body: Option<Block>) -> Expr {
        let branches = branches
            .into_iter()
            .map(|(cond, body)| IfBranch { cond, body })
            .collect();
        self.e(ExprKind::If {
            branches,
            else_body,
        })
    }

    pub fn match_expr(&mut self, scrutinee: Expr, arms: Vec<(Pat, Block)>) -> Expr {
        let arms = arms
            .into_iter()
            .map(|(pat, body)| Arm {
                span: pat.span,
                pat,
                guard: None,
                body,
                indent: None,
            })
            .collect();
        self.e(ExprKind::Match {
            scrutinee: Box::new(scrutinee),
            arms,
        })
    }

    // -- patterns -------------------------------------------------------------------------

    pub fn pat(&mut self, kind: PatKind) -> Pat {
        let span = self.span();
        self.next_expr += 1;
        Pat { kind, span }
    }

    pub fn pat_int(&mut self, v: u128) -> Pat {
        self.pat(PatKind::Int {
            negative: false,
            value: v,
        })
    }

    pub fn pat_bind(&mut self, l: LocalId) -> Pat {
        self.pat(PatKind::Bind(l))
    }

    pub fn pat_wild(&mut self) -> Pat {
        self.pat(PatKind::Wild)
    }

    // -- statements and blocks ------------------------------------------------------------

    pub fn stmt(&mut self, kind: StmtKind) -> Stmt {
        let span = self.span();
        self.next_expr += 1;
        Stmt {
            kind,
            span,
            indent: None,
        }
    }

    pub fn expr_stmt(&mut self, e: Expr) -> Stmt {
        self.stmt(StmtKind::Expr(e))
    }

    pub fn pass_stmt(&mut self) -> Stmt {
        self.stmt(StmtKind::Pass)
    }

    pub fn ret(&mut self, e: Option<Expr>) -> Stmt {
        self.stmt(StmtKind::Return(e))
    }

    /// `let name [: ty] = value`; returns the statement and the new local.
    pub fn let_(&mut self, name: &str, ty: Option<Type>, value: Expr) -> (Stmt, LocalId) {
        let l = self.local(name, false);
        let pat = self.pat_bind(l);
        (self.stmt(StmtKind::Let { pat, ty, value }), l)
    }

    /// `var name [: ty] = value`
    pub fn var(&mut self, name: &str, ty: Option<Type>, value: Expr) -> (Stmt, LocalId) {
        let l = self.local(name, true);
        (
            self.stmt(StmtKind::Var {
                local: l,
                ty,
                value,
            }),
            l,
        )
    }

    pub fn assign(&mut self, place: Expr, op: AssignOp, value: Expr) -> Stmt {
        self.stmt(StmtKind::Assign { place, op, value })
    }

    pub fn block(&mut self, stmts: Vec<Stmt>) -> Block {
        let span = self.span();
        self.next_expr += 1;
        Block { stmts, span }
    }

    // -- items ----------------------------------------------------------------------------

    /// A local in the body being built.
    pub fn local(&mut self, name: &str, mutable: bool) -> LocalId {
        self.locals.push(LocalDecl {
            name: name.into(),
            mutable,
            span: Span::default(),
            closure: 0,
        });
        LocalId(self.locals.len() as u32 - 1)
    }

    pub fn ty(&self, kind: TypeKind) -> Type {
        Type {
            kind,
            span: Span::default(),
        }
    }

    pub fn builtin(&self, b: BuiltinType) -> Type {
        self.ty(TypeKind::Builtin(b, Vec::new()))
    }

    pub fn param(&mut self, name: &str, ty: Type) -> ParamDecl {
        let local = self.local(name, false);
        ParamDecl {
            name: name.into(),
            local,
            ty,
            default: None,
            span: Span::default(),
        }
    }

    pub fn item(&mut self, def: DefId, name: &str, kind: ItemKind) -> DefId {
        let item = Item {
            def,
            name: name.into(),
            module: self.module.clone(),
            file: self.file,
            span: Span::default(),
            exported: false,
            kind,
        };
        self.program.add(item);
        def
    }

    /// Adds `fn name(params) -> ret: body`, taking the locals declared since the last item.
    pub fn func(
        &mut self,
        def: DefId,
        name: &str,
        params: Vec<ParamDecl>,
        ret: Option<Type>,
        body: BodyValue,
    ) -> DefId {
        let locals = std::mem::take(&mut self.locals);
        self.item(
            def,
            name,
            ItemKind::Fn(FnDecl {
                generics: Vec::new(),
                has_self: false,
                params,
                ret,
                sync_attr: false,
                positional: false,
                parent: None,
                body: Some(Body {
                    locals,
                    value: body,
                }),
            }),
        )
    }

    /// A method `fn name(self, …)` of the impl or trait `parent`. The first parameter must
    /// be `self` with type `TypeKind::SelfType`.
    pub fn method_fn(
        &mut self,
        def: DefId,
        name: &str,
        parent: DefId,
        params: Vec<ParamDecl>,
        ret: Option<Type>,
        body: BodyValue,
    ) -> DefId {
        self.func(def, name, params, ret, body);
        if let Some(mut item) = self.program.item(def).cloned() {
            if let ItemKind::Fn(f) = &mut item.kind {
                f.has_self = true;
                f.parent = Some(parent);
            }
            self.program.replace(item);
        }
        def
    }

    /// `fn main(): <stmts>`
    pub fn main(&mut self, stmts: Vec<Stmt>) -> DefId {
        let def = self.def();
        let block = self.block(stmts);
        self.func(def, "main", Vec::new(), None, BodyValue::Block(block))
    }

    pub fn finish(self) -> HirProgram {
        self.program
    }
}

/// Spec §28 example programs hand-built as HIR (the M2 subset), for tests of later passes
/// that do not start from source text.
pub mod fixtures {
    use super::Builder;
    use crate::hir::*;

    /// Spec §28.1 hello world, plus a second `print` of a non-ASCII (Arabic) string.
    pub fn hello_world() -> HirProgram {
        let mut b = Builder::new("hello/main");
        let s1 = b.str("Hello, world!");
        let p1 = b.print(s1);
        let s2 = b.str("مرحبا بالعالم");
        let p2 = b.print(s2);
        b.main(vec![p1, p2]);
        b.finish()
    }

    /// Spec §28.2 FizzBuzz: `for n in 1..=100` with an `if`/`else if` chain on
    /// `n % k == 0` and an interpolated `"{n}"`.
    pub fn fizzbuzz() -> HirProgram {
        let mut b = Builder::new("fizzbuzz/main");
        let n = b.local("n", false);
        let mut branches = Vec::new();
        for (k, text) in [(15, "FizzBuzz"), (3, "Fizz"), (5, "Buzz")] {
            let nr = b.local_ref(n);
            let kk = b.int(k);
            let rem = b.bin(BinOp::Rem, nr, kk);
            let zero = b.int(0);
            let cond = b.bin(BinOp::Eq, rem, zero);
            let s = b.str(text);
            let p = b.print(s);
            let body = b.block(vec![p]);
            branches.push((cond, body));
        }
        let hole = b.local_ref(n);
        let interp = b.interp(vec![StrPart::Hole(hole)]);
        let p = b.print(interp);
        let else_body = b.block(vec![p]);
        let chain = b.if_expr(branches, Some(else_body));
        let if_stmt = b.expr_stmt(chain);
        let body = b.block(vec![if_stmt]);
        let lo = b.int(1);
        let hi = b.int(100);
        let iter = b.range(lo, hi, true);
        let pat = b.pat_bind(n);
        let for_stmt = b.stmt(StmtKind::For { pat, iter, body });
        b.main(vec![for_stmt]);
        b.finish()
    }
}
