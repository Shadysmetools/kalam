//! The KIR verifier: checks the invariants every pass must preserve (ARCHITECTURE.md §10.3).
//! Run after lowering and after each pass in debug builds and tests.

use kalam_typeck::Ty;

use crate::{Block, Callee, Expr, ExprKind, Function, Program, Stmt};

/// Returns one message per violated invariant (empty when the program is well formed).
pub fn verify(program: &Program) -> Vec<String> {
    let mut v = Verifier {
        program,
        errors: Vec::new(),
        fn_name: String::new(),
        locals: 0,
        mutable: Vec::new(),
        loops: 0,
    };
    for f in &program.functions {
        v.function(f);
    }
    for c in &program.consts {
        v.fn_name = format!("const {}", c.name);
        v.locals = 0;
        v.mutable = Vec::new();
        v.loops = 0;
        if !concrete(&c.ty) {
            v.fail(format!("type {}", c.ty));
        }
        v.expr(&c.value);
    }
    if let Some(id) = program.entry_error_message {
        if program.function(id).is_none() {
            v.errors
                .push(format!("entry error message fn {id} does not exist"));
        }
    }
    if let Some(entry) = program.entry {
        if program.function(entry).is_none() {
            v.errors.push(format!("entry fn {entry} does not exist"));
        }
    }
    v.errors
}

struct Verifier<'a> {
    program: &'a Program,
    errors: Vec<String>,
    fn_name: String,
    locals: usize,
    mutable: Vec<bool>,
    loops: u32,
}

/// Invariant 1: every type is fully inferred.
fn concrete(ty: &Ty) -> bool {
    !ty.any(&mut |t| matches!(t, Ty::Var(_) | Ty::Error))
}

impl Verifier<'_> {
    fn fail(&mut self, msg: String) {
        let msg = format!("{}: {msg}", self.fn_name);
        self.errors.push(msg);
    }

    fn function(&mut self, f: &Function) {
        self.fn_name = f.name.clone();
        self.locals = f.locals.len();
        self.mutable = f.locals.iter().map(|l| l.mutable).collect();
        self.loops = 0;
        for (i, l) in f.locals.iter().enumerate() {
            if !concrete(&l.ty) {
                self.fail(format!("local {i} `{}` has type {}", l.name, l.ty));
            }
        }
        for (p, _) in &f.params {
            self.local(*p);
        }
        if !concrete(&f.ret) {
            self.fail(format!("return type {}", f.ret));
        }
        self.block(&f.body);
    }

    fn local(&mut self, l: u32) {
        if l as usize >= self.locals {
            self.fail(format!("local {l} is not declared"));
        }
    }

    fn block(&mut self, b: &Block) {
        for s in &b.stmts {
            self.stmt(s);
        }
    }

    fn stmt(&mut self, s: &Stmt) {
        match s {
            Stmt::Let { local, ty, value } => {
                self.local(*local);
                if !concrete(ty) {
                    self.fail(format!("let of local {local} has type {ty}"));
                }
                self.expr(value);
            }
            Stmt::Assign { local, value } => {
                self.local(*local);
                if !self.mutable.get(*local as usize).copied().unwrap_or(true) {
                    self.fail(format!("assignment to immutable local {local}"));
                }
                self.expr(value);
            }
            Stmt::Expr(e) | Stmt::Return(e) => self.expr(e),
            Stmt::If {
                cond,
                then,
                otherwise,
            } => {
                self.expr(cond);
                self.block(then);
                self.block(otherwise);
            }
            Stmt::Switch {
                scrutinee,
                arms,
                default,
            } => {
                self.expr(scrutinee);
                for (_, b) in arms {
                    self.block(b);
                }
                self.block(default);
            }
            Stmt::Loop { body } => self.in_loop(body),
            Stmt::ForRange {
                local,
                lo,
                hi,
                body,
                ..
            } => {
                self.local(*local);
                self.expr(lo);
                self.expr(hi);
                self.in_loop(body);
            }
            Stmt::ForEach { local, iter, body } => {
                self.local(*local);
                self.expr(iter);
                self.in_loop(body);
            }
            Stmt::Break | Stmt::Continue => {
                if self.loops == 0 {
                    self.fail("`break`/`continue` outside a loop".into());
                }
            }
        }
    }

    fn in_loop(&mut self, body: &Block) {
        self.loops += 1;
        self.block(body);
        self.loops -= 1;
    }

    fn expr(&mut self, e: &Expr) {
        if !concrete(&e.ty) {
            self.fail(format!("expression {:?} has type {}", e.span, e.ty));
        }
        match &e.kind {
            ExprKind::Local(l) => self.local(*l),
            ExprKind::Prim { op, args } => {
                if args.len() != op.arity() {
                    self.fail(format!("{op:?} takes {} argument(s)", op.arity()));
                }
                args.iter().for_each(|a| self.expr(a));
            }
            ExprKind::Call { callee, args, .. } => {
                match callee {
                    Callee::Fn(id) => {
                        if self.program.function(*id).is_none() {
                            self.fail(format!("call to missing fn {id}"));
                        }
                    }
                    Callee::Value(v) => self.expr(v),
                    Callee::Prelude(_) | Callee::Intrinsic(_) => {}
                }
                args.iter().for_each(|a| self.expr(a));
            }
            ExprKind::Record { fields: es, .. }
            | ExprKind::Variant { fields: es, .. }
            | ExprKind::Tuple(es)
            | ExprKind::List(es)
            | ExprKind::Template { holes: es, .. } => es.iter().for_each(|a| self.expr(a)),
            ExprKind::Field { base, .. }
            | ExprKind::VariantField { base, .. }
            | ExprKind::IsVariant { base, .. }
            | ExprKind::TupleField { base, .. } => self.expr(base),
            ExprKind::Map(entries) => {
                for (k, v) in entries {
                    self.expr(k);
                    self.expr(v);
                }
            }
            ExprKind::FnRef(id) => {
                if self.program.function(*id).is_none() {
                    self.fail(format!("reference to missing fn {id}"));
                }
            }
            ExprKind::Const(def) => {
                if self.program.constant(*def).is_none() {
                    self.fail(format!("use of missing constant {def:?}"));
                }
            }
            ExprKind::If {
                cond,
                then,
                otherwise,
            } => {
                self.expr(cond);
                self.expr(then);
                self.expr(otherwise);
            }
            ExprKind::Block { stmts, value } => {
                stmts.iter().for_each(|s| self.stmt(s));
                self.expr(value);
            }
            ExprKind::Lambda {
                params,
                body,
                self_local,
                ..
            } => {
                params.iter().for_each(|(l, _)| self.local(*l));
                if let Some(l) = self_local {
                    self.local(*l);
                }
                let saved = self.loops;
                self.loops = 0;
                self.block(body);
                self.loops = saved;
            }
            ExprKind::Coerce { value, .. } | ExprKind::Panic(value) => self.expr(value),
            ExprKind::Int(_)
            | ExprKind::Float(_)
            | ExprKind::Decimal(_)
            | ExprKind::BigInt(_)
            | ExprKind::Str(_)
            | ExprKind::Bool(_)
            | ExprKind::Unit => {}
        }
    }
}
