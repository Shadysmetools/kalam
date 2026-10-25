//! Lowering: typed HIR ([`kalam_typeck::hir`] + [`TypeckResults`]) → KIR.
//!
//! **M2 skeleton.** This covers the subset the M2 checker types end to end, which is what
//! the spec §28.1–§28.2 programs need: literals, interpolation of primitives, `let`/`var` and
//! assignment to locals, calls to functions with bodies (positional or named arguments),
//! `print`/`panic`/`assert`, `Some`/`None`/`Ok`/`Err`, record construction and field
//! access, `if` in both forms, `match` on primitives (literal, range, `_` and binding
//! arms, with guards), `for` over ranges and lists, `while`, `break`/`continue`/`return`,
//! and every primitive operator. Anything else is [`LowerError::Unsupported`], never a
//! silent miscompile.
//!
//! Literal `match` lowers to an `if` chain over a temporary holding the scrutinee (the
//! decision-tree pass will turn enum matches into [`crate::Stmt::Switch`]). Every chain
//! ends in a defensive panic, which exhaustiveness checking makes unreachable.

use std::collections::HashMap;

use kalam_base::{FileSpan, Span};
use kalam_resolve::DefId;
use kalam_typeck::adt::AdtKind;
use kalam_typeck::hir::{
    self, Arg, AssignOp, BinOp, BodyValue, ExprKind as H, ItemKind, LangItem, PatKind,
    ResolvedProgram, StmtKind, StrPart, UnOp,
};
use kalam_typeck::{Ty, TypeckResults};

