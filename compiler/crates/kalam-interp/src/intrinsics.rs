//! The stdlib's `@builtin` functions on prelude types, as the M2 interpreter provides them
//! (`Callee::Intrinsic` keys: `Type.method` or `module.function`, receiver first).
//!
//! The table is the single source of truth: [`is_intrinsic`] and
//! [`unsupported_intrinsics`] read it, so a program is refused before it runs when it calls
//! a stdlib function that does not run until M4 (files, network, time, randomness, JSON,
//! graphemes, NFC, `AnyError`).
//!
//! Choices where the stdlib docs are silent (provisional; listed for the founder):
//! - `String.split("")` and `String.replace("", …)` panic (an empty separator has no
//!   defined meaning).
//! - `List.take`/`List.skip` with a negative count panic; a count past the end is clamped.
//! - `List.sort_by` is a stable merge sort; `math.round` rounds halves away from zero
//!   (stdlib doc); `math.clamp` with `low > high` panics.
//! - `String.to_float` accepts `-`, ASCII digits, an optional fraction and exponent (no
//!   `inf`/`NaN` text); a value too large for `Float` is `OutOfRange`.

use std::cmp::Ordering;
use std::rc::Rc;

use kalam_base::Span;
use kalam_ir::{Callee, ExprKind, Program, Stmt};

use crate::num::{self, BigInt, Decimal, ParseFail, Rounding};
use crate::value::{compare, OrderedMap, Value};
use crate::{internal, Ctrl, Ev, Interp, InterpError};

type Intrinsic = fn(&mut Interp<'_, '_>, Vec<Value>, Span) -> Ev<Value>;

/// Every intrinsic the interpreter provides.
const TABLE: &[(&str, Intrinsic)] = &[
    // Int (std/prelude)
    ("Int.to_float", int_to_float),
    ("Int.to_string", int_to_string),
    ("Int.abs", int_abs),
    ("Int.checked_add", int_checked_add),
    ("Int.checked_sub", int_checked_sub),
    ("Int.checked_mul", int_checked_mul),
    ("Int.wrapping_add", int_wrapping_add),
    ("Int.saturating_add", int_saturating_add),
    ("Int.pow", int_pow),
    ("Float.pow", math_pow),
    ("Int.bit_and", int_bit_and),
    ("Int.bit_or", int_bit_or),
    ("Int.bit_xor", int_bit_xor),
    ("Int.shift_left", int_shift_left),
    ("Int.shift_right", int_shift_right),
    // Float (std/prelude)
    ("Float.to_int", float_to_int),
    ("Float.total_cmp", float_total_cmp),
    ("Float.near", float_near),
    ("Float.is_nan", float_is_nan),
    ("Float.abs", float_abs),
    // String (std/string)
    ("String.len_chars", str_len_chars),
    ("String.byte_len", str_byte_len),
    ("String.is_empty", str_is_empty),
    ("String.chars", str_chars),
    ("String.split", str_split),
    ("String.join", str_join),
    ("String.trim", str_trim),
    ("String.starts_with", str_starts_with),
    ("String.ends_with", str_ends_with),
    ("String.contains", str_contains),
    ("String.replace", str_replace),
    ("String.to_upper", str_to_upper),
    ("String.to_lower", str_to_lower),
    ("String.arabic_fold", str_arabic_fold),
    ("String.to_int", str_to_int),
    ("String.to_float", str_to_float),
    ("String.to_decimal", str_to_decimal),
    // List (std/list)
    ("List.len", list_len),
    ("List.is_empty", list_is_empty),
    ("List.get", list_get),
    ("List.set", list_set),
    ("List.append", list_append),
    ("List.insert", list_insert),
    ("List.remove", list_remove),
    ("List.map", list_map),
    ("List.filter", list_filter),
    ("List.fold", list_fold),
    ("List.find", list_find),
    ("List.any", list_any),
    ("List.all", list_all),
    ("List.sort_by", list_sort_by),
    ("List.enumerate", list_enumerate),
    ("List.zip", list_zip),
    ("List.take", list_take),
    ("List.skip", list_skip),
    ("List.first", list_first),
    ("List.last", list_last),
    ("List.group_by", list_group_by),
    ("List.contains", list_contains),
    ("List.sorted", list_sorted),
    ("List.sum", list_sum),
    // Map (std/map)
    ("Map.len", map_len),
    ("Map.is_empty", map_is_empty),
    ("Map.get", map_get),
    ("Map.contains_key", map_contains_key),
    ("Map.insert", map_insert),
    ("Map.remove", map_remove),
    ("Map.keys", map_keys),
    ("Map.values", map_values),
    ("Map.map", map_map),
    ("Map.filter", map_filter),
    // Set (std/set)
    ("Set.of", set_of),
    ("Set.len", map_len),
    ("Set.is_empty", map_is_empty),
    ("Set.contains", map_contains_key),
    ("Set.insert", set_insert),
    ("Set.remove", set_remove),
    ("Set.union", set_union),
    ("Set.intersection", set_intersection),
    ("Set.to_list", map_keys),
    // Option (std/option)
    ("Option.map", option_map),
    ("Option.or", option_or),
    ("Option.ok_or", option_ok_or),
    ("Option.expect", option_expect),
    ("Option.is_some", option_is_some),
    ("Option.is_none", option_is_none),
    // Result (std/result)
    ("Result.map", result_map),
    ("Result.map_err", result_map_err),
    ("Result.ok", result_ok),
    ("Result.expect", result_expect),
    ("Result.is_ok", option_is_none),
    ("Result.is_err", option_is_some),
    // BigInt and Decimal (std/bigint, std/decimal)
    ("BigInt.from", big_from),
    ("BigInt.parse", big_parse),
    ("BigInt.to_int", big_to_int),
    ("Decimal.from", dec_from),
    ("Decimal.parse", str_to_decimal),
    ("Decimal.div", dec_div),
    ("Decimal.round", dec_round),
    // Secret (std/prelude): the wrapped value itself; only `Show` differs (lowering).
    ("Secret.new", identity),
    ("Secret.expose", identity),
    // std/math
    ("std/math.abs", int_abs),
    ("std/math.min", math_min),
    ("std/math.max", math_max),
    ("std/math.clamp", math_clamp),
    ("std/math.sqrt", math_sqrt),
    ("std/math.floor", math_floor),
    ("std/math.round", math_round),
    ("std/math.pow", math_pow),
];

