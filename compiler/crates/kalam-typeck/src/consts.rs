//! Module constants and record field defaults must be constant expressions (spec §13.4,
//! §9.4; FOUNDER-QUEUE Q33, K1204).
//!
//! A **constant expression** is built only from literals (a string's interpolation holes
//! must be constant expressions of a number type, `Bool` or `String`; a tagged literal's
//! holes must be constant expressions), `()`, tuples, list and map literals, record and
//! variant construction (and `.with` on a constant record), lambdas (their bodies are not
//! evaluated), the names of functions, constants, records and variants, field and
//! tuple-field access, and the §10.6 operators other than call, index and `?`, all applied
//! to constant expressions. Calls of functions (stdlib ones included: none is marked
//! `const` yet), method calls, `if`, `match`, indexing, `?` and `unsafe.*` are not.
//!
//! Each constant is then **evaluated** as far as the checker can, with the reference
//! interpreter's rules, so that "a constant that would panic is a compile error" (§13.4)
//! holds for every type (FOUNDER-QUEUE Q40):
//! - `Int` exactly, with the §9.2 rules (checked 64-bit, D18; `//` floors; `%` takes the
//!   divisor's sign; dividing by zero panics, `Int / Int` included);
//! - `BigInt` exactly (no overflow; dividing by zero panics), up to a size bound;
//! - `Decimal` exactly, as decimal128: `+ - *` panic when the exact result needs more than
//!   34 significant digits or leaves the exponent range (`const_num`);
//! - `Float` with IEEE arithmetic (it never panics, but its comparisons decide `and`/`or`),
//!   `Bool`, plain `String` text, `()`;
//! - records (built by their constructor, read by field, copied by `.with`) and tuples (read
//!   by `.0`, `.1`, …), so a value flows through them, and `==`/`!=` on them.
//!
//! A constant whose evaluation would panic is reported at the operation that panics, with
//! the interpreter's panic message. Values flow through other constants in dependency
//! order (the checker checks constants in that order), so `let B = A + 1` overflows when
//! `A` is `Int`'s maximum. What is not evaluated (a field filled from its default, a list,
//! a variant's payload, an interpolated string, a number beyond the size bound) is
//! `Other`: it never makes a panic certain, so no verdict is guessed.
//!
//! A constant that depends on itself (a cycle of constants) has no evaluation order: one
//! K1204 per cycle. A **module constant** also may not hold a capability or a `Cell`
//! anywhere in its type, a function type that takes or returns one included (§13.4 "it
//! cannot take or produce a capability", §15.6). A record field may hold one: a record is
//! built inside a function.
//!
//! One K1204 per constant, at the part that breaks a rule, with the reason as a label.
//! Nothing is reported for an initializer that already has an error (root cause first).

use std::cmp::Ordering;
use std::collections::HashMap;

use kalam_base::{FileId, Span};
use kalam_resolve::DefId;

use crate::adt::AdtKind;
use crate::check::TypeckResults;
use crate::const_num::{Big, Dec, MAX_BIG_BITS};
use crate::error::{TypeError, TypeErrorKind};
use crate::hir::{BinOp, Expr, ExprKind, HirProgram, ItemKind, LangItem, StrPart, UnOp};
use crate::ty::Ty;

/// A constant's value as far as the checker evaluates it.
#[derive(Clone, Debug)]
enum Val {
    Int(i64),
    /// A `BigInt`, exactly.
    Big(Big),
    Dec(Dec),
    Float(f64),
    /// Tracked so `and`/`or` skip what they do not evaluate.
    Bool(bool),
    /// A string's text (literals and `+`; an interpolated string is `Other`).
    Str(String),
    Unit,
    Tuple(Vec<Val>),
    /// A record: the fields given to its constructor (or `.with`), by name. A field left
    /// to its default is absent, so reading it is `Other`.
    Record(Vec<(String, Val)>),
    /// Not evaluated (or a value nothing here inspects): it never panics here.
    Other,
}

/// The most values (tuple and record parts, counted recursively) or string bytes one value
/// may hold before it is `Other`: constants can double a value at each step, and the type
/// checker is a fuzz target.
const MAX_VALUE_SIZE: usize = 4096;

impl Val {
    /// The number of parts of the value, for [`MAX_VALUE_SIZE`].
    fn size(&self) -> usize {
        match self {
            Val::Tuple(xs) => 1 + xs.iter().map(Val::size).sum::<usize>(),
            Val::Record(fs) => 1 + fs.iter().map(|(_, v)| v.size()).sum::<usize>(),
            Val::Str(s) => 1 + s.len(),
            _ => 1,
        }
    }

