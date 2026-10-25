//! The Kalam reference interpreter: a tree-walking evaluator over KIR ([`kalam_ir::Program`]).
//!
//! It is the target-neutral oracle for differential testing (ROADMAP M2, spec §25
//! "Integers", ARCHITECTURE.md §20 point 4): every operation follows the spec, never the
//! host. `Int` is a true 64-bit checked integer (DECISIONS D18), `Float` is IEEE binary64,
//! `Decimal` is exact decimal128 with no `/`, `BigInt` has arbitrary precision.
//!
//! **Scope (M2):** the prelude. Core types, `print` (ambient, D20), `panic`, `assert`, and
//! the stdlib's `@builtin` methods of the prelude types ([`intrinsics`]). Everything else in
//! the stdlib exists only as signature stubs until M4; a program that calls one is refused
//! before it starts ([`unsupported_intrinsics`]).
//!
//! **Outcomes.** A Kalam panic (§14.4) is [`InterpError::Panic`] with its message and span,
//! never a Rust panic. [`InterpError::Internal`] means a value had the wrong shape for an
//! operation: for a well-typed program that is a soundness bug in the type checker, the
//! lowering or this interpreter, which the Q19 property test hunts for. Deep recursion is a
//! defined panic ("stack overflow", §9.10) at [`Options::max_depth`] calls; run the
//! interpreter on a thread with a large stack (the driver does).
//!
//! The crate does no I/O of its own: output goes to the `Write` the caller passes (the
//! frontend layering rule, ARCHITECTURE.md §3.2).

pub mod intrinsics;
pub mod num;
pub mod value;

use std::collections::HashMap;
use std::io::Write;
use std::rc::Rc;

use kalam_base::{FileId, Span};
use kalam_ir::{
    Block, Callee, ExprKind, FnId, Function, LocalId, PreludeFn, PrimOp, Program, Stmt, SwitchKey,
};
use kalam_resolve::DefId;
use kalam_typeck::Ty;

use num::{BigInt, Decimal};
use value::{Closure, OrderedMap, Value};

pub use intrinsics::{is_intrinsic, unsupported_intrinsics};

/// Why a run stopped early.
#[derive(Clone, Debug, PartialEq)]
pub enum InterpError {
    /// A defined Kalam panic (§14.4): `panic`, `assert`, `expect`, overflow, an index out
    /// of bounds, division by zero, stack exhaustion.
    Panic {
        message: String,
        file: FileId,
        span: Span,
    },
    /// The program needs something the M2 interpreter does not provide.
    Unsupported(String),
    /// A value of the wrong shape: a bug (the type checker, the lowering or here).
    Internal(String),
    /// Writing the program's output failed.
    Io(String),
}

/// How a run that did not stop early ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Exit {
    /// `main` returned normally (or returned `Ok`).
    Done,
    /// `main` returned `Err` (§14.3): the error's message, when it has an `Error` impl.
    MainErr(Option<String>),
}

#[derive(Clone, Debug)]
pub struct Options {
    /// Nested calls before the run panics with "stack overflow".
    pub max_depth: usize,
}

impl Default for Options {
    fn default() -> Options {
        Options { max_depth: 2_000 }
    }
}

/// Runs the program's entry point, writing its output to `out`.
pub fn run(program: &Program, out: &mut dyn Write, options: &Options) -> Result<Exit, InterpError> {
    let Some(entry) = program.entry else {
        return Err(InterpError::Unsupported(
            "the program has no `main` function".into(),
        ));
    };
    let missing = unsupported_intrinsics(program);
    if let Some(first) = missing.first() {
        return Err(InterpError::Unsupported(format!(
            "{first} is not available in the M2 reference interpreter (prelude only; the rest of the stdlib runs from M4)"
        )));
    }
    let mut it = Interp::new(program, out, options);
    let Some(f) = it.function(entry) else {
        return Err(InterpError::Internal(format!(
            "entry fn {entry} is missing"
        )));
    };
    let args = match f.params.as_slice() {
        [] => Vec::new(),
        [(_, ty)] => vec![it.world(ty)?],
        _ => {
            return Err(InterpError::Unsupported(
                "`main` takes no parameters or one `World`".into(),
            ))
        }
    };
    let result = it
        .call_fn(entry, args, f.span.span)
        .map_err(Ctrl::into_error)?;
    let _ = it.out.flush();
    match result {
        Value::Variant(1, payload) if matches!(f.ret, Ty::Result(..)) => {
            let message = match (program.entry_error_message, payload.first()) {
                (Some(m), Some(e)) => match it
                    .call_fn(m, vec![e.clone()], f.span.span)
                    .map_err(Ctrl::into_error)?
                {
                    Value::Str(s) => Some(s.to_string()),
                    _ => None,
                },
                _ => None,
            };
            Ok(Exit::MainErr(message))
        }
        _ => Ok(Exit::Done),
    }
}