fn lookup(key: &str) -> Option<Intrinsic> {
    TABLE.iter().find(|(k, _)| *k == key).map(|(_, f)| *f)
}

pub fn is_intrinsic(key: &str) -> bool {
    lookup(key).is_some()
}

/// What `program` uses that this interpreter does not provide, sorted: stdlib intrinsics
/// (`` `String.graphemes` ``) and tagged literals (`` `sql"…"` literals ``). A run is
/// refused before it starts when this is not empty, so no output is printed first.
pub fn unsupported_intrinsics(program: &Program) -> Vec<String> {
    let mut out = Vec::new();
    let mut visit = |e: &kalam_ir::Expr| match &e.kind {
        ExprKind::Call {
            callee: Callee::Intrinsic(k),
            ..
        } if !is_intrinsic(k) => out.push(format!("`{k}`")),
        ExprKind::Template { tag, .. } => out.push(format!("`{tag}\"…\"` literals")),
        _ => {}
    };
    for f in &program.functions {
        walk_block(&f.body, &mut visit);
    }
    for c in &program.consts {
        walk_expr(&c.value, &mut visit);
    }
    out.sort();
    out.dedup();
    out
}

fn walk_block(b: &kalam_ir::Block, f: &mut dyn FnMut(&kalam_ir::Expr)) {
    walk_stmts(&b.stmts, f);
}

fn walk_stmts(stmts: &[Stmt], f: &mut dyn FnMut(&kalam_ir::Expr)) {
    for s in stmts {
        match s {
            Stmt::Let { value, .. } | Stmt::Assign { value, .. } => walk_expr(value, f),
            Stmt::Expr(e) | Stmt::Return(e) => walk_expr(e, f),
            Stmt::If {
                cond,
                then,
                otherwise,
            } => {
                walk_expr(cond, f);
                walk_block(then, f);
                walk_block(otherwise, f);
            }
            Stmt::Switch {
                scrutinee,
                arms,
                default,
            } => {
                walk_expr(scrutinee, f);
                arms.iter().for_each(|(_, b)| walk_block(b, f));
                walk_block(default, f);
            }
            Stmt::Loop { body } => walk_block(body, f),
            Stmt::ForRange { lo, hi, body, .. } => {
                walk_expr(lo, f);
                walk_expr(hi, f);
                walk_block(body, f);
            }
            Stmt::ForEach { iter, body, .. } => {
                walk_expr(iter, f);
                walk_block(body, f);
            }
            Stmt::Break | Stmt::Continue => {}
        }
    }
}

fn walk_expr(e: &kalam_ir::Expr, f: &mut dyn FnMut(&kalam_ir::Expr)) {
    f(e);
    match &e.kind {
        ExprKind::Prim { args: es, .. }
        | ExprKind::Record { fields: es, .. }
        | ExprKind::Variant { fields: es, .. }
        | ExprKind::Tuple(es)
        | ExprKind::List(es)
        | ExprKind::Template { holes: es, .. } => es.iter().for_each(|x| walk_expr(x, f)),
        ExprKind::Call { callee, args, .. } => {
            if let Callee::Value(c) = callee {
                walk_expr(c, f);
            }
            args.iter().for_each(|x| walk_expr(x, f));
        }
        ExprKind::Map(entries) => {
            for (k, v) in entries {
                walk_expr(k, f);
                walk_expr(v, f);
            }
        }
        ExprKind::Field { base, .. }
        | ExprKind::TupleField { base, .. }
        | ExprKind::IsVariant { base, .. }
        | ExprKind::VariantField { base, .. } => walk_expr(base, f),
        ExprKind::If {
            cond,
            then,
            otherwise,
        } => {
            walk_expr(cond, f);
            walk_expr(then, f);
            walk_expr(otherwise, f);
        }
        ExprKind::Block { stmts, value } => {
            walk_stmts(stmts, f);
            walk_expr(value, f);
        }
        ExprKind::Lambda { body, .. } => walk_block(body, f),
        ExprKind::Coerce { value, .. } | ExprKind::Panic(value) => walk_expr(value, f),
        _ => {}
    }
}

impl Interp<'_, '_> {
    pub(crate) fn intrinsic(&mut self, key: &str, args: Vec<Value>, span: Span) -> Ev<Value> {
        match lookup(key) {
            Some(f) => f(self, args, span),
            None => Err(Ctrl::Err(InterpError::Unsupported(format!(
                "`{key}` is not available in the M2 reference interpreter"
            )))),
        }
    }

    /// A value of the stdlib enum `module.name`, variant `variant`.
    fn std_variant(
        &self,
        module: &str,
        name: &str,
        variant: &str,
        fields: Vec<Value>,
    ) -> Ev<Value> {
        let Some(e) = self.program().enum_named(module, name) else {
            return internal(format!("the stdlib enum {module}.{name} is not loaded"));
        };
        match e.variants.iter().position(|v| v.name == variant) {
            Some(i) => Ok(Value::variant(i as u32, fields)),
            None => internal(format!("{name} has no variant {variant}")),
        }
    }

    fn ordering(&self, o: Ordering) -> Ev<Value> {
        let v = match o {
            Ordering::Less => "Less",
            Ordering::Equal => "Equal",
            Ordering::Greater => "Greater",
        };
        self.std_variant("std/prelude", "Ordering", v, Vec::new())
    }

    fn parse_result(&self, r: Result<Value, ParseFail>, text: &str) -> Ev<Value> {
        Ok(match r {
            Ok(v) => Value::ok(v),
            Err(fail) => {
                let (v, fields) = match fail {
                    ParseFail::Empty => ("Empty", Vec::new()),
                    ParseFail::Invalid => ("Invalid", vec![Value::str(text)]),
                    ParseFail::OutOfRange => ("OutOfRange", vec![Value::str(text)]),
                };
                Value::err(self.std_variant("std/string", "ParseError", v, fields)?)
            }
        })
    }

    fn call1(&mut self, f: &Value, a: Value, span: Span) -> Ev<Value> {
        self.call_value(f, vec![a], span)
    }

    fn predicate(&mut self, f: &Value, a: Value, span: Span) -> Ev<bool> {
        match self.call1(f, a, span)? {
            Value::Bool(b) => Ok(b),
            other => internal(format!("predicate returned a {}", other.kind())),
        }
    }
}

// -- argument shapes -------------------------------------------------------------------------