    /// `self`, or `Other` when it is too large to keep.
    fn bounded(self) -> Val {
        if self.size() > MAX_VALUE_SIZE {
            Val::Other
        } else {
            self
        }
    }

    /// A `BigInt` result, or `Other` beyond the size bound.
    fn big(b: Big) -> Val {
        if b.bits() > MAX_BIG_BITS {
            Val::Other
        } else {
            Val::Big(b)
        }
    }

    /// `self == other` when both are known (`None` otherwise): structural for records and
    /// tuples, by value for `Decimal` (`1.5 == 1.50`), IEEE for `Float`.
    fn equals(&self, other: &Val) -> Option<bool> {
        Some(match (self, other) {
            (Val::Int(x), Val::Int(y)) => x == y,
            (Val::Big(x), Val::Big(y)) => x == y,
            (Val::Dec(x), Val::Dec(y)) => x.cmp_value(y) == Ordering::Equal,
            (Val::Float(x), Val::Float(y)) => x == y,
            (Val::Bool(x), Val::Bool(y)) => x == y,
            (Val::Str(x), Val::Str(y)) => x == y,
            (Val::Unit, Val::Unit) => true,
            (Val::Tuple(xs), Val::Tuple(ys)) if xs.len() == ys.len() => {
                let mut all = true;
                for (x, y) in xs.iter().zip(ys) {
                    all &= x.equals(y)?;
                }
                all
            }
            (Val::Record(xs), Val::Record(ys)) => {
                if xs.len() != ys.len() {
                    return None;
                }
                let mut all = true;
                for (name, x) in xs {
                    let (_, y) = ys.iter().find(|(n, _)| n == name)?;
                    all &= x.equals(y)?;
                }
                all
            }
            _ => return None,
        })
    }

    /// The order of two values of an ordered type, as the interpreter compares them
    /// (numbers, strings, `Bool`, `()`, and tuples lexicographically), or `None` (unknown,
    /// or `NaN` for `Float`). Records are not ordered here.
    fn order(&self, other: &Val) -> Option<Ordering> {
        match (self, other) {
            (Val::Int(x), Val::Int(y)) => Some(x.cmp(y)),
            (Val::Big(x), Val::Big(y)) => Some(x.cmp(y)),
            (Val::Dec(x), Val::Dec(y)) => Some(x.cmp_value(y)),
            (Val::Float(x), Val::Float(y)) => x.partial_cmp(y),
            // UTF-8 byte order is code-point order.
            (Val::Str(x), Val::Str(y)) => Some(x.cmp(y)),
            (Val::Bool(x), Val::Bool(y)) => Some(x.cmp(y)),
            (Val::Unit, Val::Unit) => Some(Ordering::Equal),
            (Val::Tuple(xs), Val::Tuple(ys)) => {
                for (x, y) in xs.iter().zip(ys) {
                    match x.order(y)? {
                        Ordering::Equal => {}
                        o => return Some(o),
                    }
                }
                Some(xs.len().cmp(&ys.len()))
            }
            _ => None,
        }
    }
}

/// Why a constant is not one: the span to point at and the label's text.
type Problem = (Span, String);

/// The K1204 state of one checked program: the value of each constant checked so far
/// (`None` when it was rejected, so its users report nothing more) and the cycles.
pub(crate) struct ConstEval {
    values: HashMap<DefId, Option<Val>>,
    /// Each constant on a dependency cycle, with the cycle's first constant in program
    /// order: the one the cycle is reported at.
    cycle_of: HashMap<DefId, DefId>,
}

impl ConstEval {
    /// Finds the cycles among `prog`'s constants. A reference inside a lambda is not a
    /// dependency: the lambda's body runs only when it is called.
    pub(crate) fn new(prog: &HirProgram) -> ConstEval {
        let consts: Vec<DefId> = prog
            .items()
            .iter()
            .filter(|i| matches!(i.kind, ItemKind::Const(_)))
            .map(|i| i.def)
            .collect();
        let index: HashMap<DefId, usize> =
            consts.iter().enumerate().map(|(i, d)| (*d, i)).collect();
        let edges: Vec<Vec<usize>> = consts
            .iter()
            .map(|d| match prog.item(*d).map(|i| &i.kind) {
                Some(ItemKind::Const(c)) => const_refs(&c.value)
                    .into_iter()
                    .filter_map(|(r, _)| index.get(&r).copied())
                    .collect(),
                _ => Vec::new(),
            })
            .collect();
        let mut cycle_of = HashMap::new();
        for scc in sccs(&edges) {
            let cyclic = scc.len() > 1 || edges[scc[0]].contains(&scc[0]);
            if cyclic {
                // `sccs` sorts each component, so its first index is first in the program.
                let first = consts[scc[0]];
                for i in scc {
                    cycle_of.insert(consts[i], first);
                }
            }
        }
        ConstEval {
            values: HashMap::new(),
            cycle_of,
        }
    }

