//! Module constants and record field defaults must be constant expressions (spec §13.4,
//! §9.4; FOUNDER-QUEUE Q33, K1204).
//!
//! A **constant expression** is built only from literals (a string's interpolation holes
//! must be constant expressions of a number type, `Bool` or `String`; a tagged literal's
//! holes must be constant expressions), `()`, tuples, list and map literals, record and
//! variant construction (and `.with` on a constant record), lambdas (their bodies are not
//! evaluated), the names of functions, constants, records and variants, field and
//! tuple-field access, the `pow` methods of `Int`, `Float`, `BigInt` and `Decimal`
//! (FOUNDER-QUEUE Q45, so the K0923 fix of `a ** b` applies to a constant too), and the
//! §10.6 operators other than call, index and `?`, all applied to constant expressions.
//! Calls of functions (stdlib ones included: none is marked `const` yet), other method
//! calls, `if`, `match`, indexing, `?` and `unsafe.*` are not.
//!
//! Each constant is then **evaluated** as far as the checker can, with the reference
//! interpreter's rules, so that "a constant that would panic is a compile error" (§13.4)
//! holds for every type (FOUNDER-QUEUE Q40):
//! - `Int` exactly, with the §9.2 rules (checked 64-bit, D18; `//` floors; `%` takes the
//!   divisor's sign; dividing by zero panics, `Int / Int` included);
//! - `BigInt` exactly (no overflow; dividing by zero panics), up to a size bound;
//! - `Decimal` exactly, as decimal128: `+ - *` and `pow` panic when the exact result needs
//!   more than 34 significant digits, or (a separate message, Q48) when its exponent is
//!   out of range even after clamping (`const_num`);
//! - `Float` with IEEE arithmetic (it never panics, but its comparisons decide `and`/`or`),
//!   `Bool`, plain `String` text, `()`;
//! - records (built by their constructor, read by field, copied by `.with`) and tuples (read
//!   by `.0`, `.1`, …), so a value flows through them;
//! - lists (and `+` on them), maps (a repeated key keeps its first place and takes the last
//!   value, as the interpreter's map does), and enum, `Option` and `Result` values (the
//!   variant and its payload);
//! - `==`/`!=` on all of these structurally, as the interpreter compares them (a map's
//!   order is not part of its value; `Decimal` compares by value): two values that differ
//!   in a known part (a variant, a length, an element) are unequal even when other parts
//!   are not evaluated.
//!
//! A constant whose evaluation would panic is reported at the operation that panics, with
//! the interpreter's panic message. Values flow through other constants in dependency
//! order (the checker checks constants in that order), so `let B = A + 1` overflows when
//! `A` is `Int`'s maximum. What is not evaluated (a field filled from its default, an
//! interpolated string, a map with a key that is not known, a number beyond the size
//! bound) is `Other`: it never makes a panic certain, so no verdict is guessed.
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
use crate::const_num::{Big, BigPow, Dec, BIG_POW_TOO_LARGE, MAX_BIG_BITS};
use crate::error::{TypeError, TypeErrorKind};
use crate::hir::{
    Arg, BinOp, Expr, ExprKind, HirProgram, IntLit, ItemKind, LangItem, StrPart, UnOp,
};
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
    /// A list's elements.
    List(Vec<Val>),
    /// A map's entries in the interpreter's order: a key's first place, its last value.
    /// Built only when every key is known (so keys can be told apart).
    Map(Vec<(Val, Val)>),
    /// An enum, `Option` or `Result` value: the interpreter's variant number (declaration
    /// order; `None` and `Ok` are 0, `Some` and `Err` are 1) and the payload in
    /// declaration order.
    Variant(u32, Vec<Val>),
    /// Not evaluated (or a value nothing here inspects): it never panics here.
    Other,
}

/// `Some(false)` when some pair is known to differ, `None` when some pair is not known,
/// otherwise `Some(true)`: the structural equality of two sequences of one length.
fn all_equal<'a>(pairs: impl Iterator<Item = (&'a Val, &'a Val)>) -> Option<bool> {
    let mut known = true;
    for (x, y) in pairs {
        match x.equals(y) {
            Some(false) => return Some(false),
            Some(true) => {}
            None => known = false,
        }
    }
    known.then_some(true)
}

/// The most values (tuple and record parts, counted recursively) or string bytes one value
/// may hold before it is `Other`: constants can double a value at each step, and the type
/// checker is a fuzz target.
const MAX_VALUE_SIZE: usize = 4096;