fn bad<T>(name: &str, args: &[Value]) -> Ev<T> {
    let kinds: Vec<&str> = args.iter().map(Value::kind).collect();
    internal(format!("{name} applied to {kinds:?}"))
}

macro_rules! args {
    ($args:expr, $name:literal, [$($pat:pat),*] => $body:expr) => {
        match $args.as_slice() {
            [$($pat),*] => $body,
            _ => return bad($name, &$args),
        }
    };
}

fn int(v: i64) -> Ev<Value> {
    Ok(Value::Int(v))
}

fn boolean(b: bool) -> Ev<Value> {
    Ok(Value::Bool(b))
}

fn string(s: &str) -> Ev<Value> {
    Ok(Value::str(s))
}

fn option(v: Option<Value>) -> Value {
    match v {
        Some(v) => Value::some(v),
        None => Value::none(),
    }
}

fn list(items: Vec<Value>) -> Ev<Value> {
    Ok(Value::list(items))
}

/// A count argument (`take`, `skip`): negative panics, past the end is clamped.
fn count(it: &Interp<'_, '_>, n: i64, len: usize, span: Span) -> Ev<usize> {
    if n < 0 {
        return it.panic(format!("negative count {n}"), span);
    }
    Ok((n as u64).min(len as u64) as usize)
}

fn index(it: &Interp<'_, '_>, i: i64, len: usize, span: Span) -> Ev<usize> {
    match usize::try_from(i) {
        Ok(u) if u < len => Ok(u),
        _ => it.panic(
            format!("index {i} is out of bounds for a list of length {len}"),
            span,
        ),
    }
}

// -- Int and Float ---------------------------------------------------------------------------

fn int_to_float(_: &mut Interp<'_, '_>, a: Vec<Value>, _: Span) -> Ev<Value> {
    args!(a, "Int.to_float", [Value::Int(x)] => Ok(Value::Float(*x as f64)))
}

fn int_to_string(_: &mut Interp<'_, '_>, a: Vec<Value>, _: Span) -> Ev<Value> {
    args!(a, "Int.to_string", [Value::Int(x)] => string(&x.to_string()))
}

fn int_abs(it: &mut Interp<'_, '_>, a: Vec<Value>, sp: Span) -> Ev<Value> {
    args!(a, "Int.abs", [Value::Int(x)] => match x.checked_abs() {
        Some(r) => int(r),
        None => it.panic("integer overflow", sp),
    })
}

fn checked(a: Vec<Value>, name: &str, f: fn(i64, i64) -> Option<i64>) -> Ev<Value> {
    match a.as_slice() {
        [Value::Int(x), Value::Int(y)] => Ok(option(f(*x, *y).map(Value::Int))),
        _ => bad(name, &a),
    }
}

fn int_checked_add(_: &mut Interp<'_, '_>, a: Vec<Value>, _: Span) -> Ev<Value> {
    checked(a, "Int.checked_add", i64::checked_add)
}

fn int_checked_sub(_: &mut Interp<'_, '_>, a: Vec<Value>, _: Span) -> Ev<Value> {
    checked(a, "Int.checked_sub", i64::checked_sub)
}

fn int_checked_mul(_: &mut Interp<'_, '_>, a: Vec<Value>, _: Span) -> Ev<Value> {
    checked(a, "Int.checked_mul", i64::checked_mul)
}

fn int_wrapping_add(_: &mut Interp<'_, '_>, a: Vec<Value>, _: Span) -> Ev<Value> {
    args!(a, "Int.wrapping_add", [Value::Int(x), Value::Int(y)] => int(x.wrapping_add(*y)))
}

fn int_saturating_add(_: &mut Interp<'_, '_>, a: Vec<Value>, _: Span) -> Ev<Value> {
    args!(a, "Int.saturating_add", [Value::Int(x), Value::Int(y)] => int(x.saturating_add(*y)))
}

fn int_pow(it: &mut Interp<'_, '_>, a: Vec<Value>, sp: Span) -> Ev<Value> {
    args!(a, "Int.pow", [Value::Int(x), Value::Int(e)] => match num::int_pow(*x, *e) {
        Ok(r) => int(r),
        Err(num::PowFail::Overflow) => it.panic("integer overflow", sp),
        Err(num::PowFail::NegativeExponent) => {
            it.panic(format!("negative exponent {e} in `Int.pow`"), sp)
        }
    })
}

fn int_bit_and(_: &mut Interp<'_, '_>, a: Vec<Value>, _: Span) -> Ev<Value> {
    args!(a, "Int.bit_and", [Value::Int(x), Value::Int(y)] => int(x & y))
}

fn int_bit_or(_: &mut Interp<'_, '_>, a: Vec<Value>, _: Span) -> Ev<Value> {
    args!(a, "Int.bit_or", [Value::Int(x), Value::Int(y)] => int(x | y))
}

fn int_bit_xor(_: &mut Interp<'_, '_>, a: Vec<Value>, _: Span) -> Ev<Value> {
    args!(a, "Int.bit_xor", [Value::Int(x), Value::Int(y)] => int(x ^ y))
}

/// `shift_left`/`shift_right`: the amount must be in `0..=63` (spec §5.9).
fn shift(it: &Interp<'_, '_>, a: Vec<Value>, sp: Span, left: bool) -> Ev<Value> {
    let name = if left {
        "Int.shift_left"
    } else {
        "Int.shift_right"
    };
    let [Value::Int(x), Value::Int(by)] = a.as_slice() else {
        return bad(name, &a);
    };
    match num::shift_amount(*by) {
        // Two's complement: `<<` drops the bits shifted out, `>>` is arithmetic on `i64`.
        Some(n) if left => int(x << n),
        Some(n) => int(x >> n),
        None => it.panic(
            format!("shift amount {by} is outside 0..=63 in `{name}`"),
            sp,
        ),
    }
}

fn int_shift_left(it: &mut Interp<'_, '_>, a: Vec<Value>, sp: Span) -> Ev<Value> {
    shift(it, a, sp, true)
}

fn int_shift_right(it: &mut Interp<'_, '_>, a: Vec<Value>, sp: Span) -> Ev<Value> {
    shift(it, a, sp, false)
}

fn float_to_int(_: &mut Interp<'_, '_>, a: Vec<Value>, _: Span) -> Ev<Value> {
    args!(a, "Float.to_int", [Value::Float(x)] => Ok(option(num::float_to_int(*x).map(Value::Int))))
}