use crate::{
    Block, Callee, EnumDecl, Expr, ExprKind, FnId, Function, Instance, LocalDecl, LocalId,
    PreludeFn, PrimOp, Program, RecordDecl, Stmt, VariantDecl,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LowerError {
    /// Lowering only runs on programs without type errors.
    TypeErrors(usize),
    /// A construct the M2 skeleton does not lower yet.
    Unsupported { what: &'static str, span: Span },
}

type R<T> = Result<T, LowerError>;

fn unsupported<T>(what: &'static str, span: Span) -> R<T> {
    Err(LowerError::Unsupported { what, span })
}

/// Lowers a whole type-checked program. `main` becomes the entry point.
pub fn lower_program(prog: &dyn ResolvedProgram, tc: &TypeckResults, package: &str) -> R<Program> {
    if !tc.errors.is_empty() {
        return Err(LowerError::TypeErrors(tc.errors.len()));
    }
    let mut program = Program {
        package: package.to_string(),
        ..Program::default()
    };
    let mut adts: Vec<_> = tc.adts.values().collect();
    adts.sort_by_key(|a| a.def);
    for adt in adts {
        let generics = adt.generics.len() as u32;
        match &adt.kind {
            AdtKind::Record(fields) => program.records.push(RecordDecl {
                def: adt.def,
                name: adt.name.clone(),
                generics,
                fields: fields
                    .iter()
                    .map(|f| (f.name.clone(), f.ty.clone()))
                    .collect(),
            }),
            AdtKind::Enum(vs) => program.enums.push(EnumDecl {
                def: adt.def,
                name: adt.name.clone(),
                generics,
                variants: vs
                    .iter()
                    .map(|v| VariantDecl {
                        def: v.def,
                        name: v.name.clone(),
                        fields: v.fields.iter().map(|f| f.ty.clone()).collect(),
                    })
                    .collect(),
            }),
        }
    }
    let mut fn_ids: HashMap<DefId, FnId> = HashMap::new();
    for def in prog.item_ids() {
        if let Some(hir::Item {
            kind: ItemKind::Fn(f),
            ..
        }) = prog.item(def)
        {
            if f.body.is_some() {
                fn_ids.insert(def, fn_ids.len() as FnId);
            }
        }
    }
    let mut ordered: Vec<(DefId, FnId)> = fn_ids.iter().map(|(d, i)| (*d, *i)).collect();
    ordered.sort_by_key(|(_, i)| *i);
    for (def, id) in ordered {
        let Some(item) = prog.item(def) else {
            continue;
        };
        let ItemKind::Fn(decl) = &item.kind else {
            continue;
        };
        let f = FnLower {
            prog,
            tc,
            owner: def,
            fn_ids: &fn_ids,
            locals: Vec::new(),
        }
        .function(id, item, decl)?;
        if item.name == "main" && decl.parent.is_none() {
            program.entry = Some(id);
        }
        program.functions.push(f);
    }
    Ok(program)
}

struct FnLower<'a> {
    prog: &'a dyn ResolvedProgram,
    tc: &'a TypeckResults,
    owner: DefId,
    fn_ids: &'a HashMap<DefId, FnId>,
    locals: Vec<LocalDecl>,
}

impl FnLower<'_> {
    fn function(mut self, id: FnId, item: &hir::Item, decl: &hir::FnDecl) -> R<Function> {
        let body = decl.body.as_ref().expect("only bodies are lowered");
        // Source locals keep their HIR index; temporaries are appended.
        for (i, l) in body.locals.iter().enumerate() {
            let ty = self
                .tc
                .local_types
                .get(&(self.owner, hir::LocalId(i as u32)))
                .cloned()
                .unwrap_or(Ty::Unit);
            self.locals.push(LocalDecl {
                name: l.name.clone(),
                ty,
                mutable: l.mutable,
            });
        }
        let Some(Ty::Fn { params, ret, .. }) =
            self.tc.signatures.get(&self.owner).map(|s| s.ty.clone())
        else {
            return unsupported("function without a signature", item.span);
        };
        if decl.params.iter().any(|p| p.default.is_some()) {
            return unsupported("default arguments", item.span);
        }
        let params: Vec<(LocalId, Ty)> = decl
            .params
            .iter()
            .zip(params)
            .map(|(p, t)| (p.local.0, t))
            .collect();
        let block = match &body.value {
            BodyValue::Block(b) => self.block(b)?,
            BodyValue::Expr(e) => {
                let e = self.expr(e)?;
                Block {
                    stmts: vec![Stmt::Return(e)],
                }
            }
        };
        Ok(Function {
            id,
            def: self.owner,
            name: item.name.clone(),
            generics: self.tc.signatures[&self.owner].generics.len() as u32,
            params,
            locals: self.locals,
            ret: *ret,
            body: block,
            suspends: false,
            instance: Instance::Sync,
            exported: item.exported,
            span: FileSpan::new(item.file, item.span),
        })
    }

    fn ty(&self, e: &hir::Expr) -> R<Ty> {
        match self.tc.expr_types.get(&(self.owner, e.id)) {
            Some(t) => Ok(t.clone()),
            None => unsupported("expression without a type", e.span),
        }
    }

    fn temp(&mut self, ty: Ty, mutable: bool) -> LocalId {
        self.locals.push(LocalDecl {
            name: format!("$t{}", self.locals.len()),
            ty,
            mutable,
        });
        self.locals.len() as LocalId - 1
    }

    fn local_expr(&self, l: LocalId, span: Span) -> Expr {
        Expr::new(ExprKind::Local(l), self.locals[l as usize].ty.clone(), span)
    }

    // -- statements -----------------------------------------------------------------------

    fn block(&mut self, b: &hir::Block) -> R<Block> {
        let mut stmts = Vec::new();
        for s in &b.stmts {
            self.stmt(s, &mut stmts)?;
        }
        Ok(Block { stmts })
    }

    fn stmt(&mut self, s: &hir::Stmt, out: &mut Vec<Stmt>) -> R<()> {
        match &s.kind {
            StmtKind::Pass => {}
            StmtKind::Break => out.push(Stmt::Break),
            StmtKind::Continue => out.push(Stmt::Continue),
            StmtKind::Return(v) => {
                let v = match v {
                    Some(e) => self.expr(e)?,
                    None => Expr::new(ExprKind::Unit, Ty::Unit, s.span),
                };
                out.push(Stmt::Return(v));
            }
            StmtKind::Expr(e) => match &e.kind {
                H::If {
                    branches,
                    else_body,
                } => out.push(self.if_stmt(branches, else_body.as_ref())?),
                H::Match { scrutinee, arms } => {
                    let s = self.match_stmts(scrutinee, arms, e.span)?;
                    out.extend(s);
                }
                _ => out.push(Stmt::Expr(self.expr(e)?)),
            },
            StmtKind::Let { pat, value, .. } => {
                let v = self.expr(value)?;
                self.bind(pat, v, out)?;
            }
            StmtKind::Var { local, value, .. } => {
                let v = self.expr(value)?;
                out.push(Stmt::Let {
                    local: local.0,
                    ty: v.ty.clone(),
                    value: v,
                });
            }
            StmtKind::Assign { place, op, value } => {
                let H::Local(l) = place.kind else {
                    return unsupported("assignment to a field or index", place.span);
                };
                let v = self.expr(value)?;
                let v = match op {
                    AssignOp::Assign => v,
                    AssignOp::Add | AssignOp::Sub | AssignOp::Mul | AssignOp::Div => {
                        let bin = match op {
                            AssignOp::Add => BinOp::Add,
                            AssignOp::Sub => BinOp::Sub,
                            AssignOp::Mul => BinOp::Mul,
                            _ => BinOp::Div,
                        };
                        let cur = self.local_expr(l.0, place.span);
                        self.arith(bin, cur, v, s.span)?
                    }
                };
                out.push(Stmt::Assign {
                    local: l.0,
                    value: v,
                });
            }
            StmtKind::While { cond, body } => {
                let c = self.expr(cond)?;
                let not = prim(PrimOp::Not, vec![c], Ty::Bool, cond.span);
                let mut stmts = vec![Stmt::If {
                    cond: not,
                    then: Block {
                        stmts: vec![Stmt::Break],
                    },
                    otherwise: Block::default(),
                }];
                stmts.extend(self.block(body)?.stmts);
                out.push(Stmt::Loop {
                    body: Block { stmts },
                });
            }
            StmtKind::For { pat, iter, body } => {
                if let H::Range { lo, hi, inclusive } = &iter.kind {
                    let PatKind::Bind(l) = pat.kind else {
                        return unsupported("non-binding pattern over a range", pat.span);
                    };
                    let (lo, hi) = (self.expr(lo)?, self.expr(hi)?);
                    let body = self.block(body)?;
                    out.push(Stmt::ForRange {
                        local: l.0,
                        lo,
                        hi,
                        inclusive: *inclusive,
                        body,
                    });
                    return Ok(());
                }
                let it = self.expr(iter)?;
                let elem = match &it.ty {
                    Ty::List(t) | Ty::Set(t) => (**t).clone(),
                    Ty::Map(k, v) => Ty::Tuple(vec![(**k).clone(), (**v).clone()]),
                    _ => return unsupported("iteration over this type", iter.span),
                };
                let (local, mut pre) = match pat.kind {
                    PatKind::Bind(l) => (l.0, Vec::new()),
                    _ => {
                        let t = self.temp(elem, false);
                        let mut pre = Vec::new();
                        let v = self.local_expr(t, pat.span);
                        self.bind(pat, v, &mut pre)?;
                        (t, pre)
                    }
                };
                pre.extend(self.block(body)?.stmts);
                out.push(Stmt::ForEach {
                    local,
                    iter: it,
                    body: Block { stmts: pre },
                });
            }
        }
        Ok(())
    }

    /// Binds an irrefutable pattern to a lowered value.
    fn bind(&mut self, pat: &hir::Pat, v: Expr, out: &mut Vec<Stmt>) -> R<()> {
        match &pat.kind {
            PatKind::Bind(l) => out.push(Stmt::Let {
                local: l.0,
                ty: v.ty.clone(),
                value: v,
            }),
            PatKind::Wild => out.push(Stmt::Expr(v)),
            PatKind::Tuple(ps) => {
                let Ty::Tuple(ts) = v.ty.clone() else {
                    return unsupported("tuple pattern on a non-tuple", pat.span);
                };
                let t = self.temp(v.ty.clone(), false);
                out.push(Stmt::Let {
                    local: t,
                    ty: v.ty.clone(),
                    value: v,
                });
                for (i, (p, ty)) in ps.iter().zip(ts).enumerate() {
                    let base = self.local_expr(t, pat.span);
                    let field = Expr::new(
                        ExprKind::TupleField {
                            base: Box::new(base),
                            index: i as u32,
                        },
                        ty,
                        p.span,
                    );
                    self.bind(p, field, out)?;
                }
            }
            _ => return unsupported("refutable pattern in a binding", pat.span),
        }
        Ok(())
    }

    fn if_stmt(&mut self, branches: &[hir::IfBranch], else_body: Option<&hir::Block>) -> R<Stmt> {
        let mut otherwise = match else_body {
            Some(b) => self.block(b)?,
            None => Block::default(),
        };
        for b in branches.iter().rev() {
            let cond = self.expr(&b.cond)?;
            let then = self.block(&b.body)?;
            otherwise = Block {
                stmts: vec![Stmt::If {
                    cond,
                    then,
                    otherwise,
                }],
            };
        }
        Ok(otherwise.stmts.pop().expect("at least one branch"))
    }

    // -- match on primitives --------------------------------------------------------------

    /// The test a literal arm makes against the scrutinee temporary.
    fn arm_test(&mut self, pat: &hir::Pat, scrut: &Expr) -> R<Option<Expr>> {
        let sp = pat.span;
        let lit = |kind: ExprKind, ty: Ty| Expr::new(kind, ty, sp);
        let eq =
            |s: &mut Self, rhs: Expr| -> R<Expr> { s.compare(BinOp::Eq, scrut.clone(), rhs, sp) };
        Ok(match &pat.kind {
            PatKind::Wild | PatKind::Bind(_) => None,
            PatKind::Bool(b) => Some(eq(self, lit(ExprKind::Bool(*b), Ty::Bool))?),
            PatKind::Str(s) => Some(eq(self, lit(ExprKind::Str(s.clone()), Ty::String))?),
            PatKind::Int { negative, value } => {
                let v = if *negative {
                    -(*value as i128)
                } else {
                    *value as i128
                };
                Some(eq(self, lit(ExprKind::Int(v as i64), Ty::Int))?)
            }
            PatKind::Range { lo, hi, inclusive } => {
                let lo = lit(ExprKind::Int(*lo as i64), Ty::Int);
                let hi = lit(ExprKind::Int(*hi as i64), Ty::Int);
                let ge = self.compare(BinOp::Ge, scrut.clone(), lo, sp)?;
                let op = if *inclusive { BinOp::Le } else { BinOp::Lt };
                let le = self.compare(op, scrut.clone(), hi, sp)?;
                Some(and(ge, le, sp))
            }
            PatKind::Or(ps) => {
                let mut acc: Option<Expr> = None;
                for p in ps {
                    let Some(t) = self.arm_test(p, scrut)? else {
                        return Ok(None);
                    };
                    acc = Some(match acc {
                        None => t,
                        Some(a) => or(a, t, sp),
                    });
                }
                acc
            }
            _ => return unsupported("match on enums, options and results", pat.span),
        })
    }

    /// `let x = $scrut` for a binding arm.
    fn arm_binding(&self, pat: &hir::Pat, scrut: &Expr) -> Vec<Stmt> {
        match pat.kind {
            PatKind::Bind(l) => vec![Stmt::Let {
                local: l.0,
                ty: scrut.ty.clone(),
                value: scrut.clone(),
            }],
            _ => Vec::new(),
        }
    }

    fn match_stmts(
        &mut self,
        scrutinee: &hir::Expr,
        arms: &[hir::Arm],
        span: Span,
    ) -> R<Vec<Stmt>> {
        let s = self.expr(scrutinee)?;
        let t = self.temp(s.ty.clone(), false);
        let scrut = self.local_expr(t, scrutinee.span);
        let mut otherwise = Block {
            stmts: vec![Stmt::Expr(unreachable_panic(span))],
        };
        for arm in arms.iter().rev() {
            let test = self.arm_test(&arm.pat, &scrut)?;
            let bind = self.arm_binding(&arm.pat, &scrut);
            let mut then = bind.clone();
            then.extend(self.block(&arm.body)?.stmts);
            let cond = match (&arm.guard, test) {
                (None, None) => {
                    otherwise = Block { stmts: then };
                    continue;
                }
                (None, Some(t)) => t,
                (Some(g), test) => {
                    let g = self.expr(g)?;
                    let guarded = if bind.is_empty() {
                        g
                    } else {
                        block_expr(bind, g)
                    };
                    match test {
                        None => guarded,
                        Some(t) => and(t, guarded, span),
                    }
                }
            };
            otherwise = Block {
                stmts: vec![Stmt::If {
                    cond,
                    then: Block { stmts: then },
                    otherwise,
                }],
            };
        }
        let mut out = vec![Stmt::Let {
            local: t,
            ty: s.ty.clone(),
            value: s,
        }];
        out.extend(otherwise.stmts);
        Ok(out)
    }

    fn match_value(
        &mut self,
        ty: Ty,
        scrutinee: &hir::Expr,
        arms: &[hir::Arm],
        span: Span,
    ) -> R<Expr> {
        let s = self.expr(scrutinee)?;
        let t = self.temp(s.ty.clone(), false);
        let scrut = self.local_expr(t, scrutinee.span);
        let mut otherwise = unreachable_panic(span);
        otherwise.ty = ty.clone();
        for arm in arms.iter().rev() {
            let test = self.arm_test(&arm.pat, &scrut)?;
            let bind = self.arm_binding(&arm.pat, &scrut);
            let value = self.block_value(&arm.body, &ty)?;
            let value = if bind.is_empty() {
                value
            } else {
                block_expr(bind.clone(), value)
            };
            let cond = match (&arm.guard, test) {
                (None, None) => {
                    otherwise = value;
                    continue;
                }
                (None, Some(t)) => t,
                (Some(g), test) => {
                    let g = self.expr(g)?;
                    let guarded = if bind.is_empty() {
                        g
                    } else {
                        block_expr(bind, g)
                    };
                    match test {
                        None => guarded,
                        Some(t) => and(t, guarded, span),
                    }
                }
            };
            otherwise = Expr::new(
                ExprKind::If {
                    cond: Box::new(cond),
                    then: Box::new(value),
                    otherwise: Box::new(otherwise),
                },
                ty.clone(),
                span,
            );
        }
        Ok(block_expr(
            vec![Stmt::Let {
                local: t,
                ty: s.ty.clone(),
                value: s,
            }],
            otherwise,
        ))
    }

    /// A value-form arm: its statements, then its last expression (§10.3).
    fn block_value(&mut self, b: &hir::Block, ty: &Ty) -> R<Expr> {
        let Some((last, init)) = b.stmts.split_last() else {
            return Ok(Expr::new(ExprKind::Unit, Ty::Unit, b.span));
        };
        let mut stmts = Vec::new();
        for s in init {
            self.stmt(s, &mut stmts)?;
        }
        let value = match &last.kind {
            StmtKind::Expr(e) => self.expr(e)?,
            _ => {
                // `return`/`break`/`continue` ending an arm: the arm's value is `Never`.
                self.stmt(last, &mut stmts)?;
                let mut p = unreachable_panic(last.span);
                p.ty = ty.clone();
                p
            }
        };
        Ok(if stmts.is_empty() {
            value
        } else {
            block_expr(stmts, value)
        })
    }

    // -- expressions ----------------------------------------------------------------------

    fn expr(&mut self, e: &hir::Expr) -> R<Expr> {
        let ty = self.ty(e)?;
        let sp = e.span;
        let kind = match &e.kind {
            H::Int(Some(v)) => int_literal(*v, false, &ty, sp)?,
            H::Int(None) => return unsupported("integer literal out of range", sp),
            H::Float(text) => {
                let clean: String = text.chars().filter(|c| *c != '_').collect();
                match ty {
                    Ty::Decimal => ExprKind::Decimal(clean),
                    _ => match clean.parse::<f64>() {
                        Ok(f) => ExprKind::Float(f),
                        Err(_) => return unsupported("malformed float literal", sp),
                    },
                }
            }
            H::Str(s) => ExprKind::Str(s.clone()),
            H::Bool(b) => ExprKind::Bool(*b),
            H::Unit => ExprKind::Unit,
            H::Local(l) => ExprKind::Local(l.0),
            H::Interp(parts) => return self.interp(parts, sp),
            H::Tagged { tag, parts, holes } => ExprKind::Template {
                tag: format!("{tag:?}").to_lowercase(),
                parts: parts.clone(),
                holes: holes.iter().map(|h| self.expr(h)).collect::<R<_>>()?,
            },
            H::Lang(LangItem::None) => ExprKind::Variant {
                def: None,
                index: 0,
                fields: Vec::new(),
            },
            H::ImplicitMember(_) | H::Def(_)
                if self.tc.variant_picks.contains_key(&(self.owner, e.id)) =>
            {
                return self.variant(e, ty, &[]);
            }
            H::Call { callee, args } => return self.call(e, ty, callee, args),
            H::Field { base, name } => {
                let b = self.expr(base)?;
                let Ty::Adt(d, _) = &b.ty else {
                    return unsupported("field of a non-record", sp);
                };
                let Some((index, _)) = self.tc.adts.get(d).and_then(|a| a.field(name)) else {
                    return unsupported("unknown field", sp);
                };
                ExprKind::Field {
                    base: Box::new(b),
                    index: index as u32,
                }
            }
            H::TupleField { base, index } => ExprKind::TupleField {
                base: Box::new(self.expr(base)?),
                index: *index,
            },
            H::Index { base, index } => {
                let b = self.expr(base)?;
                if !matches!(b.ty, Ty::List(_)) {
                    return unsupported("indexing a non-list", sp);
                }
                let i = self.expr(index)?;
                return Ok(prim(PrimOp::ListGet, vec![b, i], ty, sp));
            }
            H::Unary {
                op: UnOp::Neg,
                operand,
            } => {
                if let H::Int(Some(v)) = operand.kind {
                    int_literal(v, true, &ty, sp)?
                } else {
                    let o = self.expr(operand)?;
                    let op = match o.ty {
                        Ty::Int => PrimOp::IntNeg,
                        Ty::Float => PrimOp::FloatNeg,
                        Ty::Decimal => PrimOp::DecNeg,
                        Ty::BigInt => PrimOp::BigNeg,
                        _ => return unsupported("negation of this type", sp),
                    };
                    return Ok(prim(op, vec![o], ty, sp));
                }
            }
            H::Unary {
                op: UnOp::Not,
                operand,
            } => {
                let o = self.expr(operand)?;
                return Ok(prim(PrimOp::Not, vec![o], Ty::Bool, sp));
            }
            H::Binary { op, lhs, rhs } => {
                let (l, r) = (self.expr(lhs)?, self.expr(rhs)?);
                return match op {
                    BinOp::And => Ok(and(l, r, sp)),
                    BinOp::Or => Ok(or(l, r, sp)),
                    BinOp::Eq | BinOp::NotEq | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
                        self.compare(*op, l, r, sp)
                    }
                    _ => self.arith(*op, l, r, sp),
                };
            }
            H::Tuple(items) => {
                ExprKind::Tuple(items.iter().map(|i| self.expr(i)).collect::<R<_>>()?)
            }
            H::List(items) => ExprKind::List(items.iter().map(|i| self.expr(i)).collect::<R<_>>()?),
            H::If {
                branches,
                else_body,
            } => {
                let Some(eb) = else_body else {
                    return unsupported("value `if` without `else`", sp);
                };
                let mut acc = self.block_value(eb, &ty)?;
                for b in branches.iter().rev() {
                    let cond = self.expr(&b.cond)?;
                    let then = self.block_value(&b.body, &ty)?;
                    acc = Expr::new(
                        ExprKind::If {
                            cond: Box::new(cond),
                            then: Box::new(then),
                            otherwise: Box::new(acc),
                        },
                        ty.clone(),
                        sp,
                    );
                }
                return Ok(acc);
            }
            H::Match { scrutinee, arms } => return self.match_value(ty, scrutinee, arms, sp),
            H::Lang(_) | H::Def(_) => return unsupported("function values", sp),
            H::ImplicitMember(_) => return unsupported("unresolved implicit member", sp),
            H::MethodCall { .. } => return unsupported("method calls", sp),
            H::Range { .. } => return unsupported("ranges outside `for`", sp),
            H::Lambda { .. } => return unsupported("lambdas", sp),
            H::Error => return unsupported("error expression", sp),
        };
        Ok(Expr::new(kind, ty, sp))
    }

    /// Interpolation: `Show` each hole and concatenate left to right (invariant 5).
    fn interp(&mut self, parts: &[StrPart], span: Span) -> R<Expr> {
        let mut acc: Option<Expr> = None;
        for p in parts {
            let piece = match p {
                StrPart::Text(t) => Expr::new(ExprKind::Str(t.clone()), Ty::String, span),
                StrPart::Hole(h) => {
                    let v = self.expr(h)?;
                    show(v)?
                }
            };
            acc = Some(match acc {
                None => piece,
                Some(a) => prim(PrimOp::StrConcat, vec![a, piece], Ty::String, span),
            });
        }
        Ok(acc.unwrap_or_else(|| Expr::new(ExprKind::Str(String::new()), Ty::String, span)))
    }

    fn call(&mut self, e: &hir::Expr, ty: Ty, callee: &hir::Expr, args: &[Arg]) -> R<Expr> {
        let sp = e.span;
        let positional =
            |s: &mut Self| -> R<Vec<Expr>> { args.iter().map(|a| s.expr(&a.value)).collect() };
        match &callee.kind {
            H::Lang(LangItem::Print) => {
                let a = positional(self)?;
                let a = a.into_iter().map(show).collect::<R<Vec<_>>>()?;
                Ok(call_expr(Callee::Prelude(PreludeFn::Print), a, ty, sp))
            }
            H::Lang(LangItem::Assert) => {
                let a = positional(self)?;
                Ok(call_expr(Callee::Prelude(PreludeFn::Assert), a, ty, sp))
            }
            H::Lang(LangItem::Panic) => {
                let mut a = positional(self)?;
                let msg = a.pop().expect("typeck checked arity");
                Ok(Expr::new(ExprKind::Panic(Box::new(msg)), ty, sp))
            }
            H::Lang(item @ (LangItem::Some | LangItem::Ok | LangItem::Err)) => {
                let index = u32::from(matches!(item, LangItem::Some | LangItem::Err));
                let fields = positional(self)?;
                Ok(Expr::new(
                    ExprKind::Variant {
                        def: None,
                        index,
                        fields,
                    },
                    ty,
                    sp,
                ))
            }
            _ if self.tc.variant_picks.contains_key(&(self.owner, e.id)) => {
                self.variant(e, ty, args)
            }
            H::Def(d) if self.tc.adts.contains_key(d) => self.record(*d, ty, args, sp),
            H::Def(d) => {
                let Some(&id) = self.fn_ids.get(d) else {
                    return unsupported("call to a signature stub", sp);
                };
                let Some(ItemKind::Fn(decl)) = self.prog.item(*d).map(|i| &i.kind) else {
                    return unsupported("call to a non-function", sp);
                };
                let names: Vec<String> = decl.params.iter().map(|p| p.name.clone()).collect();
                let type_args = self
                    .tc
                    .instantiations
                    .get(&(self.owner, callee.id))
                    .cloned()
                    .unwrap_or_default();
                let (stmts, ordered) = self.by_position(&names, args, sp)?;
                let call = Expr::new(
                    ExprKind::Call {
                        callee: Callee::Fn(id),
                        args: ordered,
                        type_args,
                        suspends: false,
                    },
                    ty,
                    sp,
                );
                Ok(if stmts.is_empty() {
                    call
                } else {
                    block_expr(stmts, call)
                })
            }
            _ => unsupported("calls through function values", sp),
        }
    }

    /// Orders arguments by parameter name. When source order differs from parameter
    /// order, each argument is first bound to a temporary in source order (invariant 3).
    fn by_position(
        &mut self,
        names: &[String],
        args: &[Arg],
        span: Span,
    ) -> R<(Vec<Stmt>, Vec<Expr>)> {
        let mut slots: Vec<Option<usize>> = vec![None; names.len()];
        for (i, a) in args.iter().enumerate() {
            let slot = match &a.label {
                None => i,
                Some(l) => match names.iter().position(|n| n == l) {
                    Some(s) => s,
                    None => return unsupported("unknown argument label", a.span),
                },
            };
            slots[slot] = Some(i);
        }
        if slots.iter().any(Option::is_none) {
            return unsupported("default arguments", span);
        }
        let in_order = slots.iter().enumerate().all(|(i, s)| *s == Some(i));
        let lowered: Vec<Expr> = args.iter().map(|a| self.expr(&a.value)).collect::<R<_>>()?;
        if in_order {
            return Ok((Vec::new(), lowered));
        }
        let mut stmts = Vec::new();
        let mut temps = Vec::new();
        for v in lowered {
            let t = self.temp(v.ty.clone(), false);
            temps.push(self.local_expr(t, v.span));
            stmts.push(Stmt::Let {
                local: t,
                ty: v.ty.clone(),
                value: v,
            });
        }
        let ordered = slots
            .into_iter()
            .map(|s| temps[s.expect("checked")].clone())
            .collect();
        Ok((stmts, ordered))
    }

    fn record(&mut self, def: DefId, ty: Ty, args: &[Arg], span: Span) -> R<Expr> {
        let Some(AdtKind::Record(fields)) = self.tc.adts.get(&def).map(|a| &a.kind) else {
            return unsupported("record construction", span);
        };
        let names: Vec<String> = fields.iter().map(|f| f.name.clone()).collect();
        let (stmts, ordered) = self.by_position(&names, args, span)?;
        let rec = Expr::new(
            ExprKind::Record {
                def,
                fields: ordered,
            },
            ty,
            span,
        );
        Ok(if stmts.is_empty() {
            rec
        } else {
            block_expr(stmts, rec)
        })
    }

    fn variant(&mut self, e: &hir::Expr, ty: Ty, args: &[Arg]) -> R<Expr> {
        let vdef = self.tc.variant_picks[&(self.owner, e.id)];
        let Ty::Adt(adt, _) = &ty else {
            return unsupported("variant of a non-enum", e.span);
        };
        let Some((index, v)) = self.tc.adts.get(adt).and_then(|a| a.variant_by_def(vdef)) else {
            return unsupported("unknown variant", e.span);
        };
        let names: Vec<String> = v
            .fields
            .iter()
            .enumerate()
            .map(|(i, f)| f.label.clone().unwrap_or_else(|| format!("${i}")))
            .collect();
        let (stmts, fields) = self.by_position(&names, args, e.span)?;
        let v = Expr::new(
            ExprKind::Variant {
                def: Some(vdef),
                index: index as u32,
                fields,
            },
            ty,
            e.span,
        );
        Ok(if stmts.is_empty() {
            v
        } else {
            block_expr(stmts, v)
        })
    }

    fn compare(&mut self, op: BinOp, l: Expr, r: Expr, sp: Span) -> R<Expr> {
        use PrimOp::*;
        let ordered = |ops: [PrimOp; 5]| match op {
            BinOp::Eq | BinOp::NotEq => ops[0],
            BinOp::Lt => ops[1],
            BinOp::Le => ops[2],
            BinOp::Gt => ops[3],
            _ => ops[4],
        };
        let p = match &l.ty {
            Ty::Int => ordered([IntEq, IntLt, IntLe, IntGt, IntGe]),
            Ty::Float => ordered([FloatEq, FloatLt, FloatLe, FloatGt, FloatGe]),
            Ty::Decimal => ordered([DecEq, DecLt, DecLe, DecGt, DecGe]),
            Ty::BigInt => ordered([BigEq, BigLt, BigLe, BigGt, BigGe]),
            Ty::String => ordered([StrEq, StrLt, StrLe, StrGt, StrGe]),
            Ty::Bool if matches!(op, BinOp::Eq | BinOp::NotEq) => BoolEq,
            _ if matches!(op, BinOp::Eq | BinOp::NotEq) => StructEq,
            _ => return unsupported("ordering comparison on this type", sp),
        };
        let e = prim(p, vec![l, r], Ty::Bool, sp);
        Ok(if op == BinOp::NotEq {
            prim(Not, vec![e], Ty::Bool, sp)
        } else {
            e
        })
    }

    fn arith(&mut self, op: BinOp, l: Expr, r: Expr, sp: Span) -> R<Expr> {
        use PrimOp::*;
        let (p, ty) = match (&l.ty, op) {
            (Ty::Int, BinOp::Add) => (IntAdd, Ty::Int),
            (Ty::Int, BinOp::Sub) => (IntSub, Ty::Int),
            (Ty::Int, BinOp::Mul) => (IntMul, Ty::Int),
            (Ty::Int, BinOp::Div) => (IntToFloatDiv, Ty::Float),
            (Ty::Int, BinOp::IntDiv) => (IntFloorDiv, Ty::Int),
            (Ty::Int, BinOp::Rem) => (IntMod, Ty::Int),
            (Ty::Float, BinOp::Add) => (FloatAdd, Ty::Float),
            (Ty::Float, BinOp::Sub) => (FloatSub, Ty::Float),
            (Ty::Float, BinOp::Mul) => (FloatMul, Ty::Float),
            (Ty::Float, BinOp::Div) => (FloatDiv, Ty::Float),
            (Ty::Decimal, BinOp::Add) => (DecAdd, Ty::Decimal),
            (Ty::Decimal, BinOp::Sub) => (DecSub, Ty::Decimal),
            (Ty::Decimal, BinOp::Mul) => (DecMul, Ty::Decimal),
            (Ty::BigInt, BinOp::Add) => (BigAdd, Ty::BigInt),
            (Ty::BigInt, BinOp::Sub) => (BigSub, Ty::BigInt),
            (Ty::BigInt, BinOp::Mul) => (BigMul, Ty::BigInt),
            (Ty::BigInt, BinOp::IntDiv) => (BigFloorDiv, Ty::BigInt),
            (Ty::BigInt, BinOp::Rem) => (BigMod, Ty::BigInt),
            (Ty::String, BinOp::Add) => (StrConcat, Ty::String),
            (Ty::List(_), BinOp::Add) => (ListConcat, l.ty.clone()),
            _ => return unsupported("this operator on this type", sp),
        };
        Ok(prim(p, vec![l, r], ty, sp))
    }
}