    /// K1204 for the module constant `def`, whose body has been checked; `clean` is false
    /// when that body reported an error.
    pub(crate) fn constant(
        &mut self,
        prog: &HirProgram,
        res: &TypeckResults,
        def: DefId,
        clean: bool,
    ) -> Option<TypeError> {
        let item = prog.item(def)?;
        let ItemKind::Const(c) = &item.kind else {
            return None;
        };
        let file = item.file;
        if let Some(&first) = self.cycle_of.get(&def) {
            self.values.insert(def, None);
            if !clean || def != first {
                return None;
            }
            let at = const_refs(&c.value)
                .into_iter()
                .find(|(r, _)| self.cycle_of.get(r) == Some(&first))
                .map_or(c.value.span, |(_, s)| s);
            let label = format!("`{}` depends on its own value", item.name);
            return Some(error(file, (at, label)));
        }
        if !clean {
            self.values.insert(def, None);
            return None;
        }
        let ty = res.consts.get(&def).cloned().unwrap_or(Ty::Error);
        let held = forbidden_part(res, &ty).map(|why| {
            let at = c.ty.as_ref().map_or(c.value.span, |t| t.span);
            (at, why)
        });
        let cx = Cx { res, owner: def };
        let outcome = match held.or_else(|| cx.not_constant(&c.value)) {
            Some(p) => Err(p),
            None => cx.eval(&c.value, &self.values),
        };
        match outcome {
            Ok(v) => {
                self.values.insert(def, Some(v));
                None
            }
            Err(p) => {
                self.values.insert(def, None);
                Some(error(file, p))
            }
        }
    }

    /// K1204 for a record field default (`owner` is the record, whose defaults were
    /// checked as one body; `clean` is false when that body reported an error).
    pub(crate) fn field_default(
        &self,
        res: &TypeckResults,
        owner: DefId,
        file: FileId,
        e: &Expr,
        clean: bool,
    ) -> Option<TypeError> {
        if !clean {
            return None;
        }
        let cx = Cx { res, owner };
        cx.not_constant(e)
            .or_else(|| cx.eval(e, &self.values).err())
            .map(|p| error(file, p))
    }
}

fn error(file: FileId, (span, label): Problem) -> TypeError {
    TypeError::new(TypeErrorKind::NotConstant, file, span).with_label(span, label)
}

/// The first capability or `Cell` anywhere in `ty`, as the label's text.
fn forbidden_part(res: &TypeckResults, ty: &Ty) -> Option<String> {
    let mut found = None;
    ty.any(&mut |t| match t {
        Ty::Capability(c) => {
            found = Some(format!(
                "a module constant can't hold the capability `{}`",
                c.name()
            ));
            true
        }
        Ty::Adt(d, _) if is_cell(res, *d) => {
            found = Some(
                "a module constant can't hold a `Cell`: create it in `main` and pass it down"
                    .to_string(),
            );
            true
        }
        _ => false,
    });
    found
}

/// The stdlib's `Cell[T]` (`std/prelude`, spec §15.6).
fn is_cell(res: &TypeckResults, d: DefId) -> bool {
    res.adts
        .get(&d)
        .is_some_and(|a| a.name == "Cell" && a.module.0 == "std/prelude")
}

/// The constants `e` refers to outside lambda bodies, with each reference's span, in
/// source order.
fn const_refs(e: &Expr) -> Vec<(DefId, Span)> {
    let mut out = Vec::new();
    let mut stack = vec![e];
    while let Some(e) = stack.pop() {
        match &e.kind {
            ExprKind::Def(d) => out.push((*d, e.span)),
            ExprKind::Lambda { .. } => {}
            _ => stack.extend(children(e).into_iter().rev()),
        }
    }
    out
}