fn float_total_cmp(it: &mut Interp<'_, '_>, a: Vec<Value>, _: Span) -> Ev<Value> {
    args!(a, "Float.total_cmp", [Value::Float(x), Value::Float(y)] => it.ordering(x.total_cmp(y)))
}

fn float_near(_: &mut Interp<'_, '_>, a: Vec<Value>, _: Span) -> Ev<Value> {
    args!(a, "Float.near", [Value::Float(x), Value::Float(y), Value::Float(t)] => boolean((x - y).abs() <= *t))
}

fn float_is_nan(_: &mut Interp<'_, '_>, a: Vec<Value>, _: Span) -> Ev<Value> {
    args!(a, "Float.is_nan", [Value::Float(x)] => boolean(x.is_nan()))
}

fn float_abs(_: &mut Interp<'_, '_>, a: Vec<Value>, _: Span) -> Ev<Value> {
    args!(a, "Float.abs", [Value::Float(x)] => Ok(Value::Float(x.abs())))
}

// -- String ----------------------------------------------------------------------------------

fn str_len_chars(_: &mut Interp<'_, '_>, a: Vec<Value>, _: Span) -> Ev<Value> {
    args!(a, "String.len_chars", [Value::Str(s)] => int(s.chars().count() as i64))
}

fn str_byte_len(_: &mut Interp<'_, '_>, a: Vec<Value>, _: Span) -> Ev<Value> {
    args!(a, "String.byte_len", [Value::Str(s)] => int(s.len() as i64))
}

fn str_is_empty(_: &mut Interp<'_, '_>, a: Vec<Value>, _: Span) -> Ev<Value> {
    args!(a, "String.is_empty", [Value::Str(s)] => boolean(s.is_empty()))
}

fn str_chars(_: &mut Interp<'_, '_>, a: Vec<Value>, _: Span) -> Ev<Value> {
    args!(a, "String.chars", [Value::Str(s)] => list(s.chars().map(|c| Value::str(c.encode_utf8(&mut [0; 4]))).collect()))
}

fn str_split(it: &mut Interp<'_, '_>, a: Vec<Value>, sp: Span) -> Ev<Value> {
    args!(a, "String.split", [Value::Str(s), Value::Str(sep)] => {
        if sep.is_empty() {
            return it.panic("`split` with an empty separator", sp);
        }
        list(s.split(&**sep).map(Value::str).collect())
    })
}

fn str_join(_: &mut Interp<'_, '_>, a: Vec<Value>, _: Span) -> Ev<Value> {
    args!(a, "String.join", [Value::List(parts), Value::Str(sep)] => {
        let mut out = String::new();
        for (i, p) in parts.iter().enumerate() {
            let Value::Str(p) = p else { return bad("String.join", &a) };
            if i > 0 {
                out.push_str(sep);
            }
            out.push_str(p);
        }
        string(&out)
    })
}

fn str_trim(_: &mut Interp<'_, '_>, a: Vec<Value>, _: Span) -> Ev<Value> {
    args!(a, "String.trim", [Value::Str(s)] => string(s.trim()))
}

fn str_starts_with(_: &mut Interp<'_, '_>, a: Vec<Value>, _: Span) -> Ev<Value> {
    args!(a, "String.starts_with", [Value::Str(s), Value::Str(p)] => boolean(s.starts_with(&**p)))
}

fn str_ends_with(_: &mut Interp<'_, '_>, a: Vec<Value>, _: Span) -> Ev<Value> {
    args!(a, "String.ends_with", [Value::Str(s), Value::Str(p)] => boolean(s.ends_with(&**p)))
}

fn str_contains(_: &mut Interp<'_, '_>, a: Vec<Value>, _: Span) -> Ev<Value> {
    args!(a, "String.contains", [Value::Str(s), Value::Str(p)] => boolean(s.contains(&**p)))
}

fn str_replace(it: &mut Interp<'_, '_>, a: Vec<Value>, sp: Span) -> Ev<Value> {
    args!(a, "String.replace", [Value::Str(s), Value::Str(old), Value::Str(new)] => {
        if old.is_empty() {
            return it.panic("`replace` of an empty string", sp);
        }
        string(&s.replace(&**old, new))
    })
}

fn str_to_upper(_: &mut Interp<'_, '_>, a: Vec<Value>, _: Span) -> Ev<Value> {
    args!(a, "String.to_upper", [Value::Str(s)] => string(&s.to_uppercase()))
}

fn str_to_lower(_: &mut Interp<'_, '_>, a: Vec<Value>, _: Span) -> Ev<Value> {
    args!(a, "String.to_lower", [Value::Str(s)] => string(&s.to_lowercase()))
}

fn str_arabic_fold(_: &mut Interp<'_, '_>, a: Vec<Value>, _: Span) -> Ev<Value> {
    args!(a, "String.arabic_fold", [Value::Str(s)] => string(&kalam_skins::arabic_fold(s)))
}

fn str_to_int(it: &mut Interp<'_, '_>, a: Vec<Value>, _: Span) -> Ev<Value> {
    args!(a, "String.to_int", [Value::Str(s)] => it.parse_result(num::parse_int(s).map(Value::Int), s))
}

/// `-`? digits (`.` digits)? (`e`|`E` (`+`|`-`)? digits)?, ASCII only.
fn float_shape(text: &str) -> bool {
    let digits = |s: &str| !s.is_empty() && s.chars().all(|c| c.is_ascii_digit());
    let body = text.strip_prefix('-').unwrap_or(text);
    let (mantissa, exp) = match body.find(['e', 'E']) {
        Some(i) => (&body[..i], Some(&body[i + 1..])),
        None => (body, None),
    };
    let mantissa_ok = match mantissa.split_once('.') {
        Some((int, frac)) => digits(int) && digits(frac),
        None => digits(mantissa),
    };
    let exp_ok = exp.map_or(true, |e| {
        digits(
            e.strip_prefix('+')
                .or_else(|| e.strip_prefix('-'))
                .unwrap_or(e),
        )
    });
    mantissa_ok && exp_ok
}