impl Val {
    /// The number of parts of the value, for [`MAX_VALUE_SIZE`].
    fn size(&self) -> usize {
        match self {
            Val::Tuple(xs) | Val::List(xs) | Val::Variant(_, xs) => {
                1 + xs.iter().map(Val::size).sum::<usize>()
            }
            Val::Record(fs) => 1 + fs.iter().map(|(_, v)| v.size()).sum::<usize>(),
            Val::Map(es) => 1 + es.iter().map(|(k, v)| k.size() + v.size()).sum::<usize>(),
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

    /// Whether no part of the value is `Other`.
    fn known(&self) -> bool {
        match self {
            Val::Other => false,
            Val::Tuple(xs) | Val::List(xs) | Val::Variant(_, xs) => xs.iter().all(Val::known),
            Val::Record(fs) => fs.iter().all(|(_, v)| v.known()),
            Val::Map(es) => es.iter().all(|(k, v)| k.known() && v.known()),
            _ => true,
        }
    }

    /// `self == other` as the interpreter decides it (`None` when that depends on a part
    /// that is not known): structural for records, tuples, lists, maps (in any order) and
    /// variants, by value for `Decimal` (`1.5 == 1.50`), IEEE for `Float`.
    fn equals(&self, other: &Val) -> Option<bool> {
        Some(match (self, other) {
            (Val::Int(x), Val::Int(y)) => x == y,
            (Val::Big(x), Val::Big(y)) => x == y,
            (Val::Dec(x), Val::Dec(y)) => x.cmp_value(y) == Ordering::Equal,
            (Val::Float(x), Val::Float(y)) => x == y,
            (Val::Bool(x), Val::Bool(y)) => x == y,
            (Val::Str(x), Val::Str(y)) => x == y,
            (Val::Unit, Val::Unit) => true,
            (Val::Tuple(xs), Val::Tuple(ys)) | (Val::List(xs), Val::List(ys)) => {
                if xs.len() != ys.len() {
                    return Some(false);
                }
                return all_equal(xs.iter().zip(ys));
            }
            (Val::Variant(t, xs), Val::Variant(u, ys)) => {
                if t != u || xs.len() != ys.len() {
                    return Some(false);
                }
                return all_equal(xs.iter().zip(ys));
            }
            // A field left to its default is absent: only a given field known to differ
            // decides.
            (Val::Record(xs), Val::Record(ys)) => {
                let shared: Vec<(&Val, &Val)> = xs
                    .iter()
                    .filter_map(|(n, x)| ys.iter().find(|(m, _)| m == n).map(|(_, y)| (x, y)))
                    .collect();
                return match all_equal(shared.iter().copied()) {
                    Some(false) => Some(false),
                    Some(true) if shared.len() == xs.len() && xs.len() == ys.len() => Some(true),
                    _ => None,
                };
            }
            // Keys are known (see `Val::Map`), so each key of one map is found in the
            // other, or not, for certain.
            (Val::Map(xs), Val::Map(ys)) => {
                if xs.len() != ys.len() {
                    return Some(false);
                }
                let mut known = true;
                for (k, x) in xs {
                    match ys.iter().find(|(k2, _)| k.equals(k2) == Some(true)) {
                        None => return Some(false),
                        Some((_, y)) => match x.equals(y) {
                            Some(false) => return Some(false),
                            Some(true) => {}
                            None => known = false,
                        },
                    }
                }
                known
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
        let cx = Cx {
            prog,
            res,
            owner: def,
        };
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
        prog: &HirProgram,
        res: &TypeckResults,
        owner: DefId,
        file: FileId,
        e: &Expr,
        clean: bool,
    ) -> Option<TypeError> {
        if !clean {
            return None;
        }
        let cx = Cx { prog, res, owner };
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
    prog: &'a HirProgram,
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

    /// The variant `d` names: its number in its enum and how many payload fields it has,
    /// with their labels.
    fn variant_of(&self, d: DefId) -> Option<(u32, Vec<Option<String>>)> {
        self.res.adts.values().find_map(|a| {
            a.variant_by_def(d)
                .map(|(i, v)| (i as u32, v.fields.iter().map(|f| f.label.clone()).collect()))
        })
    }

    /// The variant `.name` denotes where `e` (the member, or the call of it) has type `ty`.
    fn implicit_variant(&self, ty: &Ty, name: &str) -> Option<(u32, Vec<Option<String>>)> {
        let Ty::Adt(d, _) = ty else {
            return None;
        };
        let (i, v) = self.res.adts.get(d)?.variant(name)?;
        Some((i as u32, v.fields.iter().map(|f| f.label.clone()).collect()))
    }

    /// Whether the method call `e` (`x.pow(k)`) calls the stdlib's `pow` of a number type
    /// (Q45): the method the checker picked, not just one with that name. Its body is not
    /// looked at: single-file mode drops the stdlib's bodies, while project mode keeps the
    /// `@builtin` stub's `panic("builtin")`, and both modes must agree.
    fn is_number_pow(&self, e: &Expr, receiver: &Expr, name: &str) -> bool {
        if name != "pow"
            || !matches!(
                self.ty(receiver),
                Ty::Int | Ty::Float | Ty::BigInt | Ty::Decimal
            )
        {
            return false;
        }
        let Some(pick) = self.res.method_picks.get(&(self.owner, e.id)) else {
            return false;
        };
        self.prog.item(*pick).is_some_and(|item| {
            item.name == "pow"
                && matches!(
                    item.module.0.as_str(),
                    "std/prelude" | "std/bigint" | "std/decimal"
                )
                && matches!(&item.kind, ItemKind::Fn(_))
        })
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
                if !update && !self.is_number_pow(e, receiver, name) {
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
            ExprKind::Int(IntLit::Small(v)) => match self.ty(e) {
                Ty::Int => i64::try_from(*v).map_or(Val::Other, Val::Int),
                Ty::BigInt => Val::big(Big::from_u128(*v)),
                Ty::Decimal => Dec::from_u128(*v).map_or(Val::Other, Val::Dec),
                Ty::Float => Val::Float(*v as f64),
                _ => Val::Other,
            },
            // Beyond `u128`: a `BigInt` past the size bound, or a `Float`/`Decimal` value
            // read from its digits (an `Int` is K0126, and is not evaluated).
            ExprKind::Int(IntLit::Large(digits)) => match self.ty(e) {
                Ty::Decimal => match Dec::parse(digits) {
                    Some(Ok(d)) => Val::Dec(d),
                    _ => Val::Other,
                },
                Ty::Float => digits.parse::<f64>().map_or(Val::Other, Val::Float),
                _ => Val::Other,
            },
            ExprKind::Float(text) => match self.ty(e) {
                Ty::Decimal => match Dec::parse(text) {
                    Some(Ok(d)) => Val::Dec(d),
                    _ => Val::Other,
                },
                Ty::Float => {
                    let clean: String = text.chars().filter(|c| *c != '_').collect();
                    clean.parse::<f64>().map_or(Val::Other, Val::Float)
                }
                _ => Val::Other,
            },
            ExprKind::Str(s) => Val::Str(s.clone()).bounded(),
            ExprKind::Bool(b) => Val::Bool(*b),
            ExprKind::Unit => Val::Unit,
            ExprKind::Def(d) => match self.variant_of(*d) {
                // A payload-free variant is a value; one with a payload is a function.
                Some((tag, fields)) if fields.is_empty() => Val::Variant(tag, Vec::new()),
                Some(_) => Val::Other,
                None => values.get(d).cloned().flatten().unwrap_or(Val::Other),
            },
            ExprKind::Lang(LangItem::None) => Val::Variant(0, Vec::new()),
            ExprKind::ImplicitMember(name) => match self.implicit_variant(&self.ty(e), name) {
                Some((tag, fields)) if fields.is_empty() => Val::Variant(tag, Vec::new()),
                _ => Val::Other,
            },
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
                if let (ExprKind::Int(IntLit::Small(v)), Ty::Int) =
                    (&operand.kind, self.ty(operand))
                {
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
            // `Some`, `Ok` or `Err` holds its payload in declaration order. Arguments run in
            // source order.
            ExprKind::Call { callee, args } => {
                let mut given = Vec::with_capacity(args.len());
                for a in args {
                    given.push(self.eval(&a.value, values)?);
                }
                let variant = match &callee.kind {
                    ExprKind::Def(d) if self.is_record(*d) => {
                        let fields = args
                            .iter()
                            .zip(given)
                            .filter_map(|(a, v)| a.label.clone().map(|l| (l, v)))
                            .collect();
                        return Ok(Val::Record(fields).bounded());
                    }
                    ExprKind::Lang(LangItem::Some | LangItem::Err) => Some((1, vec![None])),
                    ExprKind::Lang(LangItem::Ok) => Some((0, vec![None])),
                    ExprKind::Def(d) => self.variant_of(*d),
                    ExprKind::ImplicitMember(name) => self.implicit_variant(&self.ty(e), name),
                    _ => None,
                };
                match variant
                    .and_then(|(tag, labels)| payload(&labels, args, given).map(|p| (tag, p)))
                {
                    Some((tag, payload)) => Val::Variant(tag, payload).bounded(),
                    None => Val::Other,
                }
            }
            // `x.pow(k)` on a number type (Q45), with the interpreter's rules and messages.
            ExprKind::MethodCall {
                receiver,
                name,
                args,
            } if self.is_number_pow(e, receiver, name) => {
                let base = self.eval(receiver, values)?;
                let mut k = Val::Other;
                for a in args {
                    k = self.eval(&a.value, values)?;
                }
                pow(&self.ty(receiver), &base, &k).map_err(|why| panics(&why))?
            }
            // `.with` (the only other method call a constant may make): a copy of the record
            // with the fields given replaced.
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
            ExprKind::List(xs) => {
                let mut out = Vec::with_capacity(xs.len());
                for x in xs {
                    out.push(self.eval(x, values)?);
                }
                Val::List(out).bounded()
            }
            // Each key, then its value, in source order (as the interpreter builds a map).
            ExprKind::Map(pairs) => {
                let mut out: Vec<(Val, Val)> = Vec::with_capacity(pairs.len());
                let mut keys_known = true;
                for (k, v) in pairs {
                    let k = self.eval(k, values)?;
                    let v = self.eval(v, values)?;
                    keys_known &= k.known();
                    if !keys_known {
                        continue;
                    }
                    match out.iter_mut().find(|(k2, _)| k.equals(k2) == Some(true)) {
                        Some(slot) => slot.1 = v,
                        None => out.push((k, v)),
                    }
                }
                if keys_known {
                    Val::Map(out).bounded()
                } else {
                    Val::Other
                }
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
        (Val::List(x), Val::List(y)) if op == BinOp::Add => {
            Ok(Val::List(x.iter().chain(y).cloned().collect()).bounded())
        }
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
/// digits, or whose exponent leaves the range even after clamping, panics with the
/// interpreter's message for that case (§9.2, Q48).
fn dec_op(op: BinOp, x: &Dec, y: &Dec) -> Result<Val, &'static str> {
    let held = |r: Result<Dec, crate::const_num::DecFail>| r.map(Val::Dec).map_err(|f| f.message());
    match op {
        BinOp::Add => held(x.add(y)),
        BinOp::Sub => held(x.sub(y)),
        BinOp::Mul => held(x.mul(y)),
        _ => Ok(compare(op, x.cmp_value(y))),
    }
}

/// A variant's payload in declaration order from a call's arguments (`labels` are the
/// fields' labels; a positional argument takes the next field). `None` when they do not
/// line up (the checker reported it).
fn payload(labels: &[Option<String>], args: &[Arg], given: Vec<Val>) -> Option<Vec<Val>> {
    if labels.len() != args.len() {
        return None;
    }
    let mut out: Vec<Option<Val>> = vec![None; labels.len()];
    for (i, (a, v)) in args.iter().zip(given).enumerate() {
        let at = match &a.label {
            Some(l) => labels
                .iter()
                .position(|x| x.as_deref() == Some(l.as_str()))?,
            None => i,
        };
        if out[at].replace(v).is_some() {
            return None;
        }
    }
    out.into_iter().collect()
}

/// `base.pow(k)` for a receiver of type `ty` (Q45), as the interpreter computes it: the
/// value (`Other` when not evaluated) or the panic message. Every `pow` but `Float`'s
/// checks the exponent first, so a negative one panics whatever the base is.
fn pow(ty: &Ty, base: &Val, k: &Val) -> Result<Val, String> {
    let negative = |owner: &str, k: i64| format!("negative exponent {k} in `{owner}.pow`");
    Ok(match (ty, base, k) {
        (Ty::Float, Val::Float(x), Val::Float(y)) => Val::Float(x.powf(*y)),
        (Ty::Int, _, Val::Int(k)) if *k < 0 => return Err(negative("Int", *k)),
        (Ty::BigInt, _, Val::Int(k)) if *k < 0 => return Err(negative("BigInt", *k)),
        (Ty::Decimal, _, Val::Int(k)) if *k < 0 => return Err(negative("Decimal", *k)),
        (Ty::Int, Val::Int(x), Val::Int(k)) => match int_pow(*x, *k) {
            Some(v) => Val::Int(v),
            None => return Err("integer overflow".to_string()),
        },
        (Ty::BigInt, Val::Big(x), Val::Int(k)) => match x.pow(*k) {
            BigPow::Value(v) => Val::big(v),
            BigPow::TooLarge => return Err(BIG_POW_TOO_LARGE.to_string()),
            BigPow::NegativeExponent | BigPow::Unknown => Val::Other,
        },
        (Ty::Decimal, Val::Dec(x), Val::Int(k)) => match x.pow(*k) {
            Some(Ok(d)) => Val::Dec(d),
            Some(Err(f)) => return Err(f.message().to_string()),
            None => Val::Other,
        },
        _ => Val::Other,
    })
}

/// `Int.pow` for a non-negative exponent (the interpreter's `num::int_pow`): `None` on
/// overflow. The bases 0, 1 and −1 take any exponent.
fn int_pow(base: i64, k: i64) -> Option<i64> {
    match base {
        0 => return Some(if k == 0 { 1 } else { 0 }),
        1 => return Some(1),
        -1 => return Some(if k % 2 == 0 { 1 } else { -1 }),
        _ => {}
    }
    base.checked_pow(u32::try_from(k).ok()?)
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