/// The direct subexpressions of `e` that are evaluated with it (not a lambda's body, not
/// a block's statements).
fn children(e: &Expr) -> Vec<&Expr> {
    match &e.kind {
        ExprKind::Interp(parts) => parts
            .iter()
            .filter_map(|p| match p {
                StrPart::Hole(h) => Some(h),
                StrPart::Text(_) => None,
            })
            .collect(),
        ExprKind::Tagged { holes, .. } => holes.iter().collect(),
        ExprKind::Call { callee, args } => std::iter::once(&**callee)
            .chain(args.iter().map(|a| &a.value))
            .collect(),
        ExprKind::MethodCall { receiver, args, .. } => std::iter::once(&**receiver)
            .chain(args.iter().map(|a| &a.value))
            .collect(),
        ExprKind::Field { base, .. } | ExprKind::TupleField { base, .. } => vec![base],
        ExprKind::Index { base, index } => vec![base, index],
        ExprKind::Unary { operand, .. } => vec![operand],
        ExprKind::Binary { lhs, rhs, .. } => vec![lhs, rhs],
        ExprKind::Range { lo, hi, .. } => vec![lo, hi],
        ExprKind::Tuple(xs) | ExprKind::List(xs) => xs.iter().collect(),
        ExprKind::Map(pairs) => pairs.iter().flat_map(|(k, v)| [k, v]).collect(),
        ExprKind::Try(x) => vec![x],
        ExprKind::If { branches, .. } => branches.iter().map(|b| &b.cond).collect(),
        ExprKind::Match { scrutinee, .. } => vec![scrutinee],
        _ => Vec::new(),
    }
}

struct Cx<'a> {
    res: &'a TypeckResults,
    /// The body that owns the expressions (the constant, or the record of a field default).
    owner: DefId,
}

