//! Lowering: typed HIR ([`kalam_typeck::hir`] + [`TypeckResults`]) → KIR.
//!
//! **Coverage (M2).** Everything the reference interpreter needs to run prelude programs:
//! literals and interpolation (primitives, and user `Show` impls), `let`/`var` with
//! irrefutable patterns, assignment to locals, record fields, list elements and map entries
//! (§10.4 value semantics), calls with positional, named and defaulted arguments, methods
//! and associated functions (user impls, trait impls chosen statically, and the stdlib's
//! `@builtin` functions as [`Callee::Intrinsic`]), `print`/`panic`/`assert`, records with
//! default fields and `.with`, enums, `Option`/`Result`, `match` with nested patterns,
//! guards, or-patterns, `..`, list and record patterns, `?` (§14.3 rules 1–2 and `Option`),
//! lambdas and local functions (closures capture by value), function values, module
//! constants, `for` over ranges and collections, `while`, and every primitive operator.
//!
//! Anything else is [`LowerError::Unsupported`], never a silent miscompile: `Show` for a
//! type with no user impl (spec gaps G3/G4: derived `Show` text is unspecified), trait
//! methods on a type parameter (no dictionary passing yet), `?` into `AnyError`, ranges
//! as values, and tagged-literal runtime semantics. A program is also refused, fail-closed,
//! when a constant cannot be lowered (constants are evaluated eagerly, Q33) and when its
//! KIR would not verify, such as a type the checker left open (`Ok(1).is_ok()` never fixes
//! `E`): a backend never receives ill-formed KIR.
//!
//! **`match`** lowers to a chain of tests over a temporary holding the scrutinee. Each arm's
//! pattern becomes a conjunction of tests ([`ExprKind::IsVariant`], literal comparisons,
//! list lengths) that short-circuit, so a payload is only read after its tag was checked,
//! plus bindings that run once the tests pass. Every chain ends in a defensive panic, which
//! exhaustiveness checking makes unreachable.

use std::collections::{HashMap, HashSet};

use kalam_base::{FileSpan, Span};
use kalam_resolve::DefId;
use kalam_typeck::adt::AdtKind;
use kalam_typeck::hir::{
    self, Arg, AssignOp, BinOp, BodyValue, BuiltinType, ExprKind as H, HirProgram, ItemKind,
    LangItem, ParamDecl, PatKind, StmtKind, StrPart, TypeKind, UnOp,
};
use kalam_typeck::traits::{BuiltinTrait, TraitEnv, TraitId};
use kalam_typeck::{TryConversion, Ty, TypeckResults};

use crate::{
    Block, Callee, ConstDecl, EnumDecl, Expr, ExprKind, FnId, Function, Instance, LocalDecl,
    LocalId, PreludeFn, PrimOp, Program, RecordDecl, Stmt, VariantDecl,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LowerError {
    /// Lowering only runs on programs without type errors.
    TypeErrors(usize),
    /// A construct this lowering does not handle yet.
    Unsupported { what: &'static str, span: Span },
}

type R<T> = Result<T, LowerError>;

fn unsupported<T>(what: &'static str, span: Span) -> R<T> {
    Err(LowerError::Unsupported { what, span })
}

/// Lowers a whole type-checked program. The top-level function named `main` becomes the
/// entry point (for hand-built HIR; the driver names the entry with
/// [`lower_program_entry`], because `main` is spelled per skin).
pub fn lower_program(prog: &HirProgram, tc: &TypeckResults, package: &str) -> R<Program> {
    let entry = prog.item_ids().into_iter().find(|d| {
        prog.item(*d).is_some_and(|i| {
            i.name == "main" && matches!(&i.kind, ItemKind::Fn(f) if f.parent.is_none())
        })
    });
    lower_program_entry(prog, tc, package, entry)
}

/// Lowers a whole type-checked program with `entry` (the `DefId` resolve gave the file's
/// `main`) as the entry point.
pub fn lower_program_entry(
    prog: &HirProgram,
    tc: &TypeckResults,
    package: &str,
    entry: Option<DefId>,
) -> R<Program> {
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
        let module = adt.module.as_str().to_string();
        match &adt.kind {
            AdtKind::Record(fields) => program.records.push(RecordDecl {
                def: adt.def,
                name: adt.name.clone(),
                module,
                generics,
                fields: fields
                    .iter()
                    .map(|f| (f.name.clone(), f.ty.clone()))
                    .collect(),
            }),
            AdtKind::Enum(vs) => program.enums.push(EnumDecl {
                def: adt.def,
                name: adt.name.clone(),
                module,
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
            // An extern type has no structure to lower; its values come from the runtime.
            AdtKind::Extern => {}
        }
    }
    let mut cx = Ctx {
        prog,
        tc,
        fn_ids: HashMap::new(),
        consts: HashSet::new(),
    };
    for def in prog.item_ids() {
        if let Some(hir::Item {
            kind: ItemKind::Fn(f),
            ..
        }) = prog.item(def)
        {
            if f.body.is_some() {
                let id = cx.fn_ids.len() as FnId;
                cx.fn_ids.insert(def, id);
            }
        }
    }
    program.consts = lower_consts(&mut cx)?;
    let mut ordered: Vec<(DefId, FnId)> = cx.fn_ids.iter().map(|(d, i)| (*d, *i)).collect();
    ordered.sort_by_key(|(_, i)| *i);
    for (def, id) in ordered {
        let Some(item) = prog.item(def) else {
            continue;
        };
        let ItemKind::Fn(decl) = &item.kind else {
            continue;
        };
        let f = FnLower::new(&cx, def).function(id, item, decl)?;
        program.functions.push(f);
    }
    program.entry = entry.and_then(|d| cx.fn_ids.get(&d).copied());
    program.entry_error_message = entry.and_then(|d| entry_error_message(&cx, d));
    refuse_ill_formed(&program)?;
    Ok(program)
}

/// Fail closed: a program whose KIR would not verify is refused here, as
/// [`LowerError::Unsupported`] with a location, so a backend never receives ill-formed KIR
/// and a user program never ends in an internal error. A type the checker left open (an
/// unconstrained type argument) is the case user programs reach; any other violation is a
/// lowering gap, refused at the entry point.
fn refuse_ill_formed(program: &Program) -> R<()> {
    if let Some(at) = crate::verify::first_open_type(program) {
        return unsupported(
            "a value whose type is never fixed (add a type annotation)",
            at.span,
        );
    }
    if !crate::verify::verify(program).is_empty() {
        let at = program
            .entry
            .and_then(|e| program.function(e))
            .map(|f| f.span.span)
            .unwrap_or_default();
        return unsupported("a construct that this lowering cannot represent yet", at);
    }
    Ok(())
}

/// Module constants in dependency order: a constant whose initializer names a constant
/// that is not lowered yet waits for it. Every constant is evaluated eagerly at start
/// (§13.4, Q33), so one that cannot be lowered makes the whole program
/// [`LowerError::Unsupported`] (the first such constant in declaration order), even when
/// nothing uses it: leaving it out would hide its panic.
fn lower_consts(cx: &mut Ctx<'_>) -> R<Vec<ConstDecl>> {
    let (prog, tc) = (cx.prog, cx.tc);
    let mut pending: Vec<DefId> = prog
        .item_ids()
        .into_iter()
        .filter(|d| matches!(prog.item(*d).map(|i| &i.kind), Some(ItemKind::Const(_))))
        .collect();
    let mut out = Vec::new();
    let mut failed: Option<LowerError> = None;
    loop {
        let mut progress = false;
        let mut still = Vec::new();
        for def in pending {
            let Some(item) = prog.item(def) else {
                continue;
            };
            let ItemKind::Const(c) = &item.kind else {
                continue;
            };
            let Some(ty) = tc.consts.get(&def).cloned() else {
                failed.get_or_insert(LowerError::Unsupported {
                    what: "a constant without a type",
                    span: item.span,
                });
                continue;
            };
            let mut l = FnLower::new(cx, def);
            l.foreign = true;
            match l.expr(&c.value) {
                Ok(value) => {
                    out.push(ConstDecl {
                        def,
                        name: item.name.clone(),
                        file: item.file,
                        ty,
                        value,
                    });
                    progress = true;
                }
                Err(LowerError::Unsupported {
                    what: CONST_NOT_READY,
                    ..
                }) => still.push(def),
                Err(e) => {
                    failed.get_or_insert(e);
                }
            }
        }
        for c in &out {
            cx.consts.insert(c.def);
        }
        pending = still;
        if !progress || pending.is_empty() {
            break;
        }
    }
    if let Some(e) = failed {
        return Err(e);
    }
    // Still waiting with no progress: the constants name each other in a cycle.
    if let Some(item) = pending.first().and_then(|d| prog.item(*d)) {
        return unsupported("a constant whose value depends on itself", item.span);
    }
    Ok(out)
}

const CONST_NOT_READY: &str = "a constant that cannot be lowered";

/// The `Error.message` impl used to report `Err` returned from `main` (§14.3).
fn entry_error_message(cx: &Ctx<'_>, entry: DefId) -> Option<FnId> {
    let Some(Ty::Fn { ret, .. }) = cx.tc.signatures.get(&entry).map(|s| &s.ty) else {
        return None;
    };
    let Ty::Result(_, err) = &**ret else {
        return None;
    };
    let method = trait_impl_method(cx.tc, TraitId::Builtin(BuiltinTrait::Error), err, "message")?;
    cx.fn_ids.get(&method).copied()
}

/// The method `name` of the impl of trait `id` for `ty`, if one is written out.
fn trait_impl_method(tc: &TypeckResults, id: TraitId, ty: &Ty, name: &str) -> Option<DefId> {
    tc.traits
        .impls
        .iter()
        .filter(|imp| imp.trait_ref.as_ref().is_some_and(|t| t.id == id))
        .find(|imp| TraitEnv::impl_matches(imp, ty))
        .and_then(|imp| imp.methods.iter().find(|(n, _)| n == name).map(|(_, d)| *d))
}

struct Ctx<'a> {
    prog: &'a HirProgram,
    tc: &'a TypeckResults,
    fn_ids: HashMap<DefId, FnId>,
    /// Constants that lowered.
    consts: HashSet<DefId>,
}

/// One argument slot of a call or a record construction: its name and default.
struct Slot<'h> {
    name: String,
    default: Option<&'h hir::Expr>,
}

/// One step from a place's root local to the assigned part (§10.4).
enum Step {
    /// Record field `index` of a record with `count` fields.
    Field {
        index: u32,
        count: u32,
        ty: Ty,
    },
    TupleField {
        index: u32,
        count: u32,
        ty: Ty,
    },
    /// List element or map entry; the key is a temporary evaluated before the value.
    Index {
        key: LocalId,
        map: bool,
        ty: Ty,
    },
}

struct FnLower<'a> {
    cx: &'a Ctx<'a>,
    owner: DefId,
    locals: Vec<LocalDecl>,
    /// Return types of the enclosing function and closures, innermost last (`?` returns
    /// from the innermost, Q28).
    rets: Vec<Ty>,
    /// Lowering another owner's expression (a default argument, a field default or a
    /// constant): it may not use locals.
    foreign: bool,
}