/// Non-local control flow while evaluating, and errors.
pub enum Ctrl {
    Break,
    Continue,
    Return(Value),
    Err(InterpError),
}

impl Ctrl {
    fn into_error(self) -> InterpError {
        match self {
            Ctrl::Err(e) => e,
            Ctrl::Break | Ctrl::Continue => {
                InterpError::Internal("`break`/`continue` outside a loop".into())
            }
            Ctrl::Return(_) => InterpError::Internal("`return` escaped its function".into()),
        }
    }
}

impl From<InterpError> for Ctrl {
    fn from(e: InterpError) -> Ctrl {
        Ctrl::Err(e)
    }
}

pub type Ev<T> = Result<T, Ctrl>;

/// A shape error (see [`InterpError::Internal`]).
pub fn internal<T>(msg: impl Into<String>) -> Ev<T> {
    Err(Ctrl::Err(InterpError::Internal(msg.into())))
}

type Frame = Vec<Option<Value>>;

/// A closure's code: its parameters, body and self binding.
struct Lambda<'p> {
    params: Vec<LocalId>,
    body: &'p Block,
    self_local: Option<LocalId>,
    /// Frame size of the function that contains it.
    frame: usize,
    file: FileId,
}

pub struct Interp<'p, 'o> {
    program: &'p Program,
    fns: HashMap<FnId, &'p Function>,
    out: &'o mut dyn Write,
    lambdas: Vec<Lambda<'p>>,
    lambda_ids: HashMap<*const Block, usize>,
    consts: HashMap<DefId, Value>,
    depth: usize,
    max_depth: usize,
    /// The file of the function being run (panic locations).
    file: FileId,
}

impl<'p, 'o> Interp<'p, 'o> {
    fn new(program: &'p Program, out: &'o mut dyn Write, options: &Options) -> Interp<'p, 'o> {
        let mut it = Interp {
            program,
            fns: program.functions.iter().map(|f| (f.id, f)).collect(),
            out,
            lambdas: Vec::new(),
            lambda_ids: HashMap::new(),
            consts: HashMap::new(),
            depth: 0,
            max_depth: options.max_depth,
            file: FileId(0),
        };
        for f in &program.functions {
            let mut found = Vec::new();
            collect_lambdas_block(&f.body, &mut found);
            for (params, body, self_local) in found {
                it.lambda_ids.insert(body as *const Block, it.lambdas.len());
                it.lambdas.push(Lambda {
                    params,
                    body,
                    self_local,
                    frame: f.locals.len(),
                    file: f.span.file,
                });
            }
        }
        it
    }