fn str_to_float(it: &mut Interp<'_, '_>, a: Vec<Value>, _: Span) -> Ev<Value> {
    args!(a, "String.to_float", [Value::Str(s)] => {
        let r = if s.is_empty() {
            Err(ParseFail::Empty)
        } else if !float_shape(s) {
            Err(ParseFail::Invalid)
        } else {
            match s.parse::<f64>() {
                Ok(f) if f.is_finite() => Ok(Value::Float(f)),
                Ok(_) => Err(ParseFail::OutOfRange),
                Err(_) => Err(ParseFail::Invalid),
            }
        };
        it.parse_result(r, s)
    })
}

fn str_to_decimal(it: &mut Interp<'_, '_>, a: Vec<Value>, _: Span) -> Ev<Value> {
    args!(a, "String.to_decimal", [Value::Str(s)] => {
        let r = Decimal::parse(s).map(|d| Value::Decimal(Rc::new(d)));
        it.parse_result(r, s)
    })
}

// -- List ------------------------------------------------------------------------------------

fn list_len(_: &mut Interp<'_, '_>, a: Vec<Value>, _: Span) -> Ev<Value> {
    args!(a, "List.len", [Value::List(xs)] => int(xs.len() as i64))
}

fn list_is_empty(_: &mut Interp<'_, '_>, a: Vec<Value>, _: Span) -> Ev<Value> {
    args!(a, "List.is_empty", [Value::List(xs)] => boolean(xs.is_empty()))
}

fn list_get(_: &mut Interp<'_, '_>, a: Vec<Value>, _: Span) -> Ev<Value> {
    args!(a, "List.get", [Value::List(xs), Value::Int(i)] => {
        Ok(option(usize::try_from(*i).ok().and_then(|i| xs.get(i)).cloned()))
    })
}

fn list_set(it: &mut Interp<'_, '_>, a: Vec<Value>, sp: Span) -> Ev<Value> {
    args!(a, "List.set", [Value::List(xs), Value::Int(i), v] => {
        let i = index(it, *i, xs.len(), sp)?;
        let mut out = xs.to_vec();
        out[i] = v.clone();
        list(out)
    })
}

fn list_append(_: &mut Interp<'_, '_>, a: Vec<Value>, _: Span) -> Ev<Value> {
    args!(a, "List.append", [Value::List(xs), v] => {
        let mut out = xs.to_vec();
        out.push(v.clone());
        list(out)
    })
}

fn list_insert(it: &mut Interp<'_, '_>, a: Vec<Value>, sp: Span) -> Ev<Value> {
    args!(a, "List.insert", [Value::List(xs), Value::Int(i), v] => {
        let i = index(it, *i, xs.len() + 1, sp)?;
        let mut out = xs.to_vec();
        out.insert(i, v.clone());
        list(out)
    })
}

fn list_remove(it: &mut Interp<'_, '_>, a: Vec<Value>, sp: Span) -> Ev<Value> {
    args!(a, "List.remove", [Value::List(xs), Value::Int(i)] => {
        let i = index(it, *i, xs.len(), sp)?;
        let mut out = xs.to_vec();
        out.remove(i);
        list(out)
    })
}

fn list_map(it: &mut Interp<'_, '_>, a: Vec<Value>, sp: Span) -> Ev<Value> {
    args!(a, "List.map", [Value::List(xs), f] => {
        let mut out = Vec::with_capacity(xs.len());
        for x in xs.iter() {
            out.push(it.call1(f, x.clone(), sp)?);
        }
        list(out)
    })
}

fn list_filter(it: &mut Interp<'_, '_>, a: Vec<Value>, sp: Span) -> Ev<Value> {
    args!(a, "List.filter", [Value::List(xs), f] => {
        let mut out = Vec::new();
        for x in xs.iter() {
            if it.predicate(f, x.clone(), sp)? {
                out.push(x.clone());
            }
        }
        list(out)
    })
}

fn list_fold(it: &mut Interp<'_, '_>, a: Vec<Value>, sp: Span) -> Ev<Value> {
    args!(a, "List.fold", [Value::List(xs), init, f] => {
        let mut acc = init.clone();
        for x in xs.iter() {
            acc = it.call_value(f, vec![acc, x.clone()], sp)?;
        }
        Ok(acc)
    })
}

fn list_find(it: &mut Interp<'_, '_>, a: Vec<Value>, sp: Span) -> Ev<Value> {
    args!(a, "List.find", [Value::List(xs), f] => {
        for x in xs.iter() {
            if it.predicate(f, x.clone(), sp)? {
                return Ok(Value::some(x.clone()));
            }
        }
        Ok(Value::none())
    })
}

fn list_any(it: &mut Interp<'_, '_>, a: Vec<Value>, sp: Span) -> Ev<Value> {
    args!(a, "List.any", [Value::List(xs), f] => {
        for x in xs.iter() {
            if it.predicate(f, x.clone(), sp)? {
                return boolean(true);
            }
        }
        boolean(false)
    })
}

fn list_all(it: &mut Interp<'_, '_>, a: Vec<Value>, sp: Span) -> Ev<Value> {
    args!(a, "List.all", [Value::List(xs), f] => {
        for x in xs.iter() {
            if !it.predicate(f, x.clone(), sp)? {
                return boolean(false);
            }
        }
        boolean(true)
    })
}

/// A stable merge sort whose comparison may fail (a panic in the comparator).
fn merge_sort(
    xs: Vec<Value>,
    cmp: &mut dyn FnMut(&Value, &Value) -> Ev<Ordering>,
) -> Ev<Vec<Value>> {
    if xs.len() <= 1 {
        return Ok(xs);
    }
    let mut left = xs;
    let right = left.split_off(left.len() / 2);
    let left = merge_sort(left, cmp)?;
    let right = merge_sort(right, cmp)?;
    let mut out = Vec::with_capacity(left.len() + right.len());
    let (mut l, mut r) = (left.into_iter().peekable(), right.into_iter().peekable());
    while let (Some(a), Some(b)) = (l.peek(), r.peek()) {
        if cmp(b, a)? == Ordering::Less {
            out.push(r.next().expect("peeked"));
        } else {
            out.push(l.next().expect("peeked"));
        }
    }
    out.extend(l);
    out.extend(r);
    Ok(out)
}

fn list_sort_by(it: &mut Interp<'_, '_>, a: Vec<Value>, sp: Span) -> Ev<Value> {
    args!(a, "List.sort_by", [Value::List(xs), f] => {
        let mut cmp = |x: &Value, y: &Value| -> Ev<Ordering> {
            match it.call_value(f, vec![x.clone(), y.clone()], sp)? {
                Value::Variant(0, _) => Ok(Ordering::Less),
                Value::Variant(1, _) => Ok(Ordering::Equal),
                Value::Variant(2, _) => Ok(Ordering::Greater),
                other => internal(format!("comparator returned a {}", other.kind())),
            }
        };
        list(merge_sort(xs.to_vec(), &mut cmp)?)
    })
}