impl Cx<'_> {
    fn ty(&self, e: &Expr) -> Ty {
        self.res
            .expr_types
            .get(&(self.owner, e.id))
            .cloned()
            .unwrap_or(Ty::Error)
    }

    fn is_record(&self, d: DefId) -> bool {
        matches!(
            self.res.adts.get(&d).map(|a| &a.kind),
            Some(AdtKind::Record(_))
        )
    }

    fn is_variant(&self, d: DefId) -> bool {
        self.res
            .adts
            .values()
            .any(|a| a.variant_by_def(d).is_some())
    }

    /// A constructor: a record, a variant, `Some`, `Ok` or `Err`.
    fn is_constructor(&self, callee: &Expr) -> bool {
        match &callee.kind {
            ExprKind::Def(d) => self.is_record(*d) || self.is_variant(*d),
            ExprKind::ImplicitMember(_) => true,
            ExprKind::Lang(item) => matches!(item, LangItem::Some | LangItem::Ok | LangItem::Err),
            _ => false,
        }
    }

    /// The first part of `e` that is not a constant expression (in evaluation order).
    fn not_constant(&self, e: &Expr) -> Option<Problem> {
        let here = |why: &str| Some((e.span, why.to_string()));
        match &e.kind {
            ExprKind::Int(_)
            | ExprKind::Float(_)
            | ExprKind::Str(_)
            | ExprKind::Bool(_)
            | ExprKind::Unit
            | ExprKind::Def(_)
            | ExprKind::Lang(_)
            | ExprKind::ImplicitMember(_)
            | ExprKind::AssocFn { .. }
            | ExprKind::Lambda { .. }
            | ExprKind::TypeName(_)
            | ExprKind::Error => None,
            ExprKind::Interp(parts) => parts.iter().find_map(|p| match p {
                StrPart::Text(_) => None,
                StrPart::Hole(h) => self.not_constant(h).or_else(|| {
                    let shown = matches!(
                        self.ty(h),
                        Ty::Int
                            | Ty::BigInt
                            | Ty::Float
                            | Ty::Decimal
                            | Ty::Bool
                            | Ty::String
                            | Ty::Error
                    );
                    (!shown).then(|| {
                        (
                            h.span,
                            "a constant can interpolate only numbers, `Bool` and `String` values"
                                .to_string(),
                        )
                    })
                }),
            }),
            ExprKind::Call { callee, args } => {
                if !self.is_constructor(callee) {
                    let why = match callee.kind {
                        ExprKind::Lang(LangItem::Panic) => "a constant can't panic",
                        _ => "a function call is not a constant expression",
                    };
                    return here(why);
                }
                args.iter().find_map(|a| self.not_constant(&a.value))
            }
            ExprKind::MethodCall {
                receiver,
                name,
                args,
            } => {
                let update = name == "with"
                    && matches!(self.ty(receiver), Ty::Adt(d, _) if self.is_record(d));
                if !update {
                    return here("a method call is not a constant expression");
                }
                self.not_constant(receiver)
                    .or_else(|| args.iter().find_map(|a| self.not_constant(&a.value)))
            }
            ExprKind::Index { .. } => here("indexing is not a constant expression"),
            ExprKind::Try(_) => here("`?` is not a constant expression"),
            ExprKind::If { .. } => here("`if` is not a constant expression"),
            ExprKind::Match { .. } => here("`match` is not a constant expression"),
            ExprKind::Unsafe(_) => here("an `unsafe` call is not a constant expression"),
            ExprKind::Local(_) => here("a local value is not a constant expression"),
            ExprKind::Tagged { .. }
            | ExprKind::Field { .. }
            | ExprKind::TupleField { .. }
            | ExprKind::Unary { .. }
            | ExprKind::Binary { .. }
            | ExprKind::Range { .. }
            | ExprKind::Tuple(_)
            | ExprKind::List(_)
            | ExprKind::Map(_) => children(e).into_iter().find_map(|c| self.not_constant(c)),
        }
    }

    /// Evaluates a constant expression (`not_constant` found nothing in it): the value, or
    /// the operation that panics, with the interpreter's message. Constants named in it
    /// have their value in `values`. Parts are evaluated in the interpreter's order: left
    /// to right, a receiver before its arguments, arguments in source order.
    fn eval(&self, e: &Expr, values: &HashMap<DefId, Option<Val>>) -> Result<Val, Problem> {
        let panics = |why: &str| (e.span, format!("this panics: {why}"));
        Ok(match &e.kind {
            // Literals take their type from the context (§5.6), as KIR's lowering does.
            ExprKind::Int(Some(v)) => match self.ty(e) {
                Ty::Int => i64::try_from(*v).map_or(Val::Other, Val::Int),
                Ty::BigInt => Val::big(Big::from_u128(*v)),
                Ty::Decimal => Dec::from_u128(*v).map_or(Val::Other, Val::Dec),
                Ty::Float => Val::Float(*v as f64),
                _ => Val::Other,
            },
            ExprKind::Float(text) => match self.ty(e) {
                Ty::Decimal => Dec::parse(text).map_or(Val::Other, Val::Dec),
                Ty::Float => {
                    let clean: String = text.chars().filter(|c| *c != '_').collect();
                    clean.parse::<f64>().map_or(Val::Other, Val::Float)
                }
                _ => Val::Other,
            },
            ExprKind::Str(s) => Val::Str(s.clone()).bounded(),
            ExprKind::Bool(b) => Val::Bool(*b),
            ExprKind::Unit => Val::Unit,
            ExprKind::Def(d) => values.get(d).cloned().flatten().unwrap_or(Val::Other),
            ExprKind::Unary {
                op: UnOp::Not,
                operand,
            } => match self.eval(operand, values)? {
                Val::Bool(b) => Val::Bool(!b),
                _ => Val::Other,
            },
            // `and`/`or` short-circuit (§10.6): the right side runs only when the left one
            // does not decide. When the left side is not evaluated here, neither is the
            // right side's panic certain.
            ExprKind::Binary {
                op: op @ (BinOp::And | BinOp::Or),
                lhs,
                rhs,
            } => match self.eval(lhs, values)? {
                Val::Bool(b) if b == (*op == BinOp::Or) => Val::Bool(b),
                Val::Bool(_) => self.eval(rhs, values)?,
                _ => self.eval(rhs, values).unwrap_or(Val::Other),
            },
            ExprKind::Unary {
                op: UnOp::Neg,
                operand,
            } => {
                // `-9223372036854775808` is a literal of `Int` (§5.6, D18).
                if let (ExprKind::Int(Some(v)), Ty::Int) = (&operand.kind, self.ty(operand)) {
                    if *v == 1u128 << 63 {
                        return Ok(Val::Int(i64::MIN));
                    }
                }
                match self.eval(operand, values)? {
                    Val::Int(x) => {
                        Val::Int(x.checked_neg().ok_or_else(|| panics("integer overflow"))?)
                    }
                    Val::Big(x) => Val::Big(x.negated()),
                    Val::Dec(x) => Val::Dec(x.negated()),
                    Val::Float(x) => Val::Float(-x),
                    _ => Val::Other,
                }
            }
            ExprKind::Binary { op, lhs, rhs } => {
                let a = self.eval(lhs, values)?;
                let b = self.eval(rhs, values)?;
                binary(*op, &a, &b).map_err(panics)?
            }
            // A record built by its constructor holds the fields given to it; a variant,
            // `Some`, `Ok` or `Err` is not inspected, but its arguments still run.
            ExprKind::Call { callee, args } => {
                let mut fields = Vec::with_capacity(args.len());
                for a in args {
                    let v = self.eval(&a.value, values)?;
                    if let Some(label) = &a.label {
                        fields.push((label.clone(), v));
                    }
                }
                match callee.kind {
                    ExprKind::Def(d) if self.is_record(d) => Val::Record(fields).bounded(),
                    _ => Val::Other,
                }
            }
            // `.with` (the only method call a constant may make): a copy of the record with
            // the fields given replaced.
            ExprKind::MethodCall { receiver, args, .. } => {
                let base = self.eval(receiver, values)?;
                let mut given = Vec::with_capacity(args.len());
                for a in args {
                    let v = self.eval(&a.value, values)?;
                    if let Some(label) = &a.label {
                        given.push((label.clone(), v));
                    }
                }
                match base {
                    Val::Record(mut fields) => {
                        for (label, v) in given {
                            match fields.iter_mut().find(|(n, _)| *n == label) {
                                Some(slot) => slot.1 = v,
                                None => fields.push((label, v)),
                            }
                        }
                        Val::Record(fields).bounded()
                    }
                    _ => Val::Other,
                }
            }
            ExprKind::Field { base, name } => match self.eval(base, values)? {
                Val::Record(fields) => fields
                    .into_iter()
                    .find(|(n, _)| n == name)
                    .map_or(Val::Other, |(_, v)| v),
                _ => Val::Other,
            },
            ExprKind::TupleField { base, index } => match self.eval(base, values)? {
                Val::Tuple(xs) => xs.into_iter().nth(*index as usize).unwrap_or(Val::Other),
                _ => Val::Other,
            },
            ExprKind::Tuple(xs) => {
                let mut out = Vec::with_capacity(xs.len());
                for x in xs {
                    out.push(self.eval(x, values)?);
                }
                Val::Tuple(out).bounded()
            }
            _ => {
                for c in children(e) {
                    self.eval(c, values)?;
                }
                Val::Other
            }
        })
    }
}