fn int_literal(v: u128, negative: bool, ty: &Ty, sp: Span) -> R<ExprKind> {
    Ok(match ty {
        Ty::Int => {
            let v = if negative { -(v as i128) } else { v as i128 };
            match i64::try_from(v) {
                Ok(v) => ExprKind::Int(v),
                Err(_) => return unsupported("integer literal out of range", sp),
            }
        }
        Ty::Float => ExprKind::Float(if negative { -(v as f64) } else { v as f64 }),
        Ty::Decimal => ExprKind::Decimal(format!("{}{v}", if negative { "-" } else { "" })),
        Ty::BigInt => ExprKind::BigInt(format!("{}{v}", if negative { "-" } else { "" })),
        _ => return unsupported("integer literal of this type", sp),
    })
}

fn prim(op: PrimOp, args: Vec<Expr>, ty: Ty, span: Span) -> Expr {
    Expr::new(ExprKind::Prim { op, args }, ty, span)
}

fn call_expr(callee: Callee, args: Vec<Expr>, ty: Ty, span: Span) -> Expr {
    Expr::new(
        ExprKind::Call {
            callee,
            args,
            type_args: Vec::new(),
            suspends: false,
        },
        ty,
        span,
    )
}

fn block_expr(stmts: Vec<Stmt>, value: Expr) -> Expr {
    let (ty, span) = (value.ty.clone(), value.span);
    Expr::new(
        ExprKind::Block {
            stmts,
            value: Box::new(value),
        },
        ty,
        span,
    )
}