    pub fn program(&self) -> &'p Program {
        self.program
    }

    fn function(&self, id: FnId) -> Option<&'p Function> {
        self.fns.get(&id).copied()
    }

    pub fn panic<T>(&self, message: impl Into<String>, span: Span) -> Ev<T> {
        Err(Ctrl::Err(InterpError::Panic {
            message: message.into(),
            file: self.file,
            span,
        }))
    }

    /// The `World` value given to `main(world: World)`. No M2 operation uses a
    /// capability, so it is an opaque token (a record of tokens for the stdlib's `World`
    /// record stub).
    fn world(&self, ty: &Ty) -> Result<Value, InterpError> {
        if let Ty::Capability(c) = ty {
            return Ok(Value::Cap(c.name()));
        }
        let Ty::Adt(def, _) = ty else {
            return Err(InterpError::Unsupported(
                "`main`'s parameter must be a `World`".into(),
            ));
        };
        let Some(rec) = self.program.record(*def) else {
            return Err(InterpError::Unsupported(
                "`main`'s parameter must be a `World`".into(),
            ));
        };
        let fields = rec
            .fields
            .iter()
            .map(|(_, t)| match t {
                Ty::Capability(c) => Ok(Value::Cap(c.name())),
                _ => Err(InterpError::Unsupported(
                    "`main`'s parameter must be a `World`".into(),
                )),
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Value::Record(Rc::from(fields)))
    }

    fn enter(&mut self, span: Span) -> Ev<()> {
        if self.depth >= self.max_depth {
            return self.panic("stack overflow", span);
        }
        self.depth += 1;
        Ok(())
    }

    pub fn call_fn(&mut self, id: FnId, args: Vec<Value>, span: Span) -> Ev<Value> {
        let Some(f) = self.function(id) else {
            return internal(format!("call to missing fn {id}"));
        };
        if args.len() != f.params.len() {
            return internal(format!("`{}` takes {} argument(s)", f.name, f.params.len()));
        }
        self.enter(span)?;
        let saved = self.file;
        self.file = f.span.file;
        let mut frame: Frame = vec![None; f.locals.len()];
        for ((l, _), v) in f.params.iter().zip(args) {
            frame[*l as usize] = Some(v);
        }
        let r = self.block(&f.body, &mut frame);
        self.file = saved;
        self.depth -= 1;
        match r {
            Ok(()) => Ok(Value::Unit),
            Err(Ctrl::Return(v)) => Ok(v),
            Err(other) => Err(Ctrl::Err(other.into_error())),
        }
    }

    /// Calls a function value (a function reference or a closure).
    pub fn call_value(&mut self, f: &Value, args: Vec<Value>, span: Span) -> Ev<Value> {
        let c = match f {
            Value::Fn(id) => return self.call_fn(*id, args, span),
            Value::Closure(c) => c,
            other => return internal(format!("call of a {} value", other.kind())),
        };
        let Some(l) = self.lambdas.get(c.lambda) else {
            return internal("unknown closure");
        };
        let (body, self_local, file) = (l.body, l.self_local, l.file);
        if args.len() != l.params.len() {
            return internal(format!("closure takes {} argument(s)", l.params.len()));
        }
        let mut frame = c.env.clone();
        frame.resize(l.frame, None);
        for (p, v) in l.params.iter().zip(args) {
            frame[*p as usize] = Some(v);
        }
        if let Some(s) = self_local {
            frame[s as usize] = Some(f.clone());
        }
        self.enter(span)?;
        let saved = self.file;
        self.file = file;
        let r = self.block(body, &mut frame);
        self.file = saved;
        self.depth -= 1;
        match r {
            Ok(()) => Ok(Value::Unit),
            Err(Ctrl::Return(v)) => Ok(v),
            Err(other) => Err(Ctrl::Err(other.into_error())),
        }
    }

    // -- statements -----------------------------------------------------------------------

    fn block(&mut self, b: &'p Block, fr: &mut Frame) -> Ev<()> {
        for s in &b.stmts {
            self.stmt(s, fr)?;
        }
        Ok(())
    }

    fn set(fr: &mut Frame, l: LocalId, v: Value) -> Ev<()> {
        match fr.get_mut(l as usize) {
            Some(slot) => {
                *slot = Some(v);
                Ok(())
            }
            None => internal(format!("local {l} is not in the frame")),
        }
    }

    /// Runs a loop body; `Ok(false)` after `break`.
    fn loop_body(&mut self, body: &'p Block, fr: &mut Frame) -> Ev<bool> {
        match self.block(body, fr) {
            Ok(()) | Err(Ctrl::Continue) => Ok(true),
            Err(Ctrl::Break) => Ok(false),
            Err(other) => Err(other),
        }
    }

    fn stmt(&mut self, s: &'p Stmt, fr: &mut Frame) -> Ev<()> {
        match s {
            Stmt::Let { local, value, .. } | Stmt::Assign { local, value } => {
                let v = self.expr(value, fr)?;
                Interp::set(fr, *local, v)?;
            }
            Stmt::Expr(e) => {
                self.expr(e, fr)?;
            }
            Stmt::If {
                cond,
                then,
                otherwise,
            } => {
                if self.bool(cond, fr)? {
                    self.block(then, fr)?;
                } else {
                    self.block(otherwise, fr)?;
                }
            }
            Stmt::Switch {
                scrutinee,
                arms,
                default,
            } => {
                let v = self.expr(scrutinee, fr)?;
                let hit = arms.iter().find(|(k, _)| match (k, &v) {
                    (SwitchKey::Variant(t), Value::Variant(u, _)) => t == u,
                    (SwitchKey::Int(a), Value::Int(b)) => a == b,
                    (SwitchKey::Str(a), Value::Str(b)) => a.as_str() == &**b,
                    (SwitchKey::Bool(a), Value::Bool(b)) => a == b,
                    _ => false,
                });
                match hit {
                    Some((_, b)) => self.block(b, fr)?,
                    None => self.block(default, fr)?,
                }
            }
            Stmt::Loop { body } => while self.loop_body(body, fr)? {},
            Stmt::ForRange {
                local,
                lo,
                hi,
                inclusive,
                body,
            } => {
                let lo = self.int(lo, fr)?;
                let hi = self.int(hi, fr)?;
                // i128 bounds: `lo..=i64::MAX` ends after its last value, never overflows.
                let end = if *inclusive {
                    hi as i128 + 1
                } else {
                    hi as i128
                };
                let mut i = lo as i128;
                while i < end {
                    Interp::set(fr, *local, Value::Int(i as i64))?;
                    if !self.loop_body(body, fr)? {
                        break;
                    }
                    i += 1;
                }
            }
            Stmt::ForEach { local, iter, body } => {
                let items: Vec<Value> = match self.expr(iter, fr)? {
                    Value::List(xs) => xs.to_vec(),
                    Value::Set(m) => m.keys().cloned().collect(),
                    Value::Map(m) => m
                        .entries()
                        .iter()
                        .map(|(k, v)| Value::Tuple(Rc::from(vec![k.clone(), v.clone()])))
                        .collect(),
                    other => return internal(format!("`for` over a {}", other.kind())),
                };
                for item in items {
                    Interp::set(fr, *local, item)?;
                    if !self.loop_body(body, fr)? {
                        break;
                    }
                }
            }
            Stmt::Break => return Err(Ctrl::Break),
            Stmt::Continue => return Err(Ctrl::Continue),
            Stmt::Return(e) => {
                let v = self.expr(e, fr)?;
                return Err(Ctrl::Return(v));
            }
        }
        Ok(())
    }

    // -- expressions ----------------------------------------------------------------------

    fn bool(&mut self, e: &'p kalam_ir::Expr, fr: &mut Frame) -> Ev<bool> {
        match self.expr(e, fr)? {
            Value::Bool(b) => Ok(b),
            other => internal(format!("condition is a {}", other.kind())),
        }
    }

    fn int(&mut self, e: &'p kalam_ir::Expr, fr: &mut Frame) -> Ev<i64> {
        match self.expr(e, fr)? {
            Value::Int(i) => Ok(i),
            other => internal(format!("expected an Int, found a {}", other.kind())),
        }
    }

    fn exprs(&mut self, es: &'p [kalam_ir::Expr], fr: &mut Frame) -> Ev<Vec<Value>> {
        es.iter().map(|e| self.expr(e, fr)).collect()
    }

    fn expr(&mut self, e: &'p kalam_ir::Expr, fr: &mut Frame) -> Ev<Value> {
        let sp = e.span;
        Ok(match &e.kind {
            ExprKind::Local(l) => match fr.get(*l as usize) {
                Some(Some(v)) => v.clone(),
                _ => return internal(format!("read of unset local {l}")),
            },
            ExprKind::Int(i) => Value::Int(*i),
            ExprKind::Float(f) => Value::Float(*f),
            ExprKind::Decimal(text) => match Decimal::parse(text) {
                Ok(d) => Value::Decimal(Rc::new(d)),
                Err(_) => return internal(format!("malformed Decimal literal {text}")),
            },
            ExprKind::BigInt(text) => match num::parse_big(text) {
                Ok(b) => Value::BigInt(Rc::new(b)),
                Err(_) => return internal(format!("malformed BigInt literal {text}")),
            },
            ExprKind::Str(s) => Value::str(s),
            ExprKind::Bool(b) => Value::Bool(*b),
            ExprKind::Unit => Value::Unit,
            ExprKind::Prim { op, args } => {
                let a = self.exprs(args, fr)?;
                self.prim(*op, a, sp)?
            }
            ExprKind::Call { callee, args, .. } => {
                let f = match callee {
                    Callee::Value(f) => Some(self.expr(f, fr)?),
                    _ => None,
                };
                let a = self.exprs(args, fr)?;
                match callee {
                    Callee::Fn(id) => self.call_fn(*id, a, sp)?,
                    Callee::Value(_) => self.call_value(&f.expect("evaluated above"), a, sp)?,
                    Callee::Prelude(PreludeFn::Print) => {
                        let [Value::Str(s)] = a.as_slice() else {
                            return internal("`print` of a non-String");
                        };
                        let r = self
                            .out
                            .write_all(s.as_bytes())
                            .and_then(|()| self.out.write_all(b"\n"));
                        if let Err(e) = r {
                            return Err(Ctrl::Err(InterpError::Io(e.to_string())));
                        }
                        Value::Unit
                    }
                    Callee::Prelude(PreludeFn::Assert) => match a.as_slice() {
                        [Value::Bool(true), _] => Value::Unit,
                        [Value::Bool(false), Value::Str(m)] => return self.panic(&**m, sp),
                        _ => return internal("`assert` arguments"),
                    },
                    Callee::Intrinsic(name) => self.intrinsic(name, a, sp)?,
                }
            }
            ExprKind::Record { fields, .. } => Value::Record(Rc::from(self.exprs(fields, fr)?)),
            ExprKind::Variant { index, fields, .. } => {
                Value::Variant(*index, Rc::from(self.exprs(fields, fr)?))
            }
            ExprKind::Field { base, index } => match self.expr(base, fr)? {
                Value::Record(vs) => match vs.get(*index as usize) {
                    Some(v) => v.clone(),
                    None => return internal(format!("record has no field {index}")),
                },
                other => return internal(format!("field of a {}", other.kind())),
            },
            ExprKind::TupleField { base, index } => match self.expr(base, fr)? {
                Value::Tuple(vs) => match vs.get(*index as usize) {
                    Some(v) => v.clone(),
                    None => return internal(format!("tuple has no field {index}")),
                },
                other => return internal(format!("tuple field of a {}", other.kind())),
            },
            ExprKind::IsVariant { base, variant } => match self.expr(base, fr)? {
                Value::Variant(t, _) => Value::Bool(t == *variant),
                other => return internal(format!("variant test on a {}", other.kind())),
            },
            ExprKind::VariantField {
                base,
                variant,
                index,
            } => match self.expr(base, fr)? {
                Value::Variant(t, vs) if t == *variant => match vs.get(*index as usize) {
                    Some(v) => v.clone(),
                    None => return internal(format!("variant {t} has no field {index}")),
                },
                Value::Variant(t, _) => {
                    return internal(format!(
                        "payload of variant {variant} read from variant {t}"
                    ))
                }
                other => return internal(format!("payload of a {}", other.kind())),
            },
            ExprKind::Tuple(items) => Value::Tuple(Rc::from(self.exprs(items, fr)?)),
            ExprKind::List(items) => Value::list(self.exprs(items, fr)?),
            ExprKind::Map(entries) => {
                let mut m = OrderedMap::default();
                for (k, v) in entries {
                    let k = self.expr(k, fr)?;
                    let v = self.expr(v, fr)?;
                    if !k.hashable() {
                        return internal(format!("a {} as a map key", k.kind()));
                    }
                    m.insert(k, v);
                }
                Value::Map(Rc::new(m))
            }
            ExprKind::FnRef(id) => Value::Fn(*id),
            ExprKind::Const(def) => self.constant(*def, sp)?,
            ExprKind::If {
                cond,
                then,
                otherwise,
            } => {
                if self.bool(cond, fr)? {
                    self.expr(then, fr)?
                } else {
                    self.expr(otherwise, fr)?
                }
            }
            ExprKind::Block { stmts, value } => {
                for s in stmts {
                    self.stmt(s, fr)?;
                }
                self.expr(value, fr)?
            }
            ExprKind::Lambda { body, .. } => {
                let Some(&lambda) = self.lambda_ids.get(&(&**body as *const Block)) else {
                    return internal("closure outside the lambda table");
                };
                Value::Closure(Rc::new(Closure {
                    lambda,
                    env: fr.clone(),
                }))
            }
            ExprKind::Coerce { value, .. } => self.expr(value, fr)?,
            ExprKind::Template { tag, .. } => {
                return Err(Ctrl::Err(InterpError::Unsupported(format!(
                    "`{tag}\"…\"` literals are not available in the M2 reference interpreter"
                ))))
            }
            ExprKind::Panic(msg) => match self.expr(msg, fr)? {
                Value::Str(m) => return self.panic(&*m, sp),
                other => return internal(format!("`panic` of a {}", other.kind())),
            },
        })
    }

    /// A module constant: evaluated on first use, then remembered.
    fn constant(&mut self, def: DefId, span: Span) -> Ev<Value> {
        if let Some(v) = self.consts.get(&def) {
            return Ok(v.clone());
        }
        let Some(c) = self.program.constant(def) else {
            return internal(format!("missing constant {def:?}"));
        };
        self.enter(span)?;
        let r = self.expr(&c.value, &mut Vec::new());
        self.depth -= 1;
        let v = r?;
        self.consts.insert(def, v.clone());
        Ok(v)
    }

    fn prim(&mut self, op: PrimOp, a: Vec<Value>, sp: Span) -> Ev<Value> {
        use PrimOp::*;
        use Value as V;
        let overflow = "integer overflow";
        Ok(match (op, a.as_slice()) {
            (IntAdd, [V::Int(x), V::Int(y)]) => match x.checked_add(*y) {
                Some(r) => V::Int(r),
                None => return self.panic(overflow, sp),
            },
            (IntSub, [V::Int(x), V::Int(y)]) => match x.checked_sub(*y) {
                Some(r) => V::Int(r),
                None => return self.panic(overflow, sp),
            },
            (IntMul, [V::Int(x), V::Int(y)]) => match x.checked_mul(*y) {
                Some(r) => V::Int(r),
                None => return self.panic(overflow, sp),
            },
            (IntFloorDiv, [V::Int(x), V::Int(y)]) => match num::int_floor_div(*x, *y) {
                Some(r) => V::Int(r),
                None if *y == 0 => return self.panic("division by zero", sp),
                None => return self.panic(overflow, sp),
            },
            (IntMod, [V::Int(x), V::Int(y)]) => match num::int_mod(*x, *y) {
                Some(r) => V::Int(r),
                None => return self.panic("modulo by zero", sp),
            },
            (IntToFloatDiv, [V::Int(x), V::Int(y)]) => {
                if *y == 0 {
                    return self.panic("division by zero", sp);
                }
                V::Float(*x as f64 / *y as f64)
            }
            (IntNeg, [V::Int(x)]) => match x.checked_neg() {
                Some(r) => V::Int(r),
                None => return self.panic(overflow, sp),
            },
            (IntEq, [V::Int(x), V::Int(y)]) => V::Bool(x == y),
            (IntLt, [V::Int(x), V::Int(y)]) => V::Bool(x < y),
            (IntLe, [V::Int(x), V::Int(y)]) => V::Bool(x <= y),
            (IntGt, [V::Int(x), V::Int(y)]) => V::Bool(x > y),
            (IntGe, [V::Int(x), V::Int(y)]) => V::Bool(x >= y),
            (IntShow, [V::Int(x)]) => V::str(&x.to_string()),
            (FloatAdd, [V::Float(x), V::Float(y)]) => V::Float(x + y),
            (FloatSub, [V::Float(x), V::Float(y)]) => V::Float(x - y),
            (FloatMul, [V::Float(x), V::Float(y)]) => V::Float(x * y),
            (FloatDiv, [V::Float(x), V::Float(y)]) => V::Float(x / y),
            (FloatNeg, [V::Float(x)]) => V::Float(-x),
            (FloatEq, [V::Float(x), V::Float(y)]) => V::Bool(x == y),
            (FloatLt, [V::Float(x), V::Float(y)]) => V::Bool(x < y),
            (FloatLe, [V::Float(x), V::Float(y)]) => V::Bool(x <= y),
            (FloatGt, [V::Float(x), V::Float(y)]) => V::Bool(x > y),
            (FloatGe, [V::Float(x), V::Float(y)]) => V::Bool(x >= y),
            (FloatShow, [V::Float(x)]) => V::str(&show_float(*x)),
            (DecAdd | DecSub | DecMul, [V::Decimal(x), V::Decimal(y)]) => {
                let r = match op {
                    DecAdd => x.add(y),
                    DecSub => x.sub(y),
                    _ => x.mul(y),
                };
                match r {
                    Some(d) => V::Decimal(Rc::new(d)),
                    None => return self.panic("Decimal overflow (more than 34 digits)", sp),
                }
            }
            (DecNeg, [V::Decimal(x)]) => V::Decimal(Rc::new(x.negated())),
            (DecEq, [V::Decimal(x), V::Decimal(y)]) => V::Bool(x == y),
            (DecLt, [V::Decimal(x), V::Decimal(y)]) => V::Bool(x < y),
            (DecLe, [V::Decimal(x), V::Decimal(y)]) => V::Bool(x <= y),
            (DecGt, [V::Decimal(x), V::Decimal(y)]) => V::Bool(x > y),
            (DecGe, [V::Decimal(x), V::Decimal(y)]) => V::Bool(x >= y),
            (DecShow, [V::Decimal(x)]) => V::str(&x.to_string()),
            (BigAdd, [V::BigInt(x), V::BigInt(y)]) => V::BigInt(Rc::new(x.add(y))),
            (BigSub, [V::BigInt(x), V::BigInt(y)]) => V::BigInt(Rc::new(x.sub(y))),
            (BigMul, [V::BigInt(x), V::BigInt(y)]) => V::BigInt(Rc::new(x.mul(y))),
            (BigFloorDiv | BigMod, [V::BigInt(x), V::BigInt(y)]) => match x.divmod_floor(y) {
                Some((q, r)) => V::BigInt(Rc::new(if op == BigFloorDiv { q } else { r })),
                None => return self.panic("division by zero", sp),
            },
            (BigNeg, [V::BigInt(x)]) => V::BigInt(Rc::new(BigInt::clone(x).negated())),
            (BigEq, [V::BigInt(x), V::BigInt(y)]) => V::Bool(x == y),
            (BigLt, [V::BigInt(x), V::BigInt(y)]) => V::Bool(x < y),
            (BigLe, [V::BigInt(x), V::BigInt(y)]) => V::Bool(x <= y),
            (BigGt, [V::BigInt(x), V::BigInt(y)]) => V::Bool(x > y),
            (BigGe, [V::BigInt(x), V::BigInt(y)]) => V::Bool(x >= y),
            (BigShow, [V::BigInt(x)]) => V::str(&x.to_string()),
            (StrConcat, [V::Str(x), V::Str(y)]) => V::str(&format!("{x}{y}")),
            (StrEq, [V::Str(x), V::Str(y)]) => V::Bool(x == y),
            // UTF-8 byte order is code-point order.
            (StrLt, [V::Str(x), V::Str(y)]) => V::Bool(x < y),
            (StrLe, [V::Str(x), V::Str(y)]) => V::Bool(x <= y),
            (StrGt, [V::Str(x), V::Str(y)]) => V::Bool(x > y),
            (StrGe, [V::Str(x), V::Str(y)]) => V::Bool(x >= y),
            (BoolEq, [V::Bool(x), V::Bool(y)]) => V::Bool(x == y),
            (BoolShow, [V::Bool(x)]) => V::str(if *x { "true" } else { "false" }),
            (Not, [V::Bool(x)]) => V::Bool(!x),
            (StructEq, [x, y]) => {
                if std::mem::discriminant(x) != std::mem::discriminant(y) {
                    return internal(format!("`==` between a {} and a {}", x.kind(), y.kind()));
                }
                V::Bool(x == y)
            }
            (ListGet, [V::List(xs), V::Int(i)]) => {
                match usize::try_from(*i).ok().and_then(|i| xs.get(i)) {
                    Some(v) => v.clone(),
                    None => {
                        let msg = format!(
                            "index {i} is out of bounds for a list of length {}",
                            xs.len()
                        );
                        return self.panic(msg, sp);
                    }
                }
            }
            (ListLen, [V::List(xs)]) => V::Int(xs.len() as i64),
            (ListConcat, [V::List(xs), V::List(ys)]) => {
                let mut out = xs.to_vec();
                out.extend(ys.iter().cloned());
                V::list(out)
            }
            (ListSlice, [V::List(xs), V::Int(from), V::Int(to)]) => {
                let ok = 0 <= *from && from <= to && (*to as u64) <= xs.len() as u64;
                if !ok {
                    return self.panic("list slice out of bounds", sp);
                }
                V::list(xs[*from as usize..*to as usize].to_vec())
            }
            (MapGet, [V::Map(m), k]) => match m.get(k) {
                Some(v) => v.clone(),
                None => return self.panic("key not found in map", sp),
            },
            (op, args) => {
                let kinds: Vec<&str> = args.iter().map(Value::kind).collect();
                return internal(format!("{op:?} applied to {kinds:?}"));
            }
        })
    }
}