/// `a op b` (not `and`/`or`) with the interpreter's rules for the operands' type: the value
/// (`Other` when it is not evaluated) or the panic message.
fn binary(op: BinOp, a: &Val, b: &Val) -> Result<Val, &'static str> {
    if matches!(op, BinOp::Eq | BinOp::NotEq) {
        let eq = a.equals(b);
        return Ok(eq.map_or(Val::Other, |eq| Val::Bool(eq == (op == BinOp::Eq))));
    }
    match (a, b) {
        (Val::Int(x), Val::Int(y)) => int_op(op, *x, *y),
        (Val::Float(x), Val::Float(y)) => Ok(float_op(op, *x, *y)),
        (Val::Big(x), Val::Big(y)) => big_op(op, x, y),
        (Val::Dec(x), Val::Dec(y)) => dec_op(op, x, y),
        (Val::Str(x), Val::Str(y)) if op == BinOp::Add => Ok(Val::Str(format!("{x}{y}")).bounded()),
        // A divisor known to be zero panics whatever the other operand is.
        (_, Val::Int(0)) if matches!(op, BinOp::IntDiv | BinOp::Div) => Err("division by zero"),
        (_, Val::Int(0)) if op == BinOp::Rem => Err("modulo by zero"),
        (_, Val::Big(y)) if y.is_zero() && matches!(op, BinOp::IntDiv | BinOp::Rem) => {
            Err("division by zero")
        }
        _ => Ok(a.order(b).map_or(Val::Other, |o| compare(op, o))),
    }
}

/// The result of the comparison `op` for operands in order `o` (`Other` for any other
/// operator).
fn compare(op: BinOp, o: Ordering) -> Val {
    Val::Bool(match op {
        BinOp::Lt => o == Ordering::Less,
        BinOp::Le => o != Ordering::Greater,
        BinOp::Gt => o == Ordering::Greater,
        BinOp::Ge => o != Ordering::Less,
        _ => return Val::Other,
    })
}

/// `x op y` on two `Int`s with the §9.2 rules: the value (`Float` for `/`) or why it
/// panics.
fn int_op(op: BinOp, x: i64, y: i64) -> Result<Val, &'static str> {
    let overflow = "integer overflow";
    let checked = |r: Option<i64>| r.map(Val::Int).ok_or(overflow);
    match op {
        BinOp::Add => checked(x.checked_add(y)),
        BinOp::Sub => checked(x.checked_sub(y)),
        BinOp::Mul => checked(x.checked_mul(y)),
        BinOp::IntDiv => {
            if y == 0 {
                return Err("division by zero");
            }
            let q = x.checked_div(y).ok_or(overflow)?;
            Ok(Val::Int(if x % y != 0 && ((x < 0) != (y < 0)) {
                q - 1
            } else {
                q
            }))
        }
        BinOp::Rem => {
            if y == 0 {
                return Err("modulo by zero");
            }
            let r = x.wrapping_rem(y);
            Ok(Val::Int(if r != 0 && ((r < 0) != (y < 0)) {
                r + y
            } else {
                r
            }))
        }
        // `Int / Int` is a `Float` (§9.2), and dividing by zero still panics.
        BinOp::Div if y == 0 => Err("division by zero"),
        BinOp::Div => Ok(Val::Float(x as f64 / y as f64)),
        BinOp::Eq => Ok(Val::Bool(x == y)),
        BinOp::NotEq => Ok(Val::Bool(x != y)),
        BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => Ok(compare(op, x.cmp(&y))),
        BinOp::And | BinOp::Or => Ok(Val::Other),
    }
}