/// `a and b` short-circuits: `if a: b else: false`.
fn and(a: Expr, b: Expr, span: Span) -> Expr {
    let f = Expr::new(ExprKind::Bool(false), Ty::Bool, span);
    Expr::new(
        ExprKind::If {
            cond: Box::new(a),
            then: Box::new(b),
            otherwise: Box::new(f),
        },
        Ty::Bool,
        span,
    )
}

/// `a or b` short-circuits: `if a: true else: b`.
fn or(a: Expr, b: Expr, span: Span) -> Expr {
    let t = Expr::new(ExprKind::Bool(true), Ty::Bool, span);
    Expr::new(
        ExprKind::If {
            cond: Box::new(a),
            then: Box::new(t),
            otherwise: Box::new(b),
        },
        Ty::Bool,
        span,
    )
}

/// `Show` for the primitive types (user `Show` impls lower later).
fn show(v: Expr) -> R<Expr> {
    let sp = v.span;
    let op = match v.ty {
        Ty::String => return Ok(v),
        Ty::Int => PrimOp::IntShow,
        Ty::Float => PrimOp::FloatShow,
        Ty::Decimal => PrimOp::DecShow,
        Ty::BigInt => PrimOp::BigShow,
        Ty::Bool => PrimOp::BoolShow,
        _ => return unsupported("`Show` for compound types", sp),
    };
    Ok(prim(op, vec![v], Ty::String, sp))
}

fn unreachable_panic(span: Span) -> Expr {
    let msg = Expr::new(
        ExprKind::Str("internal error: no match arm applied".into()),
        Ty::String,
        span,
    );
    Expr::new(ExprKind::Panic(Box::new(msg)), Ty::Never, span)
}

#[cfg(test)]
mod tests;