fn list_enumerate(_: &mut Interp<'_, '_>, a: Vec<Value>, _: Span) -> Ev<Value> {
    args!(a, "List.enumerate", [Value::List(xs)] => list(
        xs.iter()
            .enumerate()
            .map(|(i, x)| Value::Tuple(Rc::from(vec![Value::Int(i as i64), x.clone()])))
            .collect(),
    ))
}

fn list_zip(_: &mut Interp<'_, '_>, a: Vec<Value>, _: Span) -> Ev<Value> {
    args!(a, "List.zip", [Value::List(xs), Value::List(ys)] => list(
        xs.iter()
            .zip(ys.iter())
            .map(|(x, y)| Value::Tuple(Rc::from(vec![x.clone(), y.clone()])))
            .collect(),
    ))
}

fn list_take(it: &mut Interp<'_, '_>, a: Vec<Value>, sp: Span) -> Ev<Value> {
    args!(a, "List.take", [Value::List(xs), Value::Int(n)] => {
        let n = count(it, *n, xs.len(), sp)?;
        list(xs[..n].to_vec())
    })
}

fn list_skip(it: &mut Interp<'_, '_>, a: Vec<Value>, sp: Span) -> Ev<Value> {
    args!(a, "List.skip", [Value::List(xs), Value::Int(n)] => {
        let n = count(it, *n, xs.len(), sp)?;
        list(xs[n..].to_vec())
    })
}

fn list_first(_: &mut Interp<'_, '_>, a: Vec<Value>, _: Span) -> Ev<Value> {
    args!(a, "List.first", [Value::List(xs)] => Ok(option(xs.first().cloned())))
}

fn list_last(_: &mut Interp<'_, '_>, a: Vec<Value>, _: Span) -> Ev<Value> {
    args!(a, "List.last", [Value::List(xs)] => Ok(option(xs.last().cloned())))
}

fn list_group_by(it: &mut Interp<'_, '_>, a: Vec<Value>, sp: Span) -> Ev<Value> {
    args!(a, "List.group_by", [Value::List(xs), f] => {
        let mut groups = OrderedMap::default();
        for x in xs.iter() {
            let k = it.call1(f, x.clone(), sp)?;
            let mut items = match groups.get(&k) {
                Some(Value::List(g)) => g.to_vec(),
                _ => Vec::new(),
            };
            items.push(x.clone());
            groups.insert(k, Value::list(items));
        }
        Ok(Value::Map(Rc::new(groups)))
    })
}

fn list_contains(_: &mut Interp<'_, '_>, a: Vec<Value>, _: Span) -> Ev<Value> {
    args!(a, "List.contains", [Value::List(xs), v] => boolean(xs.iter().any(|x| x == v)))
}

fn list_sorted(_: &mut Interp<'_, '_>, a: Vec<Value>, _: Span) -> Ev<Value> {
    args!(a, "List.sorted", [Value::List(xs)] => {
        let mut cmp = |x: &Value, y: &Value| -> Ev<Ordering> {
            match compare(x, y) {
                Some(o) => Ok(o),
                None => internal(format!("ordering of a {} and a {}", x.kind(), y.kind())),
            }
        };
        list(merge_sort(xs.to_vec(), &mut cmp)?)
    })
}

fn list_sum(it: &mut Interp<'_, '_>, a: Vec<Value>, sp: Span) -> Ev<Value> {
    args!(a, "List.sum", [Value::List(xs)] => {
        let mut total: i64 = 0;
        for x in xs.iter() {
            let Value::Int(i) = x else { return bad("List.sum", &a) };
            total = match total.checked_add(*i) {
                Some(t) => t,
                None => return it.panic("integer overflow", sp),
            };
        }
        int(total)
    })
}

// -- Map and Set -----------------------------------------------------------------------------

fn map_of(v: &Value) -> Option<&Rc<OrderedMap>> {
    match v {
        Value::Map(m) | Value::Set(m) => Some(m),
        _ => None,
    }
}

fn map_len(_: &mut Interp<'_, '_>, a: Vec<Value>, _: Span) -> Ev<Value> {
    match a.as_slice() {
        [m] if map_of(m).is_some() => int(map_of(m).expect("checked").len() as i64),
        _ => bad("Map.len", &a),
    }
}

fn map_is_empty(_: &mut Interp<'_, '_>, a: Vec<Value>, _: Span) -> Ev<Value> {
    match a.as_slice() {
        [m] if map_of(m).is_some() => boolean(map_of(m).expect("checked").is_empty()),
        _ => bad("Map.is_empty", &a),
    }
}

fn key(k: &Value) -> Ev<()> {
    if k.hashable() {
        Ok(())
    } else {
        internal(format!("a {} as a key", k.kind()))
    }
}

fn map_get(_: &mut Interp<'_, '_>, a: Vec<Value>, _: Span) -> Ev<Value> {
    args!(a, "Map.get", [Value::Map(m), k] => Ok(option(m.get(k).cloned())))
}

fn map_contains_key(_: &mut Interp<'_, '_>, a: Vec<Value>, _: Span) -> Ev<Value> {
    match a.as_slice() {
        [m, k] if map_of(m).is_some() => boolean(map_of(m).expect("checked").get(k).is_some()),
        _ => bad("Map.contains_key", &a),
    }
}

fn map_insert(_: &mut Interp<'_, '_>, a: Vec<Value>, _: Span) -> Ev<Value> {
    args!(a, "Map.insert", [Value::Map(m), k, v] => {
        key(k)?;
        let mut out = OrderedMap::clone(m);
        out.insert(k.clone(), v.clone());
        Ok(Value::Map(Rc::new(out)))
    })
}

fn map_remove(_: &mut Interp<'_, '_>, a: Vec<Value>, _: Span) -> Ev<Value> {
    args!(a, "Map.remove", [Value::Map(m), k] => {
        let mut out = OrderedMap::clone(m);
        out.remove(k);
        Ok(Value::Map(Rc::new(out)))
    })
}