/// `x op y` on two `Float`s: IEEE arithmetic and comparisons, which never panic.
fn float_op(op: BinOp, x: f64, y: f64) -> Val {
    match op {
        BinOp::Add => Val::Float(x + y),
        BinOp::Sub => Val::Float(x - y),
        BinOp::Mul => Val::Float(x * y),
        BinOp::Div => Val::Float(x / y),
        BinOp::Lt => Val::Bool(x < y),
        BinOp::Le => Val::Bool(x <= y),
        BinOp::Gt => Val::Bool(x > y),
        BinOp::Ge => Val::Bool(x >= y),
        _ => Val::Other,
    }
}

/// `x op y` on two `BigInt`s: exact, and dividing by zero panics (§9.2).
fn big_op(op: BinOp, x: &Big, y: &Big) -> Result<Val, &'static str> {
    Ok(match op {
        BinOp::Add => Val::big(x.add(y)),
        BinOp::Sub => Val::big(x.sub(y)),
        BinOp::Mul => Val::big(x.mul(y)),
        BinOp::IntDiv | BinOp::Rem => {
            let (q, r) = x.divmod_floor(y).ok_or("division by zero")?;
            Val::big(if op == BinOp::IntDiv { q } else { r })
        }
        _ => compare(op, x.cmp(y)),
    })
}

/// `x op y` on two `Decimal`s: exact, and a result that needs more than 34 significant
/// digits (or leaves the exponent range) panics, as decimal128 arithmetic does (§9.2).
fn dec_op(op: BinOp, x: &Dec, y: &Dec) -> Result<Val, &'static str> {
    let overflow = "Decimal overflow (more than 34 digits)";
    Ok(match op {
        BinOp::Add => Val::Dec(x.add(y).ok_or(overflow)?),
        BinOp::Sub => Val::Dec(x.sub(y).ok_or(overflow)?),
        BinOp::Mul => Val::Dec(x.mul(y).ok_or(overflow)?),
        _ => compare(op, x.cmp_value(y)),
    })
}