/// `Show` for `Float` is unspecified (spec gap G3). Provisional: the shortest text that
/// reads back as the same value, always with a `.` or an exponent (`1.0`, `0.1`, `1e21`),
/// and `NaN`, `inf`, `-inf`.
pub fn show_float(x: f64) -> String {
    format!("{x:?}")
}

type FoundLambda<'p> = (Vec<LocalId>, &'p Block, Option<LocalId>);

fn collect_lambdas_block<'p>(b: &'p Block, out: &mut Vec<FoundLambda<'p>>) {
    for s in &b.stmts {
        collect_lambdas_stmt(s, out);
    }
}

fn collect_lambdas_stmt<'p>(s: &'p Stmt, out: &mut Vec<FoundLambda<'p>>) {
    match s {
        Stmt::Let { value, .. } | Stmt::Assign { value, .. } => collect_lambdas(value, out),
        Stmt::Expr(e) | Stmt::Return(e) => collect_lambdas(e, out),
        Stmt::If {
            cond,
            then,
            otherwise,
        } => {
            collect_lambdas(cond, out);
            collect_lambdas_block(then, out);
            collect_lambdas_block(otherwise, out);
        }
        Stmt::Switch {
            scrutinee,
            arms,
            default,
        } => {
            collect_lambdas(scrutinee, out);
            for (_, b) in arms {
                collect_lambdas_block(b, out);
            }
            collect_lambdas_block(default, out);
        }
        Stmt::Loop { body } => collect_lambdas_block(body, out),
        Stmt::ForRange { lo, hi, body, .. } => {
            collect_lambdas(lo, out);
            collect_lambdas(hi, out);
            collect_lambdas_block(body, out);
        }
        Stmt::ForEach { iter, body, .. } => {
            collect_lambdas(iter, out);
            collect_lambdas_block(body, out);
        }
        Stmt::Break | Stmt::Continue => {}
    }
}

