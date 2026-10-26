//! Differential property test of the numeric core (spec §25 "Integers", SC D1): for random
//! well-typed prelude programs over `Int` and `Decimal`, the reference interpreter's output
//! equals an independent evaluation written here in plain Rust.
//!
//! The reference shares no code with `kalam_interp::num`: `Int` is computed in `i128` and
//! range-checked against [−2⁶³, 2⁶³−1] after every operation (D18); `Decimal` is an `i128`
//! coefficient with a decimal exponent, kept exact, with the decimal128 rule that a result
//! needing more than 34 significant digits traps unless dropping trailing zeros makes it
//! fit (the scale is otherwise kept, so `1.50` shows as `1.50`). Evaluation is left to
//! right; the first trap ends the program with exit 101 and the trap's message. When a
//! reference value leaves the `i128` range the reference cannot judge the rest, so only the
//! lines before it are compared (counted, and required to be rare).
//!
//! Operations: `+ - * // %`, unary `-`, comparisons, `abs`, `wrapping_add`,
//! `saturating_add`, and Q36's `pow`, `bit_and`, `bit_or`, `bit_xor`, `shift_left`,
//! `shift_right` on `Int`; `+ - *`, unary `-`, comparisons and `Decimal.from` on `Decimal`.
//!
//! `KALAM_INTERP_DIFF_CASES` (default 300) and `KALAM_INTERP_DIFF_SEED` configure a run; a
//! failure prints the seed and the program.

use kalam_driver::{run_interp, RunOutcome};

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        // splitmix64
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }

    fn chance(&mut self, percent: usize) -> bool {
        self.below(100) < percent
    }

    fn pick<T: Copy>(&mut self, xs: &[T]) -> T {
        xs[self.below(xs.len())]
    }
}

// -- the reference ------------------------------------------------------------------------

const MIN: i128 = i64::MIN as i128;
const MAX: i128 = i64::MAX as i128;
const DEC_DIGITS: u32 = 34;