fn map_keys(_: &mut Interp<'_, '_>, a: Vec<Value>, _: Span) -> Ev<Value> {
    match a.as_slice() {
        [m] if map_of(m).is_some() => list(map_of(m).expect("checked").keys().cloned().collect()),
        _ => bad("Map.keys", &a),
    }
}

fn map_values(_: &mut Interp<'_, '_>, a: Vec<Value>, _: Span) -> Ev<Value> {
    args!(a, "Map.values", [Value::Map(m)] => list(m.entries().iter().map(|(_, v)| v.clone()).collect()))
}

fn map_map(it: &mut Interp<'_, '_>, a: Vec<Value>, sp: Span) -> Ev<Value> {
    args!(a, "Map.map", [Value::Map(m), f] => {
        let mut out = OrderedMap::default();
        for (k, v) in m.entries() {
            out.insert(k.clone(), it.call1(f, v.clone(), sp)?);
        }
        Ok(Value::Map(Rc::new(out)))
    })
}

fn map_filter(it: &mut Interp<'_, '_>, a: Vec<Value>, sp: Span) -> Ev<Value> {
    args!(a, "Map.filter", [Value::Map(m), f] => {
        let mut out = OrderedMap::default();
        for (k, v) in m.entries() {
            match it.call_value(f, vec![k.clone(), v.clone()], sp)? {
                Value::Bool(true) => out.insert(k.clone(), v.clone()),
                Value::Bool(false) => {}
                other => return internal(format!("predicate returned a {}", other.kind())),
            }
        }
        Ok(Value::Map(Rc::new(out)))
    })
}

fn set_from(items: impl IntoIterator<Item = Value>) -> Ev<Value> {
    let mut m = OrderedMap::default();
    for x in items {
        key(&x)?;
        m.insert(x, Value::Unit);
    }
    Ok(Value::Set(Rc::new(m)))
}

fn set_of(_: &mut Interp<'_, '_>, a: Vec<Value>, _: Span) -> Ev<Value> {
    args!(a, "Set.of", [Value::List(xs)] => set_from(xs.iter().cloned()))
}

fn set_insert(_: &mut Interp<'_, '_>, a: Vec<Value>, _: Span) -> Ev<Value> {
    args!(a, "Set.insert", [Value::Set(m), x] => set_from(m.keys().cloned().chain([x.clone()])))
}

fn set_remove(_: &mut Interp<'_, '_>, a: Vec<Value>, _: Span) -> Ev<Value> {
    args!(a, "Set.remove", [Value::Set(m), x] => {
        let mut out = OrderedMap::clone(m);
        out.remove(x);
        Ok(Value::Set(Rc::new(out)))
    })
}

fn set_union(_: &mut Interp<'_, '_>, a: Vec<Value>, _: Span) -> Ev<Value> {
    args!(a, "Set.union", [Value::Set(m), Value::Set(n)] => set_from(m.keys().chain(n.keys()).cloned()))
}

fn set_intersection(_: &mut Interp<'_, '_>, a: Vec<Value>, _: Span) -> Ev<Value> {
    args!(a, "Set.intersection", [Value::Set(m), Value::Set(n)] => {
        set_from(m.keys().filter(|k| n.get(k).is_some()).cloned())
    })
}

// -- Option and Result -----------------------------------------------------------------------

fn option_map(it: &mut Interp<'_, '_>, a: Vec<Value>, sp: Span) -> Ev<Value> {
    args!(a, "Option.map", [Value::Variant(t, p), f] => match (t, p.first()) {
        (1, Some(v)) => Ok(Value::some(it.call1(f, v.clone(), sp)?)),
        (0, None) => Ok(Value::none()),
        _ => bad("Option.map", &a),
    })
}

fn option_or(_: &mut Interp<'_, '_>, a: Vec<Value>, _: Span) -> Ev<Value> {
    args!(a, "Option.or", [Value::Variant(t, p), d] => match (t, p.first()) {
        (1, Some(v)) => Ok(v.clone()),
        (0, None) => Ok(d.clone()),
        _ => bad("Option.or", &a),
    })
}

fn option_ok_or(_: &mut Interp<'_, '_>, a: Vec<Value>, _: Span) -> Ev<Value> {
    args!(a, "Option.ok_or", [Value::Variant(t, p), e] => match (t, p.first()) {
        (1, Some(v)) => Ok(Value::ok(v.clone())),
        (0, None) => Ok(Value::err(e.clone())),
        _ => bad("Option.ok_or", &a),
    })
}

fn option_expect(it: &mut Interp<'_, '_>, a: Vec<Value>, sp: Span) -> Ev<Value> {
    args!(a, "Option.expect", [Value::Variant(t, p), Value::Str(reason)] => match (t, p.first()) {
        (1, Some(v)) => Ok(v.clone()),
        (0, None) => it.panic(&**reason, sp),
        _ => bad("Option.expect", &a),
    })
}

/// `Option.is_some` and `Result.is_err` (both: variant 1).
fn option_is_some(_: &mut Interp<'_, '_>, a: Vec<Value>, _: Span) -> Ev<Value> {
    args!(a, "is_some/is_err", [Value::Variant(t, _)] => boolean(*t == 1))
}

/// `Option.is_none` and `Result.is_ok` (both: variant 0).
fn option_is_none(_: &mut Interp<'_, '_>, a: Vec<Value>, _: Span) -> Ev<Value> {
    args!(a, "is_none/is_ok", [Value::Variant(t, _)] => boolean(*t == 0))
}

fn result_map(it: &mut Interp<'_, '_>, a: Vec<Value>, sp: Span) -> Ev<Value> {
    args!(a, "Result.map", [Value::Variant(t, p), f] => match (t, p.first()) {
        (0, Some(v)) => Ok(Value::ok(it.call1(f, v.clone(), sp)?)),
        (1, Some(_)) => Ok(Value::Variant(1, p.clone())),
        _ => bad("Result.map", &a),
    })
}

fn result_map_err(it: &mut Interp<'_, '_>, a: Vec<Value>, sp: Span) -> Ev<Value> {
    args!(a, "Result.map_err", [Value::Variant(t, p), f] => match (t, p.first()) {
        (0, Some(_)) => Ok(Value::Variant(0, p.clone())),
        (1, Some(e)) => Ok(Value::err(it.call1(f, e.clone(), sp)?)),
        _ => bad("Result.map_err", &a),
    })
}