impl<'a> FnLower<'a> {
    fn new(cx: &'a Ctx<'a>, owner: DefId) -> FnLower<'a> {
        FnLower {
            cx,
            owner,
            locals: Vec::new(),
            rets: Vec::new(),
            foreign: false,
        }
    }

    fn function(mut self, id: FnId, item: &hir::Item, decl: &hir::FnDecl) -> R<Function> {
        let body = decl.body.as_ref().expect("only bodies are lowered");
        // Source locals keep their HIR index; temporaries are appended.
        for (i, l) in body.locals.iter().enumerate() {
            let ty = self
                .cx
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
            self.cx.tc.signatures.get(&self.owner).map(|s| s.ty.clone())
        else {
            return unsupported("function without a signature", item.span);
        };
        let params: Vec<(LocalId, Ty)> = decl
            .params
            .iter()
            .zip(params)
            .map(|(p, t)| (p.local.0, t))
            .collect();
        self.rets.push((*ret).clone());
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
            generics: self.cx.tc.signatures[&self.owner].generics.len() as u32,
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
        match self.cx.tc.expr_types.get(&(self.owner, e.id)) {
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

    /// Binds `v` to a fresh temporary: `let $t = v`, returning a read of it.
    fn bind_temp(&mut self, v: Expr, out: &mut Vec<Stmt>) -> Expr {
        let t = self.temp(v.ty.clone(), false);
        let span = v.span;
        out.push(Stmt::Let {
            local: t,
            ty: v.ty.clone(),
            value: v,
        });
        self.local_expr(t, span)
    }

    fn local(&self, l: hir::LocalId, span: Span) -> R<LocalId> {
        if self.foreign || l.0 as usize >= self.locals.len() {
            return unsupported("a local in a default value or constant", span);
        }
        Ok(l.0)
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
                let l = self.local(*local, s.span)?;
                let v = self.expr(value)?;
                out.push(Stmt::Let {
                    local: l,
                    ty: self.locals[l as usize].ty.clone(),
                    value: v,
                });
            }
            StmtKind::Assign { place, op, value } => self.assign(place, *op, value, s.span, out)?,
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
            StmtKind::LocalFn { local, decl, .. } => self.local_fn(*local, decl, s.span, out)?,
            StmtKind::For { pat, iter, body } => {
                if let H::Range { lo, hi, inclusive } = &iter.kind {
                    let (lo, hi) = (self.expr(lo)?, self.expr(hi)?);
                    let (local, pre) = self.loop_var(pat, Ty::Int)?;
                    let mut stmts = pre;
                    stmts.extend(self.block(body)?.stmts);
                    out.push(Stmt::ForRange {
                        local,
                        lo,
                        hi,
                        inclusive: *inclusive,
                        body: Block { stmts },
                    });
                    return Ok(());
                }
                let it = self.expr(iter)?;
                let elem = match &it.ty {
                    Ty::List(t) | Ty::Set(t) => (**t).clone(),
                    Ty::Map(k, v) => Ty::Tuple(vec![(**k).clone(), (**v).clone()]),
                    _ => return unsupported("iteration over this type", iter.span),
                };
                let (local, mut stmts) = self.loop_var(pat, elem)?;
                stmts.extend(self.block(body)?.stmts);
                out.push(Stmt::ForEach {
                    local,
                    iter: it,
                    body: Block { stmts },
                });
            }
        }
        Ok(())
    }

    /// The loop variable of a `for`, and the statements that destructure it.
    fn loop_var(&mut self, pat: &hir::Pat, elem: Ty) -> R<(LocalId, Vec<Stmt>)> {
        if let PatKind::Bind(l) = pat.kind {
            return Ok((self.local(l, pat.span)?, Vec::new()));
        }
        let t = self.temp(elem, false);
        let mut pre = Vec::new();
        let v = self.local_expr(t, pat.span);
        self.bind(pat, v, &mut pre)?;
        Ok((t, pre))
    }

    /// Binds an irrefutable pattern (typeck checked it, K1003) to a lowered value.
    fn bind(&mut self, pat: &hir::Pat, v: Expr, out: &mut Vec<Stmt>) -> R<()> {
        match &pat.kind {
            PatKind::Bind(l) => {
                let l = self.local(*l, pat.span)?;
                out.push(Stmt::Let {
                    local: l,
                    ty: v.ty.clone(),
                    value: v,
                });
            }
            PatKind::Wild => out.push(Stmt::Expr(v)),
            _ => {
                let scrut = self.bind_temp(v, out);
                let mut tests = Vec::new();
                let mut binds = Vec::new();
                self.pat(pat, &scrut, &mut tests, &mut binds)?;
                out.extend(let_binds(binds));
            }
        }
        Ok(())
    }

    fn local_fn(
        &mut self,
        local: hir::LocalId,
        decl: &hir::FnDecl,
        span: Span,
        out: &mut Vec<Stmt>,
    ) -> R<()> {
        let local = self.local(local, span)?;
        if decl.params.iter().any(|p| p.default.is_some()) {
            return unsupported("default arguments of a local function", span);
        }
        let fty = self.locals[local as usize].ty.clone();
        let Ty::Fn { params, ret, .. } = &fty else {
            return unsupported("local function without a function type", span);
        };
        let mut ps = Vec::new();
        for (p, t) in decl.params.iter().zip(params) {
            ps.push((self.local(p.local, p.span)?, t.clone()));
        }
        let Some(body) = &decl.body else {
            return unsupported("local function without a body", span);
        };
        self.rets.push((**ret).clone());
        let block = match &body.value {
            BodyValue::Block(b) => self.block(b),
            BodyValue::Expr(e) => self.expr(e).map(|e| Block {
                stmts: vec![Stmt::Return(e)],
            }),
        };
        self.rets.pop();
        let value = Expr::new(
            ExprKind::Lambda {
                params: ps,
                body: Box::new(block?),
                suspends: false,
                self_local: Some(local),
            },
            fty.clone(),
            span,
        );
        out.push(Stmt::Let {
            local,
            ty: fty,
            value,
        });
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

    // -- assignment (§10.4) ---------------------------------------------------------------

    /// `place op= value`: the root `var` is rebound to a copy with the part replaced.
    /// Index keys are evaluated first, left to right, then the value (`p.x = e` means
    /// `p = p.with(x: e)`, `xs[i] = e` means `xs = xs.set(i, e)`).
    fn assign(
        &mut self,
        place: &hir::Expr,
        op: AssignOp,
        value: &hir::Expr,
        span: Span,
        out: &mut Vec<Stmt>,
    ) -> R<()> {
        let mut steps = Vec::new();
        let root = self.place(place, &mut steps, out)?;
        let rhs = self.expr(value)?;
        let new = match op {
            AssignOp::Assign => rhs,
            _ => {
                let bin = match op {
                    AssignOp::Add => BinOp::Add,
                    AssignOp::Sub => BinOp::Sub,
                    AssignOp::Mul => BinOp::Mul,
                    _ => BinOp::Div,
                };
                let cur = self.read_place(root, &steps, place.span);
                self.arith(bin, cur, rhs, span)?
            }
        };
        let rebuilt = self.rebuild(root, &steps, 0, new, span);
        out.push(Stmt::Assign {
            local: root,
            value: rebuilt,
        });
        Ok(())
    }

    /// The root local of a place and the steps to its assigned part (outermost first).
    fn place(&mut self, e: &hir::Expr, steps: &mut Vec<Step>, out: &mut Vec<Stmt>) -> R<LocalId> {
        match &e.kind {
            H::Local(l) => self.local(*l, e.span),
            H::Field { base, name } => {
                let root = self.place(base, steps, out)?;
                let bt = self.ty(base)?;
                let Ty::Adt(d, _) = &bt else {
                    return unsupported("assignment to a field of a non-record", e.span);
                };
                let Some(adt) = self.cx.tc.adts.get(d) else {
                    return unsupported("unknown record", e.span);
                };
                let (Some((index, _)), AdtKind::Record(fields)) = (adt.field(name), &adt.kind)
                else {
                    return unsupported("unknown field", e.span);
                };
                steps.push(Step::Field {
                    index: index as u32,
                    count: fields.len() as u32,
                    ty: bt.clone(),
                });
                Ok(root)
            }
            H::TupleField { base, index } => {
                let root = self.place(base, steps, out)?;
                let bt = self.ty(base)?;
                let Ty::Tuple(ts) = &bt else {
                    return unsupported("tuple field of a non-tuple", e.span);
                };
                steps.push(Step::TupleField {
                    index: *index,
                    count: ts.len() as u32,
                    ty: bt.clone(),
                });
                Ok(root)
            }
            H::Index { base, index } => {
                let root = self.place(base, steps, out)?;
                let bt = self.ty(base)?;
                let map = match &bt {
                    Ty::List(_) => false,
                    Ty::Map(..) => true,
                    _ => return unsupported("index assignment on this type", e.span),
                };
                let k = self.expr(index)?;
                let Expr {
                    kind: ExprKind::Local(key),
                    ..
                } = self.bind_temp(k, out)
                else {
                    unreachable!("bind_temp reads a local")
                };
                steps.push(Step::Index { key, map, ty: bt });
                Ok(root)
            }
            _ => unsupported("assignment to this place", e.span),
        }
    }

    /// The current value of the place `root` + `steps`.
    fn read_place(&self, root: LocalId, steps: &[Step], span: Span) -> Expr {
        let mut cur = self.local_expr(root, span);
        for s in steps {
            cur = self.step_read(cur, s, span);
        }
        cur
    }

    fn step_read(&self, base: Expr, s: &Step, span: Span) -> Expr {
        match s {
            Step::Field { index, ty, .. } => {
                let fty = self.field_ty(ty, *index);
                Expr::new(
                    ExprKind::Field {
                        base: Box::new(base),
                        index: *index,
                    },
                    fty,
                    span,
                )
            }
            Step::TupleField { index, ty, .. } => {
                let Ty::Tuple(ts) = ty else {
                    unreachable!("checked in place()")
                };
                Expr::new(
                    ExprKind::TupleField {
                        base: Box::new(base),
                        index: *index,
                    },
                    ts[*index as usize].clone(),
                    span,
                )
            }
            Step::Index { key, ty, .. } => {
                let (op, elem) = match ty {
                    Ty::List(t) => (PrimOp::ListGet, (**t).clone()),
                    Ty::Map(_, v) => (PrimOp::MapGet, (**v).clone()),
                    _ => unreachable!("checked in place()"),
                };
                let k = self.local_expr(*key, span);
                prim(op, vec![base, k], elem, span)
            }
        }
    }

    /// The type of record field `index` of the record type `ty`.
    fn field_ty(&self, ty: &Ty, index: u32) -> Ty {
        let Ty::Adt(d, args) = ty else {
            return Ty::Error;
        };
        match self.cx.tc.adts.get(d).map(|a| &a.kind) {
            Some(AdtKind::Record(fs)) => fs[index as usize].ty.subst(args),
            _ => Ty::Error,
        }
    }

    /// The container at depth `depth` with the part below it replaced by `new`.
    fn rebuild(&self, root: LocalId, steps: &[Step], depth: usize, new: Expr, span: Span) -> Expr {
        let Some(step) = steps.get(depth) else {
            return new;
        };
        let container = self.read_place(root, &steps[..depth], span);
        let child = self.rebuild(root, steps, depth + 1, new, span);
        match step {
            Step::Field { index, count, ty } => {
                let Ty::Adt(def, _) = ty else {
                    unreachable!("checked in place()")
                };
                let fields = (0..*count)
                    .map(|i| {
                        if i == *index {
                            child.clone()
                        } else {
                            let s = Step::Field {
                                index: i,
                                count: *count,
                                ty: ty.clone(),
                            };
                            self.step_read(container.clone(), &s, span)
                        }
                    })
                    .collect();
                Expr::new(ExprKind::Record { def: *def, fields }, ty.clone(), span)
            }
            Step::TupleField { index, count, ty } => {
                let items = (0..*count)
                    .map(|i| {
                        if i == *index {
                            child.clone()
                        } else {
                            let s = Step::TupleField {
                                index: i,
                                count: *count,
                                ty: ty.clone(),
                            };
                            self.step_read(container.clone(), &s, span)
                        }
                    })
                    .collect();
                Expr::new(ExprKind::Tuple(items), ty.clone(), span)
            }
            Step::Index { key, map, ty } => {
                let name = if *map { "Map.insert" } else { "List.set" };
                let k = self.local_expr(*key, span);
                call_expr(
                    Callee::Intrinsic(name.into()),
                    vec![container, k, child],
                    ty.clone(),
                    span,
                )
            }
        }
    }

    // -- patterns -------------------------------------------------------------------------

    /// The tests `pat` makes against `scrut` (a pure expression: a local or a field path
    /// of one) and the bindings it makes once they pass. Tests are in an order where each
    /// payload read comes after the test of its tag.
    fn pat(
        &mut self,
        pat: &hir::Pat,
        scrut: &Expr,
        tests: &mut Vec<Expr>,
        binds: &mut Vec<(LocalId, Expr)>,
    ) -> R<()> {
        let sp = pat.span;
        match &pat.kind {
            PatKind::Wild => {}
            PatKind::Bind(l) => binds.push((self.local(*l, sp)?, scrut.clone())),
            PatKind::As { pat: inner, local } => {
                binds.push((self.local(*local, sp)?, scrut.clone()));
                self.pat(inner, scrut, tests, binds)?;
            }
            PatKind::Bool(b) => {
                let lit = Expr::new(ExprKind::Bool(*b), Ty::Bool, sp);
                tests.push(self.compare(BinOp::Eq, scrut.clone(), lit, sp)?);
            }
            PatKind::Str(s) => {
                let lit = Expr::new(ExprKind::Str(s.clone()), Ty::String, sp);
                tests.push(self.compare(BinOp::Eq, scrut.clone(), lit, sp)?);
            }
            PatKind::Int { negative, value } => {
                let lit = Expr::new(
                    int_literal(&hir::IntLit::Small(*value), *negative, &scrut.ty, sp)?,
                    scrut.ty.clone(),
                    sp,
                );
                tests.push(self.compare(BinOp::Eq, scrut.clone(), lit, sp)?);
            }
            PatKind::Range { lo, hi, inclusive } => {
                let (Ok(lo), Ok(hi)) = (i64::try_from(*lo), i64::try_from(*hi)) else {
                    return unsupported("range pattern bound out of range", sp);
                };
                let lo = Expr::new(ExprKind::Int(lo), Ty::Int, sp);
                let hi = Expr::new(ExprKind::Int(hi), Ty::Int, sp);
                tests.push(self.compare(BinOp::Ge, scrut.clone(), lo, sp)?);
                let op = if *inclusive { BinOp::Le } else { BinOp::Lt };
                tests.push(self.compare(op, scrut.clone(), hi, sp)?);
            }
            PatKind::Tuple(ps) => {
                let Ty::Tuple(ts) = scrut.ty.clone() else {
                    return unsupported("tuple pattern on a non-tuple", sp);
                };
                for (i, (p, t)) in ps.iter().zip(ts).enumerate() {
                    let sub = Expr::new(
                        ExprKind::TupleField {
                            base: Box::new(scrut.clone()),
                            index: i as u32,
                        },
                        t,
                        p.span,
                    );
                    self.pat(p, &sub, tests, binds)?;
                }
            }
            PatKind::Lang { item, inner } => {
                let (tag, payload) = match (item, &scrut.ty) {
                    (LangItem::None, Ty::Option(_)) => (0, None),
                    (LangItem::Some, Ty::Option(t)) => (1, Some((**t).clone())),
                    (LangItem::Ok, Ty::Result(t, _)) => (0, Some((**t).clone())),
                    (LangItem::Err, Ty::Result(_, e)) => (1, Some((**e).clone())),
                    _ => return unsupported("core pattern on this type", sp),
                };
                tests.push(is_variant(scrut.clone(), tag, sp));
                if let (Some(p), Some(t)) = (inner, payload) {
                    let sub = variant_field(scrut.clone(), tag, 0, t, p.span);
                    self.pat(p, &sub, tests, binds)?;
                }
            }
            PatKind::Variant {
                def, name, fields, ..
            } => {
                let Ty::Adt(enum_def, args) = &scrut.ty else {
                    return unsupported("variant pattern on a non-enum", sp);
                };
                let Some(adt) = self.cx.tc.adts.get(enum_def) else {
                    return unsupported("unknown enum", sp);
                };
                let found = match def {
                    Some(d) => adt.variant_by_def(*d),
                    None => adt.variant(name),
                };
                let Some((tag, v)) = found else {
                    return unsupported("unknown variant", sp);
                };
                let tag = tag as u32;
                tests.push(is_variant(scrut.clone(), tag, sp));
                for (i, f) in fields.iter().enumerate() {
                    let index = match &f.label {
                        None => i,
                        Some(l) => {
                            match v.fields.iter().position(|vf| vf.label.as_ref() == Some(l)) {
                                Some(k) => k,
                                None => return unsupported("unknown variant field", f.pat.span),
                            }
                        }
                    };
                    let Some(vf) = v.fields.get(index) else {
                        return unsupported("too many variant fields", f.pat.span);
                    };
                    let sub = variant_field(
                        scrut.clone(),
                        tag,
                        index as u32,
                        vf.ty.subst(args),
                        f.pat.span,
                    );
                    self.pat(&f.pat, &sub, tests, binds)?;
                }
            }
            PatKind::Record { fields, .. } => {
                let Ty::Adt(d, args) = &scrut.ty else {
                    return unsupported("record pattern on a non-record", sp);
                };
                let Some(adt) = self.cx.tc.adts.get(d) else {
                    return unsupported("unknown record", sp);
                };
                for f in fields {
                    let label = match (&f.label, &f.pat.kind) {
                        (Some(l), _) => l.clone(),
                        (None, PatKind::Bind(l)) => match self.locals.get(l.0 as usize) {
                            Some(decl) => decl.name.clone(),
                            None => return unsupported("unknown binding", f.pat.span),
                        },
                        _ => return unsupported("unlabelled record pattern field", f.pat.span),
                    };
                    let Some((index, fd)) = adt.field(&label) else {
                        return unsupported("unknown field", f.pat.span);
                    };
                    let sub = Expr::new(
                        ExprKind::Field {
                            base: Box::new(scrut.clone()),
                            index: index as u32,
                        },
                        fd.ty.subst(args),
                        f.pat.span,
                    );
                    self.pat(&f.pat, &sub, tests, binds)?;
                }
            }
            PatKind::List {
                before,
                rest,
                after,
            } => {
                let Ty::List(elem) = scrut.ty.clone() else {
                    return unsupported("list pattern on a non-list", sp);
                };
                let elem = *elem;
                let len = prim(PrimOp::ListLen, vec![scrut.clone()], Ty::Int, sp);
                let fixed = (before.len() + after.len()) as i64;
                let n = Expr::new(ExprKind::Int(fixed), Ty::Int, sp);
                let op = if rest.is_some() { BinOp::Ge } else { BinOp::Eq };
                tests.push(self.compare(op, len.clone(), n, sp)?);
                for (i, p) in before.iter().enumerate() {
                    let idx = Expr::new(ExprKind::Int(i as i64), Ty::Int, sp);
                    let sub = prim(
                        PrimOp::ListGet,
                        vec![scrut.clone(), idx],
                        elem.clone(),
                        p.span,
                    );
                    self.pat(p, &sub, tests, binds)?;
                }
                // `len - after.len()`: the index where the trailing patterns start.
                let tail_start = |k: usize| {
                    let off = Expr::new(ExprKind::Int((after.len() - k) as i64), Ty::Int, sp);
                    prim(PrimOp::IntSub, vec![len.clone(), off], Ty::Int, sp)
                };
                for (k, p) in after.iter().enumerate() {
                    let sub = prim(
                        PrimOp::ListGet,
                        vec![scrut.clone(), tail_start(k)],
                        elem.clone(),
                        p.span,
                    );
                    self.pat(p, &sub, tests, binds)?;
                }
                if let Some(Some(l)) = rest {
                    let from = Expr::new(ExprKind::Int(before.len() as i64), Ty::Int, sp);
                    let slice = prim(
                        PrimOp::ListSlice,
                        vec![scrut.clone(), from, tail_start(0)],
                        scrut.ty.clone(),
                        sp,
                    );
                    binds.push((self.local(*l, sp)?, slice));
                }
            }
            PatKind::Or(alts) => {
                let mut alt_tests = Vec::new();
                let mut alt_binds: Vec<Vec<(LocalId, Expr)>> = Vec::new();
                for p in alts {
                    let (mut t, mut b) = (Vec::new(), Vec::new());
                    self.pat(p, scrut, &mut t, &mut b)?;
                    alt_tests.push(conj(t, p.span));
                    b.sort_by_key(|(l, _)| *l);
                    alt_binds.push(b);
                }
                // K1004: every alternative binds the same names; each binding takes its
                // value from the first alternative that matches.
                let names: Vec<LocalId> = alt_binds[0].iter().map(|(l, _)| *l).collect();
                if alt_binds
                    .iter()
                    .any(|b| b.iter().map(|(l, _)| *l).collect::<Vec<_>>() != names)
                {
                    return unsupported("or-pattern alternatives with different bindings", sp);
                }
                for (k, l) in names.iter().enumerate() {
                    let mut value = alt_binds.last().expect("an alternative")[k].1.clone();
                    for (t, b) in alt_tests.iter().zip(&alt_binds).rev().skip(1) {
                        let then = b[k].1.clone();
                        value = match t {
                            None => then,
                            Some(t) => if_expr(t.clone(), then, value, sp),
                        };
                    }
                    binds.push((*l, value));
                }
                let mut any: Option<Expr> = None;
                for t in alt_tests {
                    let Some(t) = t else {
                        any = None;
                        break;
                    };
                    any = Some(match any {
                        None => t,
                        Some(a) => or(a, t, sp),
                    });
                }
                tests.extend(any);
            }
            PatKind::Float(_) => return unsupported("float pattern", sp),
            PatKind::Error => return unsupported("error pattern", sp),
        }
        Ok(())
    }

    /// One arm: its condition (tests and guard; `None` when it always matches) and the
    /// bindings that run before its body.
    fn arm(&mut self, arm: &hir::Arm, scrut: &Expr) -> R<(Option<Expr>, Vec<Stmt>)> {
        let mut tests = Vec::new();
        let mut binds = Vec::new();
        self.pat(&arm.pat, scrut, &mut tests, &mut binds)?;
        let bind = let_binds(binds);
        let test = conj(tests, arm.span);
        let cond = match &arm.guard {
            None => test,
            Some(g) => {
                let g = self.expr(g)?;
                let guarded = if bind.is_empty() {
                    g
                } else {
                    block_expr(bind.clone(), g)
                };
                Some(match test {
                    None => guarded,
                    Some(t) => and(t, guarded, arm.span),
                })
            }
        };
        Ok((cond, bind))
    }

    fn scrutinee(&mut self, scrutinee: &hir::Expr) -> R<(Stmt, Expr)> {
        let s = self.expr(scrutinee)?;
        let t = self.temp(s.ty.clone(), false);
        let scrut = self.local_expr(t, scrutinee.span);
        Ok((
            Stmt::Let {
                local: t,
                ty: s.ty.clone(),
                value: s,
            },
            scrut,
        ))
    }

    fn match_stmts(
        &mut self,
        scrutinee: &hir::Expr,
        arms: &[hir::Arm],
        span: Span,
    ) -> R<Vec<Stmt>> {
        let (init, scrut) = self.scrutinee(scrutinee)?;
        let mut otherwise = Block {
            stmts: vec![Stmt::Expr(unreachable_panic(span, Ty::Never))],
        };
        for arm in arms.iter().rev() {
            let (cond, bind) = self.arm(arm, &scrut)?;
            let mut then = bind;
            then.extend(self.block(&arm.body)?.stmts);
            otherwise = match cond {
                None => Block { stmts: then },
                Some(cond) => Block {
                    stmts: vec![Stmt::If {
                        cond,
                        then: Block { stmts: then },
                        otherwise,
                    }],
                },
            };
        }
        let mut out = vec![init];
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
        let (init, scrut) = self.scrutinee(scrutinee)?;
        let mut otherwise = unreachable_panic(span, ty.clone());
        for arm in arms.iter().rev() {
            let (cond, bind) = self.arm(arm, &scrut)?;
            let value = self.block_value(&arm.body, &ty)?;
            let value = if bind.is_empty() {
                value
            } else {
                block_expr(bind, value)
            };
            otherwise = match cond {
                None => value,
                Some(cond) => Expr::new(
                    ExprKind::If {
                        cond: Box::new(cond),
                        then: Box::new(value),
                        otherwise: Box::new(otherwise),
                    },
                    ty.clone(),
                    span,
                ),
            };
        }
        Ok(block_expr(vec![init], otherwise))
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
            StmtKind::Expr(e) if *ty != Ty::Unit => self.expr(e)?,
            StmtKind::Expr(_) => {
                // A `()` arm: its last expression may be a statement-form `if`/`match`.
                self.stmt(last, &mut stmts)?;
                Expr::new(ExprKind::Unit, Ty::Unit, last.span)
            }
            _ => {
                // `return`/`break`/`continue` ending an arm: the arm's value is `Never`.
                self.stmt(last, &mut stmts)?;
                unreachable_panic(last.span, ty.clone())
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
            H::Int(lit) => int_literal(lit, false, &ty, sp)?,
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
            H::Local(l) => ExprKind::Local(self.local(*l, sp)?),
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
            H::ImplicitMember(_) | H::Def(_) if self.variant_pick(&[e]).is_some() => {
                return self.variant(&[e], ty, &[], sp);
            }
            H::Call { callee, args } => return self.call(e, ty, callee, args),
            H::MethodCall {
                receiver,
                name,
                args,
            } => return self.method_call(e, ty, receiver, name, args),
            H::Field { base, name } => {
                let b = self.expr(base)?;
                if matches!(b.ty, Ty::Capability(_)) {
                    return unsupported("a capability (I/O runs from M4)", sp);
                }
                let Ty::Adt(d, _) = &b.ty else {
                    return unsupported("field of a non-record", sp);
                };
                let Some((index, _)) = self.cx.tc.adts.get(d).and_then(|a| a.field(name)) else {
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
                let op = match b.ty {
                    Ty::List(_) => PrimOp::ListGet,
                    Ty::Map(..) => PrimOp::MapGet,
                    _ => return unsupported("indexing this type", sp),
                };
                let i = self.expr(index)?;
                return Ok(prim(op, vec![b, i], ty, sp));
            }
            H::Unary {
                op: UnOp::Neg,
                operand,
            } => {
                if let H::Int(lit) = &operand.kind {
                    int_literal(lit, true, &ty, sp)?
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
            H::Map(entries) => {
                let mut out = Vec::new();
                for (k, v) in entries {
                    out.push((self.expr(k)?, self.expr(v)?));
                }
                ExprKind::Map(out)
            }
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
            H::Try(inner) => return self.try_expr(e, ty, inner),
            H::Lambda { params, body, .. } => {
                if self.foreign {
                    return unsupported("a lambda in a default value or constant", sp);
                }
                let Ty::Fn {
                    params: pts, ret, ..
                } = ty.clone()
                else {
                    return unsupported("lambda without a function type", sp);
                };
                let mut ps = Vec::new();
                for (p, t) in params.iter().zip(pts) {
                    ps.push((self.local(p.local, sp)?, t));
                }
                self.rets.push(*ret);
                let b = self.expr(body);
                self.rets.pop();
                ExprKind::Lambda {
                    params: ps,
                    body: Box::new(Block {
                        stmts: vec![Stmt::Return(b?)],
                    }),
                    suspends: false,
                    self_local: None,
                }
            }
            H::Def(d) if self.cx.fn_ids.contains_key(d) => ExprKind::FnRef(self.cx.fn_ids[d]),
            H::Def(d) if self.is_const(*d) => {
                if !self.cx.consts.contains(d) {
                    return unsupported(CONST_NOT_READY, sp);
                }
                ExprKind::Const(*d)
            }
            H::Lang(_) => return unsupported("a core function as a value", sp),
            H::Def(_) => return unsupported("this item as a value", sp),
            H::ImplicitMember(_) => return unsupported("unresolved implicit member", sp),
            H::Range { .. } => return unsupported("ranges outside `for`", sp),
            H::AssocFn { .. } => return unsupported("an associated function as a value", sp),
            H::Unsafe(_) => return unsupported("`unsafe.*`", sp),
            H::TypeName(_) => return unsupported("a type in value position", sp),
            H::Error => return unsupported("error expression", sp),
        };
        Ok(Expr::new(kind, ty, sp))
    }

    fn is_const(&self, d: DefId) -> bool {
        matches!(
            self.cx.prog.item(d).map(|i| &i.kind),
            Some(ItemKind::Const(_))
        )
    }

    /// Interpolation: `Show` each hole and concatenate left to right (invariant 5).
    fn interp(&mut self, parts: &[StrPart], span: Span) -> R<Expr> {
        let mut acc: Option<Expr> = None;
        for p in parts {
            let piece = match p {
                StrPart::Text(t) => Expr::new(ExprKind::Str(t.clone()), Ty::String, span),
                StrPart::Hole(h) => {
                    let v = self.expr(h)?;
                    self.show(v)?
                }
            };
            acc = Some(match acc {
                None => piece,
                Some(a) => prim(PrimOp::StrConcat, vec![a, piece], Ty::String, span),
            });
        }
        Ok(acc.unwrap_or_else(|| Expr::new(ExprKind::Str(String::new()), Ty::String, span)))
    }

    /// `Show` (§9.7): the primitives, and a user impl (which overrides derivation). Derived
    /// `Show` text for compound types is unspecified (spec gaps G3/G4), so it is refused.
    fn show(&mut self, v: Expr) -> R<Expr> {
        let sp = v.span;
        let op = match v.ty {
            Ty::String => return Ok(v),
            Ty::Int => PrimOp::IntShow,
            Ty::Float => PrimOp::FloatShow,
            Ty::Decimal => PrimOp::DecShow,
            Ty::BigInt => PrimOp::BigShow,
            Ty::Bool => PrimOp::BoolShow,
            Ty::Adt(..) => {
                let id = TraitId::Builtin(BuiltinTrait::Show);
                let Some(m) = trait_impl_method(self.cx.tc, id, &v.ty, "show") else {
                    return unsupported("derived `Show` (its text is unspecified)", sp);
                };
                let callee = self.callee_for(m, sp)?;
                return Ok(call_expr(callee, vec![v], Ty::String, sp));
            }
            // §9.7: `Secret[T]` shows `[REDACTED]`, a capability shows `<Net>`. The value
            // is still evaluated (left-to-right evaluation, §10.6).
            Ty::Secret(_) | Ty::Capability(_) => {
                let text = match &v.ty {
                    Ty::Capability(c) => format!("<{}>", c.name()),
                    _ => "[REDACTED]".to_string(),
                };
                let text = Expr::new(ExprKind::Str(text), Ty::String, sp);
                return Ok(block_expr(vec![Stmt::Expr(v)], text));
            }
            _ => return unsupported("`Show` for this type", sp),
        };
        Ok(prim(op, vec![v], Ty::String, sp))
    }

    fn variant_pick(&self, exprs: &[&hir::Expr]) -> Option<DefId> {
        exprs
            .iter()
            .find_map(|e| self.cx.tc.variant_picks.get(&(self.owner, e.id)).copied())
    }

    fn call(&mut self, e: &hir::Expr, ty: Ty, callee: &hir::Expr, args: &[Arg]) -> R<Expr> {
        let sp = e.span;
        match &callee.kind {
            H::Lang(LangItem::Print) => {
                let slots = [slot("value")];
                let (stmts, a) = self.call_args(self.owner, &slots, Vec::new(), args, sp)?;
                let a = a.into_iter().map(|v| self.show(v)).collect::<R<Vec<_>>>()?;
                let call = call_expr(Callee::Prelude(PreludeFn::Print), a, ty, sp);
                Ok(with_stmts(stmts, call))
            }
            H::Lang(LangItem::Assert) => {
                let slots = [slot("condition"), slot("message")];
                let (stmts, a) = self.call_args(self.owner, &slots, Vec::new(), args, sp)?;
                let call = call_expr(Callee::Prelude(PreludeFn::Assert), a, ty, sp);
                Ok(with_stmts(stmts, call))
            }
            H::Lang(LangItem::Panic) => {
                let slots = [slot("message")];
                let (stmts, mut a) = self.call_args(self.owner, &slots, Vec::new(), args, sp)?;
                let msg = a.pop().expect("typeck checked arity");
                Ok(with_stmts(
                    stmts,
                    Expr::new(ExprKind::Panic(Box::new(msg)), ty, sp),
                ))
            }
            H::Lang(item @ (LangItem::Some | LangItem::Ok | LangItem::Err)) => {
                let index = u32::from(matches!(item, LangItem::Some | LangItem::Err));
                let slots = [slot("value")];
                let (stmts, fields) = self.call_args(self.owner, &slots, Vec::new(), args, sp)?;
                Ok(with_stmts(
                    stmts,
                    Expr::new(
                        ExprKind::Variant {
                            def: None,
                            index,
                            fields,
                        },
                        ty,
                        sp,
                    ),
                ))
            }
            _ if self.variant_pick(&[e, callee]).is_some() => {
                self.variant(&[e, callee], ty, args, sp)
            }
            H::Def(d) if self.cx.tc.adts.contains_key(d) => self.record(*d, ty, args, sp),
            H::Def(d) if self.is_fn(*d) => self.fn_call(*d, &[callee, e], Vec::new(), args, ty, sp),
            H::AssocFn { .. } => {
                let pick = [callee, e]
                    .iter()
                    .find_map(|x| self.cx.tc.method_picks.get(&(self.owner, x.id)).copied());
                let Some(pick) = pick else {
                    return unsupported("unresolved associated function", sp);
                };
                self.fn_call(pick, &[callee, e], Vec::new(), args, ty, sp)
            }
            _ => {
                // A function value: a local, a closure, a constant or any expression.
                let f = self.expr(callee)?;
                if args.iter().any(|a| a.label.is_some()) {
                    return unsupported("named arguments to a function value", sp);
                }
                let a = args
                    .iter()
                    .map(|a| self.expr(&a.value))
                    .collect::<R<Vec<_>>>()?;
                Ok(call_expr(Callee::Value(Box::new(f)), a, ty, sp))
            }
        }
    }

    fn is_fn(&self, d: DefId) -> bool {
        matches!(self.cx.prog.item(d).map(|i| &i.kind), Some(ItemKind::Fn(_)))
    }

    fn method_call(
        &mut self,
        e: &hir::Expr,
        ty: Ty,
        receiver: &hir::Expr,
        name: &str,
        args: &[Arg],
    ) -> R<Expr> {
        let recv = self.expr(receiver)?;
        let pick = self.cx.tc.method_picks.get(&(self.owner, e.id)).copied();
        let is_record = matches!(&recv.ty, Ty::Adt(d, _)
            if matches!(self.cx.tc.adts.get(d).map(|a| &a.kind), Some(AdtKind::Record(_))));
        match pick {
            None if name == "with" && is_record => self.record_update(recv, args, ty, e.span),
            None => unsupported("unresolved method", e.span),
            Some(m) => self.fn_call(m, &[e], vec![recv], args, ty, e.span),
        }
    }

    /// A call of function or method `def`. `leading` holds the receiver of a method call.
    /// Trait methods are dispatched statically on the receiver's (or first argument's)
    /// type; stdlib `@builtin` functions become [`Callee::Intrinsic`].
    fn fn_call(
        &mut self,
        def: DefId,
        inst_keys: &[&hir::Expr],
        leading: Vec<Expr>,
        args: &[Arg],
        ty: Ty,
        sp: Span,
    ) -> R<Expr> {
        let target = self.dispatch(def, leading.first(), args, sp)?;
        let Some(ItemKind::Fn(decl)) = self.cx.prog.item(target).map(|i| &i.kind) else {
            return unsupported("call to a non-function", sp);
        };
        let skip = leading.len().min(decl.params.len());
        let slots = param_slots(&decl.params[skip..]);
        let (stmts, ordered) = self.call_args(target, &slots, leading, args, sp)?;
        let callee = self.callee_for(target, sp)?;
        let type_args = inst_keys
            .iter()
            .find_map(|x| self.cx.tc.instantiations.get(&(self.owner, x.id)).cloned())
            .unwrap_or_default();
        let call = Expr::new(
            ExprKind::Call {
                callee,
                args: ordered,
                type_args,
                suspends: false,
            },
            ty,
            sp,
        );
        Ok(with_stmts(stmts, call))
    }

    /// A trait method resolves to the method of the impl for the receiver's type; any
    /// other function is itself.
    fn dispatch(&mut self, def: DefId, recv: Option<&Expr>, args: &[Arg], sp: Span) -> R<DefId> {
        let prog = self.cx.prog;
        let Some(ItemKind::Fn(decl)) = prog.item(def).map(|i| &i.kind) else {
            return Ok(def);
        };
        let Some(ItemKind::Trait(tr)) = decl.parent.and_then(|p| prog.item(p)).map(|i| &i.kind)
        else {
            return Ok(def);
        };
        let id = match tr.builtin {
            Some(b) => TraitId::Builtin(b),
            None => TraitId::User(decl.parent.expect("checked above")),
        };
        let self_ty = match (recv, args.first()) {
            (Some(r), _) => r.ty.clone(),
            (None, Some(a)) => self.ty(&a.value)?,
            (None, None) => return unsupported("a trait function without a receiver", sp),
        };
        if self_ty.any(&mut |t| matches!(t, Ty::Param(_))) {
            return unsupported("a trait method on a type parameter", sp);
        }
        let name = &prog.item(def).expect("checked above").name;
        match trait_impl_method(self.cx.tc, id, &self_ty, name) {
            Some(m) => Ok(m),
            None => unsupported("a derived trait method", sp),
        }
    }

    /// How a call reaches function `def`: its body, or a runtime intrinsic for a stdlib
    /// signature stub.
    fn callee_for(&self, def: DefId, sp: Span) -> R<Callee> {
        if let Some(&id) = self.cx.fn_ids.get(&def) {
            return Ok(Callee::Fn(id));
        }
        match intrinsic_key(self.cx.prog, def) {
            Some(k) => Ok(Callee::Intrinsic(k)),
            None => unsupported("call to a function without a body", sp),
        }
    }

    /// Orders arguments by slot. Positional arguments come first, then named ones, and a
    /// missing slot takes its default (evaluated at each call, with `default_owner`'s
    /// types). When source order differs from slot order, `leading` and each argument are
    /// first bound to temporaries in source order (invariant 3).
    fn call_args(
        &mut self,
        default_owner: DefId,
        slots: &[Slot<'_>],
        leading: Vec<Expr>,
        args: &[Arg],
        span: Span,
    ) -> R<(Vec<Stmt>, Vec<Expr>)> {
        let mut filled: Vec<Option<usize>> = vec![None; slots.len()];
        for (i, a) in args.iter().enumerate() {
            let k = match &a.label {
                None => i,
                Some(l) => match slots.iter().position(|s| &s.name == l) {
                    Some(k) => k,
                    None => return unsupported("unknown argument label", a.span),
                },
            };
            if k >= slots.len() || filled[k].is_some() {
                return unsupported("argument count", a.span);
            }
            filled[k] = Some(i);
        }
        let lowered: Vec<Expr> = args.iter().map(|a| self.expr(&a.value)).collect::<R<_>>()?;
        // Arguments `0..n` in slots `0..n`: only trailing defaults are missing.
        let in_order = filled
            .iter()
            .enumerate()
            .all(|(k, f)| f.map_or(true, |i| i == k))
            && filled.iter().take(args.len()).all(Option::is_some);
        let mut stmts = Vec::new();
        let (mut ordered, values): (Vec<Expr>, Vec<Expr>) = if in_order {
            (leading, lowered)
        } else {
            let lead = leading
                .into_iter()
                .map(|v| self.bind_temp(v, &mut stmts))
                .collect();
            let vals = lowered
                .into_iter()
                .map(|v| self.bind_temp(v, &mut stmts))
                .collect();
            (lead, vals)
        };
        for (k, f) in filled.iter().enumerate() {
            match f {
                Some(i) => ordered.push(values[*i].clone()),
                None => match slots[k].default {
                    Some(d) => {
                        let v = self
                            .foreign_expr(default_owner, d)
                            .map_err(|e| self.at_call_site(e, default_owner, span))?;
                        ordered.push(v);
                    }
                    None => return unsupported("missing argument", span),
                },
            }
        }
        Ok((stmts, ordered))
    }

    /// A default value from another file (a stdlib stub's) that cannot be lowered is
    /// reported at the call `site`: a span is an offset into its own file only, and the
    /// error carries no file.
    fn at_call_site(&self, e: LowerError, owner: DefId, site: Span) -> LowerError {
        let file = |d: DefId| self.cx.prog.item(d).map(|i| i.file);
        match e {
            LowerError::Unsupported { what, .. } if file(owner) != file(self.owner) => {
                LowerError::Unsupported { what, span: site }
            }
            e => e,
        }
    }

    /// Lowers another owner's expression (a default value): its types are keyed by
    /// `owner`, and it may not use locals.
    fn foreign_expr(&mut self, owner: DefId, e: &hir::Expr) -> R<Expr> {
        let saved = (self.owner, self.foreign);
        self.owner = owner;
        self.foreign = true;
        let r = self.expr(e);
        (self.owner, self.foreign) = saved;
        r
    }

    fn record(&mut self, def: DefId, ty: Ty, args: &[Arg], span: Span) -> R<Expr> {
        let Some(hir::Item {
            kind: ItemKind::Record(rd),
            ..
        }) = self.cx.prog.item(def)
        else {
            return unsupported("record construction", span);
        };
        let slots: Vec<Slot<'_>> = rd
            .fields
            .iter()
            .map(|f| Slot {
                name: f.name.clone(),
                default: f.default.as_ref(),
            })
            .collect();
        let (stmts, ordered) = self.call_args(def, &slots, Vec::new(), args, span)?;
        let rec = Expr::new(
            ExprKind::Record {
                def,
                fields: ordered,
            },
            ty,
            span,
        );
        Ok(with_stmts(stmts, rec))
    }

    /// `r.with(field: value, …)` (§9.4): the receiver, then the arguments in source order,
    /// then a copy with those fields replaced.
    fn record_update(&mut self, recv: Expr, args: &[Arg], ty: Ty, span: Span) -> R<Expr> {
        let Ty::Adt(def, targs) = recv.ty.clone() else {
            return unsupported("`.with` on a non-record", span);
        };
        let Some(AdtKind::Record(fields)) = self.cx.tc.adts.get(&def).map(|a| a.kind.clone())
        else {
            return unsupported("`.with` on a non-record", span);
        };
        let mut stmts = Vec::new();
        let base = self.bind_temp(recv, &mut stmts);
        let mut given: Vec<Option<Expr>> = vec![None; fields.len()];
        for a in args {
            let Some(i) = a
                .label
                .as_ref()
                .and_then(|l| fields.iter().position(|f| &f.name == l))
            else {
                return unsupported("`.with` argument", a.span);
            };
            let v = self.expr(&a.value)?;
            given[i] = Some(self.bind_temp(v, &mut stmts));
        }
        let values = given
            .into_iter()
            .enumerate()
            .map(|(i, g)| {
                g.unwrap_or_else(|| {
                    Expr::new(
                        ExprKind::Field {
                            base: Box::new(base.clone()),
                            index: i as u32,
                        },
                        fields[i].ty.subst(&targs),
                        span,
                    )
                })
            })
            .collect();
        let rec = Expr::new(
            ExprKind::Record {
                def,
                fields: values,
            },
            ty,
            span,
        );
        Ok(block_expr(stmts, rec))
    }

    fn variant(&mut self, keys: &[&hir::Expr], ty: Ty, args: &[Arg], span: Span) -> R<Expr> {
        let Some(vdef) = self.variant_pick(keys) else {
            return unsupported("unresolved variant", span);
        };
        let Ty::Adt(adt, _) = &ty else {
            return unsupported("variant of a non-enum", span);
        };
        let Some((index, v)) = self
            .cx
            .tc
            .adts
            .get(adt)
            .and_then(|a| a.variant_by_def(vdef))
        else {
            return unsupported("unknown variant", span);
        };
        let slots: Vec<Slot<'_>> = v
            .fields
            .iter()
            .enumerate()
            .map(|(i, f)| Slot {
                name: f.label.clone().unwrap_or_else(|| format!("${i}")),
                default: None,
            })
            .collect();
        let (stmts, fields) = self.call_args(self.owner, &slots, Vec::new(), args, span)?;
        let v = Expr::new(
            ExprKind::Variant {
                def: Some(vdef),
                index: index as u32,
                fields,
            },
            ty,
            span,
        );
        Ok(with_stmts(stmts, v))
    }

    /// `e?` (§14.3): `let $t = e`; when `$t` is `Err` (or `None`), return it from the
    /// innermost function or closure (Q28), converted by the recorded rule; otherwise the
    /// value is the payload.
    fn try_expr(&mut self, e: &hir::Expr, ty: Ty, inner: &hir::Expr) -> R<Expr> {
        let sp = e.span;
        let v = self.expr(inner)?;
        let mut stmts = Vec::new();
        let t = self.bind_temp(v, &mut stmts);
        let Some(ret) = self.rets.last().cloned() else {
            return unsupported("`?` outside a function", sp);
        };
        let conv = self.cx.tc.try_conversions.get(&(self.owner, e.id)).cloned();
        let (fail_tag, ok_tag, early) = match (&t.ty, conv) {
            (Ty::Option(_), _) => (
                0,
                1,
                Expr::new(
                    ExprKind::Variant {
                        def: None,
                        index: 0,
                        fields: Vec::new(),
                    },
                    ret,
                    sp,
                ),
            ),
            (Ty::Result(_, err), Some(conv)) => {
                let err_val = variant_field(t.clone(), 1, 0, (**err).clone(), sp);
                let converted = match conv {
                    TryConversion::Same => err_val,
                    TryConversion::From { method: Some(m) } => {
                        let Ty::Result(_, f) = &ret else {
                            return unsupported("`?` in a function that returns no `Result`", sp);
                        };
                        let callee = self.callee_for(m, sp)?;
                        call_expr(callee, vec![err_val], (**f).clone(), sp)
                    }
                    TryConversion::From { method: None } => {
                        return unsupported("`?` through a `From` impl without a method", sp)
                    }
                    TryConversion::BoxAnyError => {
                        return unsupported("`?` into `AnyError`", sp);
                    }
                    TryConversion::OptionNone => {
                        return unsupported("`?` on a `Result` with an `Option` rule", sp)
                    }
                };
                let early = Expr::new(
                    ExprKind::Variant {
                        def: None,
                        index: 1,
                        fields: vec![converted],
                    },
                    ret,
                    sp,
                );
                (1, 0, early)
            }
            _ => return unsupported("`?` on this type", sp),
        };
        stmts.push(Stmt::If {
            cond: is_variant(t.clone(), fail_tag, sp),
            then: Block {
                stmts: vec![Stmt::Return(early)],
            },
            otherwise: Block::default(),
        });
        let value = variant_field(t, ok_tag, 0, ty, sp);
        Ok(block_expr(stmts, value))
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

/// The intrinsic key of a stdlib signature stub (see [`Callee::Intrinsic`]).
fn intrinsic_key(prog: &HirProgram, def: DefId) -> Option<String> {
    let item = prog.item(def)?;
    let ItemKind::Fn(decl) = &item.kind else {
        return None;
    };
    if decl.body.is_some() {
        return None;
    }
    let Some(parent) = decl.parent else {
        return Some(format!("{}.{}", item.module.as_str(), item.name));
    };
    let ItemKind::Impl(imp) = &prog.item(parent)?.kind else {
        return None;
    };
    let owner = match &imp.self_ty.kind {
        TypeKind::Builtin(b, _) => builtin_name(*b).to_string(),
        TypeKind::Def(d, _) => prog.item(*d)?.name.clone(),
        _ => return None,
    };
    Some(format!("{owner}.{}", item.name))
}

fn builtin_name(b: BuiltinType) -> &'static str {
    match b {
        BuiltinType::Int => "Int",
        BuiltinType::BigInt => "BigInt",
        BuiltinType::Float => "Float",
        BuiltinType::Decimal => "Decimal",
        BuiltinType::Bool => "Bool",
        BuiltinType::String => "String",
        BuiltinType::Bytes => "Bytes",
        BuiltinType::Never => "Never",
        BuiltinType::List => "List",
        BuiltinType::Map => "Map",
        BuiltinType::Set => "Set",
        BuiltinType::Option => "Option",
        BuiltinType::Result => "Result",
        BuiltinType::Range => "Range",
        BuiltinType::Secret => "Secret",
        BuiltinType::AnyError => "AnyError",
        BuiltinType::JsValue => "JsValue",
        BuiltinType::Capability(c) => c.name(),
        BuiltinType::Sink(s) => s.name(),
    }
}

fn slot(name: &str) -> Slot<'static> {
    Slot {
        name: name.to_string(),
        default: None,
    }
}

fn param_slots(params: &[ParamDecl]) -> Vec<Slot<'_>> {
    params
        .iter()
        .map(|p| Slot {
            name: p.name.clone(),
            default: p.default.as_ref(),
        })
        .collect()
}

/// An integer literal (negated when it is the operand of unary `-`) of its checked type.
/// The type checker has range-checked it (K0126), so an `Int` fits in 64 bits, a `Float`
/// is finite and a `Decimal` is one decimal128 holds; a literal larger than `u128` is
/// carried as its decimal digits.
fn int_literal(lit: &hir::IntLit, negative: bool, ty: &Ty, sp: Span) -> R<ExprKind> {
    let sign = if negative { "-" } else { "" };
    Ok(match (ty, lit) {
        (Ty::Int, hir::IntLit::Small(v)) => {
            let v = if negative { -(*v as i128) } else { *v as i128 };
            match i64::try_from(v) {
                Ok(v) => ExprKind::Int(v),
                Err(_) => return unsupported("integer literal out of range", sp),
            }
        }
        (Ty::Float, hir::IntLit::Small(v)) => {
            ExprKind::Float(if negative { -(*v as f64) } else { *v as f64 })
        }
        (Ty::Float, hir::IntLit::Large(digits)) => match digits.parse::<f64>() {
            Ok(f) if f.is_finite() => ExprKind::Float(if negative { -f } else { f }),
            _ => return unsupported("integer literal out of range", sp),
        },
        (Ty::Decimal, _) => ExprKind::Decimal(format!("{sign}{}", lit.digits())),
        (Ty::BigInt, _) => ExprKind::BigInt(format!("{sign}{}", lit.digits())),
        (Ty::Int, hir::IntLit::Large(_)) => return unsupported("integer literal out of range", sp),
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

fn is_variant(base: Expr, variant: u32, span: Span) -> Expr {
    Expr::new(
        ExprKind::IsVariant {
            base: Box::new(base),
            variant,
        },
        Ty::Bool,
        span,
    )
}

fn variant_field(base: Expr, variant: u32, index: u32, ty: Ty, span: Span) -> Expr {
    Expr::new(
        ExprKind::VariantField {
            base: Box::new(base),
            variant,
            index,
        },
        ty,
        span,
    )
}

fn let_binds(binds: Vec<(LocalId, Expr)>) -> Vec<Stmt> {
    binds
        .into_iter()
        .map(|(local, value)| Stmt::Let {
            local,
            ty: value.ty.clone(),
            value,
        })
        .collect()
}

/// The conjunction of `tests` (short-circuit, in order); `None` when there are none.
fn conj(tests: Vec<Expr>, span: Span) -> Option<Expr> {
    tests.into_iter().reduce(|a, b| and(a, b, span))
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

fn with_stmts(stmts: Vec<Stmt>, value: Expr) -> Expr {
    if stmts.is_empty() {
        value
    } else {
        block_expr(stmts, value)
    }
}

fn if_expr(cond: Expr, then: Expr, otherwise: Expr, span: Span) -> Expr {
    let ty = then.ty.clone();
    Expr::new(
        ExprKind::If {
            cond: Box::new(cond),
            then: Box::new(then),
            otherwise: Box::new(otherwise),
        },
        ty,
        span,
    )
}

/// `a and b` short-circuits: `if a: b else: false`.
fn and(a: Expr, b: Expr, span: Span) -> Expr {
    let f = Expr::new(ExprKind::Bool(false), Ty::Bool, span);
    if_expr(a, b, f, span)
}

/// `a or b` short-circuits: `if a: true else: b`.
fn or(a: Expr, b: Expr, span: Span) -> Expr {
    let t = Expr::new(ExprKind::Bool(true), Ty::Bool, span);
    if_expr(a, t, b, span)
}

/// The defensive default of a `match` chain: unreachable after exhaustiveness checking.
fn unreachable_panic(span: Span, ty: Ty) -> Expr {
    let msg = Expr::new(
        ExprKind::Str("internal error: no match arm applied".into()),
        Ty::String,
        span,
    );
    Expr::new(ExprKind::Panic(Box::new(msg)), ty, span)
}

#[cfg(test)]
mod tests;