/// Strongly connected components of a small graph (Tarjan, iterative), each as node
/// indices.
fn sccs(edges: &[Vec<usize>]) -> Vec<Vec<usize>> {
    let n = edges.len();
    let mut index = vec![usize::MAX; n];
    let mut low = vec![0; n];
    let mut on_stack = vec![false; n];
    let mut stack = Vec::new();
    let mut out = Vec::new();
    let mut next = 0;
    for root in 0..n {
        if index[root] != usize::MAX {
            continue;
        }
        // (node, next edge to look at)
        let mut work = vec![(root, 0usize)];
        index[root] = next;
        low[root] = next;
        next += 1;
        stack.push(root);
        on_stack[root] = true;
        while let Some(&mut (v, ref mut i)) = work.last_mut() {
            if let Some(&w) = edges[v].get(*i) {
                *i += 1;
                if index[w] == usize::MAX {
                    index[w] = next;
                    low[w] = next;
                    next += 1;
                    stack.push(w);
                    on_stack[w] = true;
                    work.push((w, 0));
                } else if on_stack[w] {
                    low[v] = low[v].min(index[w]);
                }
                continue;
            }
            work.pop();
            if let Some(&(parent, _)) = work.last() {
                low[parent] = low[parent].min(low[v]);
            }
            if low[v] == index[v] {
                let mut scc = Vec::new();
                while let Some(w) = stack.pop() {
                    on_stack[w] = false;
                    scc.push(w);
                    if w == v {
                        break;
                    }
                }
                scc.sort_unstable();
                out.push(scc);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A short text for a value, so results compare with `assert_eq!`.
    fn shown(r: Result<Val, &'static str>) -> String {
        match r {
            Ok(Val::Int(i)) => format!("int {i}"),
            Ok(Val::Float(f)) => format!("float {f}"),
            Ok(Val::Bool(b)) => format!("bool {b}"),
            Ok(Val::Other) => "other".to_string(),
            Ok(v) => format!("{v:?}"),
            Err(why) => format!("panic: {why}"),
        }
    }

    #[test]
    fn int_ops_follow_spec_section_9_2() {
        let op = |o, x, y| shown(int_op(o, x, y));
        assert_eq!(op(BinOp::IntDiv, -7, 2), "int -4");
        assert_eq!(op(BinOp::Rem, -7, 3), "int 2");
        assert_eq!(op(BinOp::Rem, 7, -3), "int -2");
        assert_eq!(op(BinOp::Rem, i64::MIN, -1), "int 0");
        assert_eq!(op(BinOp::IntDiv, i64::MIN, -1), "panic: integer overflow");
        assert_eq!(op(BinOp::IntDiv, 1, 0), "panic: division by zero");
        assert_eq!(op(BinOp::Rem, 1, 0), "panic: modulo by zero");
        assert_eq!(op(BinOp::Div, 1, 0), "panic: division by zero");
        assert_eq!(op(BinOp::Div, 7, 2), "float 3.5");
        assert_eq!(op(BinOp::Add, i64::MAX, 1), "panic: integer overflow");
        assert_eq!(op(BinOp::Lt, 1, 2), "bool true");
    }

    #[test]
    fn operations_on_other_types_follow_the_interpreter() {
        let bin = |o, a: Val, b: Val| shown(binary(o, &a, &b));
        // IEEE comparisons are defined for NaN (false), and never panic.
        let nan = f64::NAN;
        assert_eq!(
            bin(BinOp::Lt, Val::Float(nan), Val::Float(1.0)),
            "bool false"
        );
        assert_eq!(
            bin(BinOp::NotEq, Val::Float(nan), Val::Float(nan)),
            "bool true"
        );
        assert_eq!(
            bin(BinOp::Div, Val::Float(1.0), Val::Float(0.0)),
            "float inf"
        );
        // A divisor known to be zero panics even when the dividend is not known.
        assert_eq!(
            bin(BinOp::Rem, Val::Other, Val::Int(0)),
            "panic: modulo by zero"
        );
        assert_eq!(
            bin(BinOp::IntDiv, Val::Other, Val::Big(Big::zero())),
            "panic: division by zero"
        );
        // Unknown operands decide nothing.
        assert_eq!(bin(BinOp::Add, Val::Other, Val::Int(1)), "other");
        assert_eq!(bin(BinOp::Eq, Val::Other, Val::Int(1)), "other");
        // Strings: `+` and code-point order.
        let s = |t: &str| Val::Str(t.to_string());
        assert_eq!(bin(BinOp::Lt, s("a"), s("b")), "bool true");
        assert_eq!(bin(BinOp::Eq, s("ab"), s("ab")), "bool true");
        // Tuples and records compare structurally; a missing field is not known.
        let rec = |x: i64| Val::Record(vec![("x".into(), Val::Int(x))]);
        assert_eq!(bin(BinOp::Eq, rec(1), rec(1)), "bool true");
        assert_eq!(bin(BinOp::NotEq, rec(1), rec(2)), "bool true");
        assert_eq!(bin(BinOp::Eq, rec(1), Val::Record(Vec::new())), "other");
        let tup = Val::Tuple(vec![Val::Int(1), Val::Other]);
        assert_eq!(bin(BinOp::Eq, tup.clone(), tup), "other");
        // Tuples are ordered lexicographically, as the interpreter orders them.
        let pair = |a: i64, b: i64| Val::Tuple(vec![Val::Int(a), Val::Int(b)]);
        assert_eq!(bin(BinOp::Lt, pair(1, 3), pair(1, 2)), "bool false");
        assert_eq!(bin(BinOp::Ge, pair(2, 0), pair(1, 9)), "bool true");
        // Values too large to keep are not kept.
        assert!(matches!(
            Val::Str("x".repeat(MAX_VALUE_SIZE)).bounded(),
            Val::Other
        ));
    }

    #[test]
    fn cycles_are_the_strongly_connected_components_with_an_edge() {
        let edges = [vec![1], vec![0], vec![2], vec![], vec![0]];
        let mut cyclic: Vec<Vec<usize>> = sccs(&edges)
            .into_iter()
            .filter(|c| c.len() > 1 || edges[c[0]].contains(&c[0]))
            .collect();
        cyclic.sort();
        assert_eq!(cyclic, vec![vec![0, 1], vec![2]]);
    }
}