fn result_ok(_: &mut Interp<'_, '_>, a: Vec<Value>, _: Span) -> Ev<Value> {
    args!(a, "Result.ok", [Value::Variant(t, p)] => match (t, p.first()) {
        (0, Some(v)) => Ok(Value::some(v.clone())),
        (1, Some(_)) => Ok(Value::none()),
        _ => bad("Result.ok", &a),
    })
}

fn result_expect(it: &mut Interp<'_, '_>, a: Vec<Value>, sp: Span) -> Ev<Value> {
    args!(a, "Result.expect", [Value::Variant(t, p), Value::Str(reason)] => match (t, p.first()) {
        (0, Some(v)) => Ok(v.clone()),
        (1, Some(_)) => it.panic(&**reason, sp),
        _ => bad("Result.expect", &a),
    })
}

// -- BigInt and Decimal ----------------------------------------------------------------------

fn big_from(_: &mut Interp<'_, '_>, a: Vec<Value>, _: Span) -> Ev<Value> {
    args!(a, "BigInt.from", [Value::Int(x)] => Ok(Value::BigInt(Rc::new(BigInt::from_i64(*x)))))
}

fn big_parse(it: &mut Interp<'_, '_>, a: Vec<Value>, _: Span) -> Ev<Value> {
    args!(a, "BigInt.parse", [Value::Str(s)] => {
        let r = num::parse_big(s).map(|b| Value::BigInt(Rc::new(b)));
        it.parse_result(r, s)
    })
}

fn big_to_int(_: &mut Interp<'_, '_>, a: Vec<Value>, _: Span) -> Ev<Value> {
    args!(a, "BigInt.to_int", [Value::BigInt(b)] => Ok(option(b.to_i64().map(Value::Int))))
}

fn dec_from(_: &mut Interp<'_, '_>, a: Vec<Value>, _: Span) -> Ev<Value> {
    args!(a, "Decimal.from", [Value::Int(x)] => Ok(Value::Decimal(Rc::new(Decimal::from_i64(*x)))))
}

fn rounding(it: &Interp<'_, '_>, v: &Value) -> Ev<Rounding> {
    let Value::Variant(t, _) = v else {
        return internal("a Rounding that is not a variant");
    };
    let Some(e) = it.program().enum_named("std/decimal", "Rounding") else {
        return internal("the stdlib enum std/decimal.Rounding is not loaded");
    };
    Ok(match e.variants.get(*t as usize).map(|v| v.name.as_str()) {
        Some("HalfEven") => Rounding::HalfEven,
        Some("HalfUp") => Rounding::HalfUp,
        Some("Down") => Rounding::Down,
        Some("Up") => Rounding::Up,
        Some("Floor") => Rounding::Floor,
        Some("Ceiling") => Rounding::Ceiling,
        _ => return internal(format!("unknown Rounding variant {t}")),
    })
}

fn dec_div(it: &mut Interp<'_, '_>, a: Vec<Value>, sp: Span) -> Ev<Value> {
    args!(a, "Decimal.div", [Value::Decimal(x), Value::Decimal(y), Value::Int(scale), r] => {
        let mode = rounding(it, r)?;
        match x.div(y, *scale, mode) {
            Ok(d) => Ok(Value::Decimal(Rc::new(d))),
            Err(true) => it.panic("division by zero", sp),
            Err(false) => it.panic("Decimal overflow (more than 34 digits)", sp),
        }
    })
}

fn dec_round(it: &mut Interp<'_, '_>, a: Vec<Value>, sp: Span) -> Ev<Value> {
    args!(a, "Decimal.round", [Value::Decimal(x), Value::Int(scale), r] => {
        let mode = rounding(it, r)?;
        match x.round(*scale, mode) {
            Some(d) => Ok(Value::Decimal(Rc::new(d))),
            None => it.panic("Decimal overflow (more than 34 digits)", sp),
        }
    })
}

// -- std/math --------------------------------------------------------------------------------

fn ordered(a: &Value, b: &Value) -> Ev<Ordering> {
    match compare(a, b) {
        Some(o) => Ok(o),
        None => internal(format!("ordering of a {} and a {}", a.kind(), b.kind())),
    }
}

fn math_min(_: &mut Interp<'_, '_>, a: Vec<Value>, _: Span) -> Ev<Value> {
    args!(a, "std/math.min", [x, y] => Ok(if ordered(y, x)? == Ordering::Less { y.clone() } else { x.clone() }))
}

fn math_max(_: &mut Interp<'_, '_>, a: Vec<Value>, _: Span) -> Ev<Value> {
    args!(a, "std/math.max", [x, y] => Ok(if ordered(y, x)? == Ordering::Greater { y.clone() } else { x.clone() }))
}

fn math_clamp(it: &mut Interp<'_, '_>, a: Vec<Value>, sp: Span) -> Ev<Value> {
    args!(a, "std/math.clamp", [x, lo, hi] => {
        if ordered(lo, hi)? == Ordering::Greater {
            return it.panic("`clamp` with low > high", sp);
        }
        Ok(if ordered(x, lo)? == Ordering::Less {
            lo.clone()
        } else if ordered(x, hi)? == Ordering::Greater {
            hi.clone()
        } else {
            x.clone()
        })
    })
}

fn math_sqrt(_: &mut Interp<'_, '_>, a: Vec<Value>, _: Span) -> Ev<Value> {
    args!(a, "std/math.sqrt", [Value::Float(x)] => Ok(Value::Float(x.sqrt())))
}

fn math_floor(_: &mut Interp<'_, '_>, a: Vec<Value>, _: Span) -> Ev<Value> {
    args!(a, "std/math.floor", [Value::Float(x)] => Ok(Value::Float(x.floor())))
}

fn math_round(_: &mut Interp<'_, '_>, a: Vec<Value>, _: Span) -> Ev<Value> {
    args!(a, "std/math.round", [Value::Float(x)] => Ok(Value::Float(x.round())))
}

fn math_pow(_: &mut Interp<'_, '_>, a: Vec<Value>, _: Span) -> Ev<Value> {
    args!(a, "std/math.pow", [Value::Float(x), Value::Float(y)] => Ok(Value::Float(x.powf(*y))))
}

// -- Secret ----------------------------------------------------------------------------------

fn identity(_: &mut Interp<'_, '_>, a: Vec<Value>, _: Span) -> Ev<Value> {
    args!(a, "Secret.new/expose", [v] => Ok(v.clone()))
}