/// Why evaluation stopped: a defined trap (its message must appear in the panic), or the
/// reference left `i128` and cannot judge.
#[derive(Clone, Debug, PartialEq)]
enum Stop {
    Trap(&'static str),
    Unknown,
}

type Ev<T> = Result<T, Stop>;

fn int(v: i128) -> Ev<i128> {
    if (MIN..=MAX).contains(&v) {
        Ok(v)
    } else {
        Err(Stop::Trap("integer overflow"))
    }
}

/// Python's `%`: the remainder takes the divisor's sign.
fn py_mod(a: i128, b: i128) -> i128 {
    let r = a % b;
    if r != 0 && ((r < 0) != (b < 0)) {
        r + b
    } else {
        r
    }
}

fn int_pow(base: i128, exp: i128) -> Ev<i128> {
    if exp < 0 {
        return Err(Stop::Trap("negative exponent"));
    }
    let mut acc: i128 = 1;
    let mut e = exp;
    while e > 0 {
        acc *= base;
        // |base| >= 2 grows without bound, so leaving the range early is final;
        // |base| <= 1 never leaves it.
        if !(MIN..=MAX).contains(&acc) {
            return Err(Stop::Trap("integer overflow"));
        }
        if base.abs() <= 1 && e > 2 {
            // 0, 1 and -1: the rest only depends on the parity.
            e = 2 + e % 2;
        }
        e -= 1;
    }
    Ok(acc)
}

fn shift_amount(by: i128) -> Ev<u32> {
    if (0..=63).contains(&by) {
        Ok(by as u32)
    } else {
        Err(Stop::Trap("shift amount"))
    }
}

/// The low 64 bits of `v`, read as two's complement.
fn wrap64(v: i128) -> i128 {
    let m = v.rem_euclid(1i128 << 64);
    if m > MAX {
        m - (1i128 << 64)
    } else {
        m
    }
}

/// A decimal value: `coef × 10^exp`, as the spec's decimal128 keeps it.
#[derive(Clone, Copy, Debug)]
struct Dec {
    coef: i128,
    exp: i32,
}

fn digits(mut v: i128) -> u32 {
    let mut n = 1;
    while v.abs() >= 10 {
        v /= 10;
        n += 1;
    }
    n
}

fn pow10(k: u32) -> Ev<i128> {
    10i128.checked_pow(k).ok_or(Stop::Unknown)
}

fn dec(coef: i128, exp: i32) -> Ev<Dec> {
    let mut d = Dec { coef, exp };
    while digits(d.coef) > DEC_DIGITS {
        if d.coef % 10 != 0 {
            return Err(Stop::Trap("Decimal overflow"));
        }
        d.coef /= 10;
        d.exp += 1;
    }
    Ok(d)
}

fn aligned(a: Dec, b: Dec) -> Ev<(i128, i128, i32)> {
    let e = a.exp.min(b.exp);
    let x = a
        .coef
        .checked_mul(pow10((a.exp - e) as u32)?)
        .ok_or(Stop::Unknown)?;
    let y = b
        .coef
        .checked_mul(pow10((b.exp - e) as u32)?)
        .ok_or(Stop::Unknown)?;
    Ok((x, y, e))
}

fn show_dec(d: Dec) -> String {
    let sign = if d.coef < 0 { "-" } else { "" };
    let body = d.coef.unsigned_abs().to_string();
    if d.exp >= 0 {
        let zeros = if d.coef == 0 {
            String::new()
        } else {
            "0".repeat(d.exp as usize)
        };
        return format!("{sign}{body}{zeros}");
    }
    let places = (-d.exp) as usize;
    let padded = format!("{body:0>width$}", width = places + 1);
    let (i, f) = padded.split_at(padded.len() - places);
    format!("{sign}{i}.{f}")
}

// -- programs -----------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq)]
enum IOp {
    Add,
    Sub,
    Mul,
    FloorDiv,
    Mod,
    WrappingAdd,
    SaturatingAdd,
    BitAnd,
    BitOr,
    BitXor,
    ShiftLeft,
    ShiftRight,
    Pow,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Cmp {
    Lt,
    Le,
    Gt,
    Ge,
    Eq,
    Ne,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum DOp {
    Add,
    Sub,
    Mul,
}

#[derive(Clone, Debug)]
enum IExpr {
    Var(usize),
    Lit(i64),
    Neg(Box<IExpr>),
    Abs(Box<IExpr>),
    Bin(IOp, Box<IExpr>, Box<IExpr>),
}

#[derive(Clone, Debug)]
enum DExpr {
    Var(usize),
    FromInt(IExpr),
    Neg(Box<DExpr>),
    Bin(DOp, Box<DExpr>, Box<DExpr>),
}

#[derive(Clone, Debug)]
enum Stmt {
    LetInt(IExpr),
    LetDec(DExpr),
    PrintInt(IExpr),
    PrintDec(DExpr),
    CmpInt(Cmp, IExpr, IExpr),
    CmpDec(Cmp, DExpr, DExpr),
}

/// Edge values: the 64-bit bounds, the JS safe-integer edge (2⁵³±1), the square-root edge
/// of the range, powers of two and small numbers.
const INTS: &[&str] = &[
    "0",
    "1",
    "-1",
    "2",
    "-2",
    "3",
    "7",
    "-13",
    "100",
    "255",
    "-256",
    "1000003",
    "2147483647",
    "4294967296",
    "9007199254740991",
    "9007199254740992",
    "-9007199254740993",
    "3037000499",
    "3037000500",
    "-3037000500",
    "4611686018427387904",
    "9223372036854775807",
    "9223372036854775806",
    "-9223372036854775808",
    "-9223372036854775807",
];

/// Decimal literals: scales 0–4, a 34-digit coefficient and trailing zeros.
const DECS: &[&str] = &[
    "0",
    "0.1",
    "0.2",
    "-0.5",
    "1.50",
    "19.99",
    "-3.75",
    "100",
    "0.0001",
    "123456789.123",
    "-0.30",
    "2.5",
    "1000000000000000000",
    "9999999999999999999999999999999999",
    "-999999999999999999999999999999999.9",
    "0.1000",
];

fn parse_dec(text: &str) -> Dec {
    let (neg, body) = match text.strip_prefix('-') {
        Some(b) => (true, b),
        None => (false, text),
    };
    let (i, f) = body.split_once('.').unwrap_or((body, ""));
    let coef: i128 = format!("{i}{f}").parse().expect("decimal digits");
    Dec {
        coef: if neg { -coef } else { coef },
        exp: -(f.len() as i32),
    }
}

struct Program {
    ints: Vec<String>,
    decs: Vec<String>,
    stmts: Vec<Stmt>,
}

struct Gen {
    rng: Rng,
    ints: usize,
    decs: usize,
}

impl Gen {
    fn iexpr(&mut self, depth: u32) -> IExpr {
        if depth == 0 || self.rng.chance(30) {
            return if self.rng.chance(80) {
                IExpr::Var(self.rng.below(self.ints))
            } else {
                IExpr::Lit(self.rng.pick(&[0, 1, 2, 3, 5, 10, 63, 64]))
            };
        }
        if self.rng.chance(10) {
            let inner = Box::new(self.iexpr(depth - 1));
            return if self.rng.chance(50) {
                IExpr::Neg(inner)
            } else {
                IExpr::Abs(inner)
            };
        }
        let op = self.rng.pick(&[
            IOp::Add,
            IOp::Sub,
            IOp::Mul,
            IOp::FloorDiv,
            IOp::Mod,
            IOp::WrappingAdd,
            IOp::SaturatingAdd,
            IOp::BitAnd,
            IOp::BitOr,
            IOp::BitXor,
            IOp::ShiftLeft,
            IOp::ShiftRight,
            IOp::Pow,
        ]);
        let left = Box::new(self.iexpr(depth - 1));
        let right = match op {
            // Exponents and shift amounts around the edges, sometimes outside them.
            IOp::Pow => IExpr::Lit(self.rng.pick(&[-1, 0, 1, 2, 3, 5, 31, 62, 63, 64])),
            IOp::ShiftLeft | IOp::ShiftRight => {
                if self.rng.chance(85) {
                    IExpr::Lit(self.rng.pick(&[0, 1, 2, 7, 31, 32, 62, 63]))
                } else {
                    self.iexpr(depth - 1)
                }
            }
            _ => self.iexpr(depth - 1),
        };
        IExpr::Bin(op, left, Box::new(right))
    }

    fn dexpr(&mut self, depth: u32) -> DExpr {
        if depth == 0 || self.rng.chance(30) {
            return if self.rng.chance(85) {
                DExpr::Var(self.rng.below(self.decs))
            } else {
                DExpr::FromInt(self.iexpr(1))
            };
        }
        if self.rng.chance(10) {
            return DExpr::Neg(Box::new(self.dexpr(depth - 1)));
        }
        let op = self.rng.pick(&[DOp::Add, DOp::Sub, DOp::Mul]);
        DExpr::Bin(
            op,
            Box::new(self.dexpr(depth - 1)),
            Box::new(self.dexpr(depth - 1)),
        )
    }

    fn cmp(&mut self) -> Cmp {
        self.rng
            .pick(&[Cmp::Lt, Cmp::Le, Cmp::Gt, Cmp::Ge, Cmp::Eq, Cmp::Ne])
    }

    fn program(seed: u64) -> Program {
        let mut g = Gen {
            rng: Rng(seed),
            ints: 0,
            decs: 0,
        };
        // Half the operands are small, so most programs get past their first operation.
        let ints: Vec<String> = (0..3 + g.rng.below(3))
            .map(|_| {
                let pool = if g.rng.chance(50) { &INTS[..10] } else { INTS };
                g.rng.pick(pool).to_string()
            })
            .collect();
        let decs: Vec<String> = (0..2 + g.rng.below(3))
            .map(|_| {
                let pool = if g.rng.chance(50) { &DECS[..8] } else { DECS };
                g.rng.pick(pool).to_string()
            })
            .collect();
        g.ints = ints.len();
        g.decs = decs.len();
        let mut stmts = Vec::new();
        for _ in 0..6 + g.rng.below(8) {
            let s = match g.rng.below(10) {
                0 => {
                    let e = g.iexpr(2);
                    g.ints += 1;
                    Stmt::LetInt(e)
                }
                1 => {
                    let e = g.dexpr(2);
                    g.decs += 1;
                    Stmt::LetDec(e)
                }
                2..=4 => Stmt::PrintInt(g.iexpr(3)),
                5..=6 => Stmt::PrintDec(g.dexpr(3)),
                7 => {
                    let c = g.cmp();
                    Stmt::CmpInt(c, g.iexpr(2), g.iexpr(2))
                }
                _ => {
                    let c = g.cmp();
                    Stmt::CmpDec(c, g.dexpr(2), g.dexpr(2))
                }
            };
            stmts.push(s);
        }
        Program { ints, decs, stmts }
    }
}

// -- printing as Kalam ----------------------------------------------------------------------

fn ivar(i: usize) -> String {
    format!("i{i}")
}

fn dvar(i: usize) -> String {
    format!("d{i}")
}

fn iop_text(op: IOp) -> Result<&'static str, &'static str> {
    // Ok: an infix operator; Err: a method name.
    match op {
        IOp::Add => Ok("+"),
        IOp::Sub => Ok("-"),
        IOp::Mul => Ok("*"),
        IOp::FloorDiv => Ok("//"),
        IOp::Mod => Ok("%"),
        IOp::WrappingAdd => Err("wrapping_add"),
        IOp::SaturatingAdd => Err("saturating_add"),
        IOp::BitAnd => Err("bit_and"),
        IOp::BitOr => Err("bit_or"),
        IOp::BitXor => Err("bit_xor"),
        IOp::ShiftLeft => Err("shift_left"),
        IOp::ShiftRight => Err("shift_right"),
        IOp::Pow => Err("pow"),
    }
}

fn cmp_text(c: Cmp) -> &'static str {
    match c {
        Cmp::Lt => "<",
        Cmp::Le => "<=",
        Cmp::Gt => ">",
        Cmp::Ge => ">=",
        Cmp::Eq => "==",
        Cmp::Ne => "!=",
    }
}

fn itext(e: &IExpr) -> String {
    match e {
        IExpr::Var(i) => ivar(*i),
        IExpr::Lit(v) => format!("({v})"),
        IExpr::Neg(x) => format!("(-{})", itext(x)),
        IExpr::Abs(x) => format!("{}.abs()", itext(x)),
        IExpr::Bin(op, l, r) => match iop_text(*op) {
            Ok(sym) => format!("({} {sym} {})", itext(l), itext(r)),
            Err(method) => format!("{}.{method}({})", itext(l), itext(r)),
        },
    }
}

fn dtext(e: &DExpr) -> String {
    match e {
        DExpr::Var(i) => dvar(*i),
        DExpr::FromInt(x) => format!("Decimal.from({})", itext(x)),
        DExpr::Neg(x) => format!("(-{})", dtext(x)),
        DExpr::Bin(op, l, r) => {
            let sym = match op {
                DOp::Add => "+",
                DOp::Sub => "-",
                DOp::Mul => "*",
            };
            format!("({} {sym} {})", dtext(l), dtext(r))
        }
    }
}

fn source(p: &Program) -> String {
    let mut s = String::from("fn main():\n");
    for (i, v) in p.ints.iter().enumerate() {
        s.push_str(&format!("    let {}: Int = {v}\n", ivar(i)));
    }
    for (i, v) in p.decs.iter().enumerate() {
        s.push_str(&format!("    let {}: Decimal = {v}\n", dvar(i)));
    }
    let (mut ni, mut nd) = (p.ints.len(), p.decs.len());
    for st in &p.stmts {
        let line = match st {
            Stmt::LetInt(e) => {
                ni += 1;
                format!("let {}: Int = {}", ivar(ni - 1), itext(e))
            }
            Stmt::LetDec(e) => {
                nd += 1;
                format!("let {}: Decimal = {}", dvar(nd - 1), dtext(e))
            }
            Stmt::PrintInt(e) => format!("print(\"{{{}}}\")", itext(e)),
            Stmt::PrintDec(e) => format!("print(\"{{{}}}\")", dtext(e)),
            Stmt::CmpInt(c, l, r) => {
                format!("print(\"{{{} {} {}}}\")", itext(l), cmp_text(*c), itext(r))
            }
            Stmt::CmpDec(c, l, r) => {
                format!("print(\"{{{} {} {}}}\")", dtext(l), cmp_text(*c), dtext(r))
            }
        };
        s.push_str("    ");
        s.push_str(&line);
        s.push('\n');
    }
    s
}

// -- evaluating with the reference ---------------------------------------------------------

struct Env {
    ints: Vec<i128>,
    decs: Vec<Dec>,
}

fn ieval(env: &Env, e: &IExpr) -> Ev<i128> {
    Ok(match e {
        IExpr::Var(i) => env.ints[*i],
        IExpr::Lit(v) => *v as i128,
        IExpr::Neg(x) => int(-ieval(env, x)?)?,
        IExpr::Abs(x) => int(ieval(env, x)?.abs())?,
        IExpr::Bin(op, l, r) => {
            let a = ieval(env, l)?;
            let b = ieval(env, r)?;
            match op {
                IOp::Add => int(a + b)?,
                IOp::Sub => int(a - b)?,
                IOp::Mul => int(a * b)?,
                IOp::FloorDiv => {
                    if b == 0 {
                        return Err(Stop::Trap("division by zero"));
                    }
                    int((a - py_mod(a, b)) / b)?
                }
                IOp::Mod => {
                    if b == 0 {
                        return Err(Stop::Trap("modulo by zero"));
                    }
                    py_mod(a, b)
                }
                IOp::WrappingAdd => wrap64(a + b),
                IOp::SaturatingAdd => (a + b).clamp(MIN, MAX),
                IOp::BitAnd => (a as i64 as u64 & b as i64 as u64) as i64 as i128,
                IOp::BitOr => (a as i64 as u64 | b as i64 as u64) as i64 as i128,
                IOp::BitXor => (a as i64 as u64 ^ b as i64 as u64) as i64 as i128,
                IOp::ShiftLeft => wrap64(a * (1i128 << shift_amount(b)?)),
                IOp::ShiftRight => {
                    let d = 1i128 << shift_amount(b)?;
                    (a - a.rem_euclid(d)) / d
                }
                IOp::Pow => int_pow(a, b)?,
            }
        }
    })
}

fn deval(env: &Env, e: &DExpr) -> Ev<Dec> {
    match e {
        DExpr::Var(i) => Ok(env.decs[*i]),
        DExpr::FromInt(x) => Ok(Dec {
            coef: ieval(env, x)?,
            exp: 0,
        }),
        DExpr::Neg(x) => {
            let d = deval(env, x)?;
            Ok(Dec {
                coef: -d.coef,
                exp: d.exp,
            })
        }
        DExpr::Bin(op, l, r) => {
            let a = deval(env, l)?;
            let b = deval(env, r)?;
            match op {
                DOp::Add | DOp::Sub => {
                    let (x, y, e) = aligned(a, b)?;
                    let c = if *op == DOp::Add {
                        x.checked_add(y)
                    } else {
                        x.checked_sub(y)
                    };
                    dec(c.ok_or(Stop::Unknown)?, e)
                }
                DOp::Mul => dec(
                    a.coef.checked_mul(b.coef).ok_or(Stop::Unknown)?,
                    a.exp + b.exp,
                ),
            }
        }
    }
}

fn compare(c: Cmp, o: std::cmp::Ordering) -> bool {
    use std::cmp::Ordering::*;
    match c {
        Cmp::Lt => o == Less,
        Cmp::Le => o != Greater,
        Cmp::Gt => o == Greater,
        Cmp::Ge => o != Less,
        Cmp::Eq => o == Equal,
        Cmp::Ne => o != Equal,
    }
}

/// The reference run: the printed lines, then why it stopped (if it did).
fn reference(p: &Program) -> (Vec<String>, Option<Stop>) {
    let mut env = Env {
        ints: p.ints.iter().map(|t| t.parse().expect("an Int")).collect(),
        decs: p.decs.iter().map(|t| parse_dec(t)).collect(),
    };
    let mut lines = Vec::new();
    for st in &p.stmts {
        let r: Ev<Option<String>> = (|| {
            Ok(match st {
                Stmt::LetInt(e) => {
                    let v = ieval(&env, e)?;
                    env.ints.push(v);
                    None
                }
                Stmt::LetDec(e) => {
                    let v = deval(&env, e)?;
                    env.decs.push(v);
                    None
                }
                Stmt::PrintInt(e) => Some(ieval(&env, e)?.to_string()),
                Stmt::PrintDec(e) => Some(show_dec(deval(&env, e)?)),
                Stmt::CmpInt(c, l, r) => {
                    let a = ieval(&env, l)?;
                    let b = ieval(&env, r)?;
                    Some(compare(*c, a.cmp(&b)).to_string())
                }
                Stmt::CmpDec(c, l, r) => {
                    let a = deval(&env, l)?;
                    let b = deval(&env, r)?;
                    let (x, y, _) = aligned(a, b)?;
                    Some(compare(*c, x.cmp(&y)).to_string())
                }
            })
        })();
        match r {
            Ok(Some(line)) => lines.push(line),
            Ok(None) => {}
            Err(stop) => return (lines, Some(stop)),
        }
    }
    (lines, None)
}

// -- the property ----------------------------------------------------------------------------

#[test]
fn reference_evaluation_unit_checks() {
    assert_eq!(int_pow(2, 62), Ok(1 << 62));
    assert_eq!(int_pow(-2, 63), Ok(MIN));
    assert_eq!(int_pow(-1, 63), Ok(-1));
    assert_eq!(int_pow(0, 0), Ok(1));
    assert_eq!(int_pow(2, 63), Err(Stop::Trap("integer overflow")));
    assert_eq!(wrap64(MAX + 1), MIN);
    assert_eq!(show_dec(parse_dec("-0.30")), "-0.30");
    assert_eq!(show_dec(parse_dec("0.0001")), "0.0001");
    let big = parse_dec("9999999999999999999999999999999999");
    assert!(matches!(
        dec(big.coef + 1, 0),
        Ok(Dec { coef, exp: 1 }) if coef == 10i128.pow(33)
    ));
    assert_eq!(
        dec(big.coef * 10 + 1, 0).map(|d| d.coef),
        Err(Stop::Trap("Decimal overflow"))
    );
}

fn env_u64(key: &str, default: u64) -> u64 {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

#[test]
fn interpreter_numbers_match_an_independent_reference() {
    let cases = env_u64("KALAM_INTERP_DIFF_CASES", 300);
    let base = env_u64("KALAM_INTERP_DIFF_SEED", 0x0064_6966_6600);
    let (mut ran, mut trapped, mut unknown, mut rejected) = (0u64, 0u64, 0u64, 0u64);
    let mut lines_checked = 0usize;
    let mut first_rejected = None;
    for i in 0..cases {
        let seed = base.wrapping_add(i);
        let p = Gen::program(seed);
        let src = source(&p);
        let mut out = Vec::new();
        let outcome = run_interp("main.kl", src.as_bytes(), &mut out).expect("a Kalam path");
        if let RunOutcome::Rejected(c) = &outcome {
            rejected += 1;
            first_rejected
                .get_or_insert_with(|| (seed, c.render_human(kalam_base::Skin::En), src.clone()));
            continue;
        }
        ran += 1;
        let stdout = String::from_utf8(out).expect("UTF-8");
        let got: Vec<&str> = stdout.lines().collect();
        let (want, stop) = reference(&p);
        let ctx = || format!("seed {seed}\n{src}\nreference: {want:?} then {stop:?}\ngot: {got:?}");
        match stop {
            None => {
                assert!(
                    matches!(outcome, RunOutcome::Done),
                    "expected a normal end: {}\n{}",
                    outcome.stderr_text("main.kl"),
                    ctx()
                );
                assert_eq!(got, want, "{}", ctx());
            }
            Some(Stop::Trap(msg)) => {
                trapped += 1;
                match &outcome {
                    RunOutcome::Panicked { message, .. } => {
                        assert!(
                            message.contains(msg),
                            "panic {message:?}, want {msg:?}\n{}",
                            ctx()
                        )
                    }
                    other => panic!(
                        "expected a trap ({msg}): exit {}\n{}",
                        other.exit_code(),
                        ctx()
                    ),
                }
                assert_eq!(got, want, "{}", ctx());
            }
            Some(Stop::Unknown) => {
                unknown += 1;
                assert!(
                    !matches!(
                        outcome,
                        RunOutcome::Internal(_) | RunOutcome::Unsupported(_)
                    ),
                    "{}\n{}",
                    outcome.stderr_text("main.kl"),
                    ctx()
                );
                assert!(got.len() >= want.len(), "{}", ctx());
                assert_eq!(got[..want.len()], want[..], "{}", ctx());
            }
        }
        lines_checked += want.len();
    }
    eprintln!(
        "numeric differential: {cases} programs, {ran} ran ({trapped} trapped, {unknown} past the reference's range), {rejected} rejected, {lines_checked} lines compared"
    );
    assert!(
        ran * 10 >= cases * 9,
        "only {ran}/{cases} generated programs type-checked; first rejected: {first_rejected:?}"
    );
    assert!(
        unknown * 10 <= ran,
        "the reference judged too few programs: {unknown}/{ran}"
    );
}