fn collect_lambdas<'p>(e: &'p kalam_ir::Expr, out: &mut Vec<FoundLambda<'p>>) {
    match &e.kind {
        ExprKind::Lambda {
            params,
            body,
            self_local,
            ..
        } => {
            out.push((params.iter().map(|(l, _)| *l).collect(), body, *self_local));
            collect_lambdas_block(body, out);
        }
        ExprKind::Prim { args: es, .. }
        | ExprKind::Record { fields: es, .. }
        | ExprKind::Variant { fields: es, .. }
        | ExprKind::Tuple(es)
        | ExprKind::List(es)
        | ExprKind::Template { holes: es, .. } => es.iter().for_each(|x| collect_lambdas(x, out)),
        ExprKind::Call { callee, args, .. } => {
            if let Callee::Value(f) = callee {
                collect_lambdas(f, out);
            }
            args.iter().for_each(|x| collect_lambdas(x, out));
        }
        ExprKind::Map(entries) => {
            for (k, v) in entries {
                collect_lambdas(k, out);
                collect_lambdas(v, out);
            }
        }
        ExprKind::Field { base, .. }
        | ExprKind::TupleField { base, .. }
        | ExprKind::IsVariant { base, .. }
        | ExprKind::VariantField { base, .. } => collect_lambdas(base, out),
        ExprKind::If {
            cond,
            then,
            otherwise,
        } => {
            collect_lambdas(cond, out);
            collect_lambdas(then, out);
            collect_lambdas(otherwise, out);
        }
        ExprKind::Block { stmts, value } => {
            stmts.iter().for_each(|s| collect_lambdas_stmt(s, out));
            collect_lambdas(value, out);
        }
        ExprKind::Coerce { value, .. } | ExprKind::Panic(value) => collect_lambdas(value, out),
        ExprKind::Local(_)
        | ExprKind::Int(_)
        | ExprKind::Float(_)
        | ExprKind::Decimal(_)
        | ExprKind::BigInt(_)
        | ExprKind::Str(_)
        | ExprKind::Bool(_)
        | ExprKind::Unit
        | ExprKind::FnRef(_)
        | ExprKind::Const(_) => {}
    }
}
