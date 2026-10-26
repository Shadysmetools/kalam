//! The numeric runtime with spec semantics (§9.2): checked 64-bit `Int` (D18), an
//! arbitrary-precision `BigInt`, and an exact `Decimal` with 34 significant digits
//! (decimal128 range). No dependencies (ARCHITECTURE.md §3.2 rule 5).

use std::cmp::Ordering;
use std::fmt;

// ---------------------------------------------------------------------------------------
// Int
// ---------------------------------------------------------------------------------------

/// `a // b`: floors; `None` on division by zero or overflow (`−2⁶³ // −1`).
pub fn int_floor_div(a: i64, b: i64) -> Option<i64> {
    if b == 0 {
        return None;
    }
    let q = a.checked_div(b)?;
    Some(if a % b != 0 && ((a < 0) != (b < 0)) {
        q - 1
    } else {
        q
    })
}

/// `a % b` with the sign of the divisor; `None` on modulo by zero. `−2⁶³ % −1` is 0.
pub fn int_mod(a: i64, b: i64) -> Option<i64> {
    if b == 0 {
        return None;
    }
    let r = a.wrapping_rem(b);
    Some(if r != 0 && ((r < 0) != (b < 0)) {
        r + b
    } else {
        r
    })
}

/// Why `Int.pow` has no `Int` result.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PowFail {
    NegativeExponent,
    Overflow,
}

/// `Int.pow` (Q36): exact, checked (D18). `0.pow(0)` is 1; the bases 0, 1 and −1 take any
/// exponent without looping.
pub fn int_pow(base: i64, exponent: i64) -> Result<i64, PowFail> {
    if exponent < 0 {
        return Err(PowFail::NegativeExponent);
    }
    match base {
        0 => return Ok(if exponent == 0 { 1 } else { 0 }),
        1 => return Ok(1),
        -1 => return Ok(if exponent % 2 == 0 { 1 } else { -1 }),
        _ => {}
    }
    // |base| >= 2, so any exponent past 63 overflows.
    let e = u32::try_from(exponent).map_err(|_| PowFail::Overflow)?;
    base.checked_pow(e).ok_or(PowFail::Overflow)
}

/// The shift amount of `Int.shift_left`/`shift_right` (spec §5.9: outside `0..=63` panics).
pub fn shift_amount(by: i64) -> Option<u32> {
    u32::try_from(by).ok().filter(|b| *b <= 63)
}

/// `Float.to_int`: truncates toward zero; `None` for NaN, infinities and values outside
/// [−2⁶³, 2⁶³) (2⁶³ itself is a `Float` and is out of range).
pub fn float_to_int(f: f64) -> Option<i64> {
    const TWO_63: f64 = 9_223_372_036_854_775_808.0;
    if !f.is_finite() {
        return None;
    }
    let t = f.trunc();
    if (-TWO_63..TWO_63).contains(&t) {
        Some(t as i64)
    } else {
        None
    }
}

/// The value of a digit: ASCII or Arabic-Indic (U+0660–U+0669), with which system it is.
pub fn digit_value(c: char) -> Option<(u32, bool)> {
    match c {
        '0'..='9' => Some((c as u32 - '0' as u32, false)),
        '\u{0660}'..='\u{0669}' => Some((c as u32 - 0x0660, true)),
        _ => None,
    }
}

/// Why runtime number parsing failed (`std/string.ParseError`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParseFail {
    Empty,
    Invalid,
    OutOfRange,
}

/// `String.to_int`: an optional `-`, then ASCII or Arabic-Indic digits (one system per
/// number, as for literals, §5.6), exactly; text outside the `Int` range is
/// [`ParseFail::OutOfRange`], never a rounded value.
pub fn parse_int(text: &str) -> Result<i64, ParseFail> {
    let big = parse_big(text)?;
    big.to_i64().ok_or(ParseFail::OutOfRange)
}

/// The digits of an integer: an optional `-`, then digits of one system.
fn parse_digits(text: &str) -> Result<(bool, Vec<u32>), ParseFail> {
    if text.is_empty() {
        return Err(ParseFail::Empty);
    }
    let (neg, body) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text),
    };
    let mut digits = Vec::new();
    let mut system = None;
    for c in body.chars() {
        let (d, arabic) = digit_value(c).ok_or(ParseFail::Invalid)?;
        if *system.get_or_insert(arabic) != arabic {
            return Err(ParseFail::Invalid);
        }
        digits.push(d);
    }
    if digits.is_empty() {
        return Err(ParseFail::Invalid);
    }
    Ok((neg, digits))
}

/// `BigInt.parse`: like `String.to_int`, with no range limit.
pub fn parse_big(text: &str) -> Result<BigInt, ParseFail> {
    let (neg, digits) = parse_digits(text)?;
    let mut v = BigInt::zero();
    for d in digits {
        v = v.mul_small(10).add_small(d);
    }
    Ok(if neg { v.negated() } else { v })
}

// ---------------------------------------------------------------------------------------
// BigInt: sign and magnitude, base 2^32 limbs, little-endian, no trailing zero limbs.
// ---------------------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct BigInt {
    neg: bool,
    mag: Vec<u32>,
}

impl BigInt {
    pub fn zero() -> BigInt {
        BigInt {
            neg: false,
            mag: Vec::new(),
        }
    }

    pub fn from_i64(v: i64) -> BigInt {
        BigInt::from_i128(v as i128)
    }

    pub fn from_i128(v: i128) -> BigInt {
        let neg = v < 0;
        let mut m = v.unsigned_abs();
        let mut mag = Vec::new();
        while m > 0 {
            mag.push(m as u32);
            m >>= 32;
        }
        BigInt { neg, mag }
    }

    pub fn is_zero(&self) -> bool {
        self.mag.is_empty()
    }

    pub fn is_negative(&self) -> bool {
        self.neg
    }

    pub fn to_i64(&self) -> Option<i64> {
        if self.mag.len() > 2 {
            return None;
        }
        let mut m: u128 = 0;
        for (i, l) in self.mag.iter().enumerate() {
            m |= (*l as u128) << (32 * i);
        }
        let v = if self.neg { -(m as i128) } else { m as i128 };
        i64::try_from(v).ok()
    }

    fn norm(mut self) -> BigInt {
        while self.mag.last() == Some(&0) {
            self.mag.pop();
        }
        if self.mag.is_empty() {
            self.neg = false;
        }
        self
    }

    pub fn negated(self) -> BigInt {
        let neg = !self.neg;
        BigInt { neg, ..self }.norm()
    }

    pub fn abs(self) -> BigInt {
        BigInt { neg: false, ..self }
    }

    fn mul_small(&self, k: u32) -> BigInt {
        let mut out = Vec::with_capacity(self.mag.len() + 1);
        let mut carry: u64 = 0;
        for l in &self.mag {
            let v = *l as u64 * k as u64 + carry;
            out.push(v as u32);
            carry = v >> 32;
        }
        if carry > 0 {
            out.push(carry as u32);
        }
        BigInt {
            neg: self.neg,
            mag: out,
        }
        .norm()
    }

    /// `|self| + k` keeping the sign (only used on non-negative values while parsing).
    fn add_small(&self, k: u32) -> BigInt {
        self.add(&BigInt::from_i64(k as i64))
    }

    /// Magnitude division by a small divisor: (quotient, remainder).
    fn divmod_small(mag: &[u32], k: u32) -> (Vec<u32>, u32) {
        let mut out = vec![0u32; mag.len()];
        let mut rem: u64 = 0;
        for i in (0..mag.len()).rev() {
            let cur = (rem << 32) | mag[i] as u64;
            out[i] = (cur / k as u64) as u32;
            rem = cur % k as u64;
        }
        (out, rem as u32)
    }

    fn cmp_mag(a: &[u32], b: &[u32]) -> Ordering {
        a.len()
            .cmp(&b.len())
            .then_with(|| a.iter().rev().cmp(b.iter().rev()))
    }

    fn add_mag(a: &[u32], b: &[u32]) -> Vec<u32> {
        let mut out = Vec::with_capacity(a.len().max(b.len()) + 1);
        let mut carry = 0u64;
        for i in 0..a.len().max(b.len()) {
            let v = *a.get(i).unwrap_or(&0) as u64 + *b.get(i).unwrap_or(&0) as u64 + carry;
            out.push(v as u32);
            carry = v >> 32;
        }
        if carry > 0 {
            out.push(carry as u32);
        }
        out
    }

    /// `a - b` for `|a| >= |b|`.
    fn sub_mag(a: &[u32], b: &[u32]) -> Vec<u32> {
        let mut out = Vec::with_capacity(a.len());
        let mut borrow = 0i64;
        for (i, x) in a.iter().enumerate() {
            let mut v = *x as i64 - *b.get(i).unwrap_or(&0) as i64 - borrow;
            borrow = 0;
            if v < 0 {
                v += 1 << 32;
                borrow = 1;
            }
            out.push(v as u32);
        }
        out
    }

    pub fn add(&self, o: &BigInt) -> BigInt {
        if self.neg == o.neg {
            return BigInt {
                neg: self.neg,
                mag: BigInt::add_mag(&self.mag, &o.mag),
            }
            .norm();
        }
        match BigInt::cmp_mag(&self.mag, &o.mag) {
            Ordering::Less => BigInt {
                neg: o.neg,
                mag: BigInt::sub_mag(&o.mag, &self.mag),
            },
            _ => BigInt {
                neg: self.neg,
                mag: BigInt::sub_mag(&self.mag, &o.mag),
            },
        }
        .norm()
    }

    pub fn sub(&self, o: &BigInt) -> BigInt {
        self.add(&o.clone().negated())
    }

    pub fn mul(&self, o: &BigInt) -> BigInt {
        let mut out = vec![0u32; self.mag.len() + o.mag.len()];
        for (i, a) in self.mag.iter().enumerate() {
            let mut carry = 0u64;
            for (j, b) in o.mag.iter().enumerate() {
                let v = out[i + j] as u64 + *a as u64 * *b as u64 + carry;
                out[i + j] = v as u32;
                carry = v >> 32;
            }
            let mut k = i + o.mag.len();
            while carry > 0 {
                let v = out[k] as u64 + carry;
                out[k] = v as u32;
                carry = v >> 32;
                k += 1;
            }
        }
        BigInt {
            neg: self.neg != o.neg,
            mag: out,
        }
        .norm()
    }

    fn bit_len(mag: &[u32]) -> usize {
        match mag.last() {
            None => 0,
            Some(top) => (mag.len() - 1) * 32 + (32 - top.leading_zeros() as usize),
        }
    }

    fn bit(mag: &[u32], i: usize) -> bool {
        (mag[i / 32] >> (i % 32)) & 1 == 1
    }

    /// Truncating division of magnitudes (shift-subtract): (quotient, remainder).
    fn divmod_mag(a: &[u32], b: &[u32]) -> (Vec<u32>, Vec<u32>) {
        if b.len() == 1 {
            let (q, r) = BigInt::divmod_small(a, b[0]);
            return (q, vec![r]);
        }
        let mut q = vec![0u32; a.len()];
        let mut r = BigInt::zero();
        for i in (0..BigInt::bit_len(a)).rev() {
            r = r.mul_small(2);
            if BigInt::bit(a, i) {
                r = r.add_small(1);
            }
            if BigInt::cmp_mag(&r.mag, b) != Ordering::Less {
                r = BigInt {
                    neg: false,
                    mag: BigInt::sub_mag(&r.mag, b),
                }
                .norm();
                q[i / 32] |= 1 << (i % 32);
            }
        }
        (q, r.mag)
    }

    /// Truncating division: (quotient, remainder with the sign of `self`). `None` on zero.
    pub fn divmod_trunc(&self, o: &BigInt) -> Option<(BigInt, BigInt)> {
        if o.is_zero() {
            return None;
        }
        let (q, r) = BigInt::divmod_mag(&self.mag, &o.mag);
        let q = BigInt {
            neg: self.neg != o.neg,
            mag: q,
        }
        .norm();
        let r = BigInt {
            neg: self.neg,
            mag: r,
        }
        .norm();
        Some((q, r))
    }

    /// Floor division and the modulo with the divisor's sign (Python `//`, `%`).
    pub fn divmod_floor(&self, o: &BigInt) -> Option<(BigInt, BigInt)> {
        let (q, r) = self.divmod_trunc(o)?;
        if !r.is_zero() && (r.neg != o.neg) {
            Some((q.sub(&BigInt::from_i64(1)), r.add(o)))
        } else {
            Some((q, r))
        }
    }

    pub fn pow10(n: u32) -> BigInt {
        let mut v = BigInt::from_i64(1);
        for _ in 0..n {
            v = v.mul_small(10);
        }
        v
    }

    /// The number of decimal digits of the magnitude (0 has 1).
    pub fn digits(&self) -> usize {
        self.to_string().trim_start_matches('-').len()
    }

    /// `BigInt.pow` (FOUNDER-QUEUE Q45): exact. `x.pow(0)` is 1 (`0.pow(0)` too); the bases
    /// 0, 1 and −1 take any exponent. A negative exponent is
    /// [`PowFail::NegativeExponent`]; a result of at least [`BIG_POW_MAX_BITS`] bits
    /// (judged from the base's bit length, [`big_pow_too_large`]) is
    /// [`PowFail::Overflow`], so a program cannot ask for an exabyte number.
    pub fn pow(&self, k: i64) -> Result<BigInt, PowFail> {
        if k < 0 {
            return Err(PowFail::NegativeExponent);
        }
        let one = BigInt::from_i64(1);
        if k == 0 {
            return Ok(one);
        }
        if self.is_zero() {
            return Ok(BigInt::zero());
        }
        if self.clone().abs() == one {
            return Ok(if self.neg && k % 2 == 1 {
                self.clone()
            } else {
                one
            });
        }
        if big_pow_too_large(BigInt::bit_len(&self.mag), k) {
            return Err(PowFail::Overflow);
        }
        let (mut acc, mut base, mut e) = (one, self.clone(), k as u64);
        while e > 0 {
            if e & 1 == 1 {
                acc = acc.mul(&base);
            }
            e >>= 1;
            if e > 0 {
                base = base.mul(&base);
            }
        }
        Ok(acc)
    }
}

/// The bit-length bound of `BigInt.pow` results (FOUNDER-QUEUE Q45 erratum).
pub const BIG_POW_MAX_BITS: u128 = 1 << 16;

/// The panic message of a `BigInt.pow` past [`BIG_POW_MAX_BITS`].
pub const BIG_POW_TOO_LARGE: &str = "BigInt.pow result too large (65536 bits or more)";

/// Whether `b.pow(k)` is refused for a base of `bits` bits (|b| >= 2, k >= 0): its result
/// has at least `(bits - 1) × k + 1` bits, and that reaches [`BIG_POW_MAX_BITS`]. The type
/// checker's constant evaluator applies the same rule.
pub fn big_pow_too_large(bits: usize, k: i64) -> bool {
    (bits as u128 - 1) * k as u128 + 1 >= BIG_POW_MAX_BITS
}

impl Ord for BigInt {
    fn cmp(&self, o: &BigInt) -> Ordering {
        match (self.neg, o.neg) {
            (false, true) => Ordering::Greater,
            (true, false) => Ordering::Less,
            (false, false) => BigInt::cmp_mag(&self.mag, &o.mag),
            (true, true) => BigInt::cmp_mag(&o.mag, &self.mag),
        }
    }
}

impl PartialOrd for BigInt {
    fn partial_cmp(&self, o: &BigInt) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}

impl fmt::Display for BigInt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_zero() {
            return f.write_str("0");
        }
        let mut chunks = Vec::new();
        let mut mag = self.mag.clone();
        while !mag.is_empty() {
            let (q, r) = BigInt::divmod_small(&mag, 1_000_000_000);
            chunks.push(r);
            mag = q;
            while mag.last() == Some(&0) {
                mag.pop();
            }
        }
        let mut s = String::new();
        if self.neg {
            s.push('-');
        }
        s.push_str(&chunks.pop().expect("non-zero").to_string());
        for c in chunks.iter().rev() {
            s.push_str(&format!("{c:09}"));
        }
        f.write_str(&s)
    }
}

// ---------------------------------------------------------------------------------------
// Decimal: coefficient × 10^exponent, at most 34 significant digits, exponents within the
// decimal128 range. Arithmetic is exact; a result that cannot be held exactly traps.
// ---------------------------------------------------------------------------------------

pub const DEC_DIGITS: usize = 34;
const DEC_EXP_MIN: i64 = -6176;
const DEC_EXP_MAX: i64 = 6111;
/// An exponent beyond this magnitude (in a literal, in `Decimal.parse` text, or computed by
/// `pow`, `div` or `round`) is read as this magnitude: a non-zero value there is out of
/// range either way, and zero clamps to the range either way.
const EXP_SATURATE: i64 = 1_000_000_000_000_000;
/// `div` and `round` never scale a coefficient by more than this power of ten: beyond it the
/// result is decided without building the number (see `div`).
const SCALE_LIMIT: i64 = 20_000;

/// Why a `Decimal` result cannot be held (FOUNDER-QUEUE Q48: two messages).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecFail {
    /// The exact value needs more than 34 significant digits.
    Digits,
    /// The value has at most 34 significant digits, but its exponent is outside the
    /// decimal128 range even after clamping (IEEE 754: zeros move between the coefficient
    /// and the exponent while the coefficient keeps at most 34 digits).
    Exponent,
}

impl DecFail {
    /// The panic message.
    pub fn message(self) -> &'static str {
        match self {
            DecFail::Digits => "Decimal overflow (more than 34 digits)",
            DecFail::Exponent => "Decimal exponent out of range",
        }
    }
}

/// Why `Decimal.div` has no result.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DivFail {
    DivisionByZero,
    Overflow(DecFail),
}

/// Why `Decimal.pow` has no result.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecPowFail {
    NegativeExponent,
    Overflow(DecFail),
}

/// The scale (number of decimal places) is kept, as in IEEE decimal (`1.50` shows as
/// `1.50`); equality and order compare values (`1.5 == 1.50`).
#[derive(Clone, Debug)]
pub struct Decimal {
    coef: BigInt,
    exp: i64,
}

/// `Decimal` rounding modes (`std/decimal.Rounding`, in declaration order).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rounding {
    HalfEven,
    HalfUp,
    Down,
    Up,
    Floor,
    Ceiling,
}

/// An `i128` exponent saturated to [`EXP_SATURATE`].
fn saturate(e: i128) -> i64 {
    e.clamp(-i128::from(EXP_SATURATE), i128::from(EXP_SATURATE)) as i64
}

/// An exponent's text: an optional sign, then ASCII digits; saturates at [`EXP_SATURATE`].
fn parse_exponent(text: &str) -> Option<i64> {
    let (neg, digits) = match text.as_bytes().first() {
        Some(b'-') => (true, &text[1..]),
        Some(b'+') => (false, &text[1..]),
        _ => (false, text),
    };
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let mut v: i64 = 0;
    for b in digits.bytes() {
        v = (v * 10 + i64::from(b - b'0')).min(EXP_SATURATE);
    }
    Some(if neg { -v } else { v })
}

impl Decimal {
    /// The value `coef × 10^exp` as decimal128 holds it, or why it cannot be held. Exact
    /// reductions only: trailing zeros leave a coefficient longer than 34 digits, then the
    /// exponent is clamped into range by moving zeros into (or, exactly, out of) the
    /// coefficient. Zero takes any exponent, clamped.
    fn make(coef: BigInt, exp: i64) -> Result<Decimal, DecFail> {
        let mut d = Decimal { coef, exp };
        let n = d.coef.digits();
        if n > DEC_DIGITS {
            let extra = n - DEC_DIGITS;
            let (q, r) = d
                .coef
                .divmod_trunc(&BigInt::pow10(extra as u32))
                .expect("10^k is not 0");
            if !r.is_zero() {
                return Err(DecFail::Digits);
            }
            d.coef = q;
            d.exp = d.exp.saturating_add(extra as i64);
        }
        if d.coef.is_zero() {
            d.exp = d.exp.clamp(DEC_EXP_MIN, DEC_EXP_MAX);
            return Ok(d);
        }
        if d.exp > DEC_EXP_MAX {
            // Fold-down clamping: `1e6144` is 10^33 × 10^6111.
            let pad = d.exp - DEC_EXP_MAX;
            if pad > (DEC_DIGITS - d.coef.digits()) as i64 {
                return Err(DecFail::Exponent);
            }
            d.coef = d.coef.mul(&BigInt::pow10(pad as u32));
            d.exp = DEC_EXP_MAX;
        } else if d.exp < DEC_EXP_MIN {
            // Exact only: a non-zero coefficient of at most 34 digits has at most 33
            // trailing zeros to give up.
            let need = DEC_EXP_MIN - d.exp;
            if need >= DEC_DIGITS as i64 {
                return Err(DecFail::Exponent);
            }
            let (q, r) = d
                .coef
                .divmod_trunc(&BigInt::pow10(need as u32))
                .expect("10^k is not 0");
            if !r.is_zero() {
                return Err(DecFail::Exponent);
            }
            d.coef = q;
            d.exp = DEC_EXP_MIN;
        }
        Ok(d)
    }

    /// `make(coef × 10^zeros, exp)` (`zeros` >= 0) without building zeros that `make` would
    /// drop again.
    fn make_padded(coef: BigInt, zeros: i128, exp: i128) -> Result<Decimal, DecFail> {
        if coef.is_zero() {
            return Decimal::make(coef, saturate(exp));
        }
        let (mut coef, mut exp) = (coef, exp);
        let n = coef.digits();
        if n > DEC_DIGITS {
            let extra = n - DEC_DIGITS;
            let (q, r) = coef
                .divmod_trunc(&BigInt::pow10(extra as u32))
                .expect("10^k is not 0");
            if !r.is_zero() {
                return Err(DecFail::Digits);
            }
            coef = q;
            exp += extra as i128;
        }
        let room = (DEC_DIGITS - coef.digits()) as i128;
        let pad = zeros.clamp(0, room);
        let exp = saturate(exp + zeros - pad);
        Decimal::make(coef.mul(&BigInt::pow10(pad as u32)), exp)
    }

    pub fn from_i64(v: i64) -> Decimal {
        Decimal {
            coef: BigInt::from_i64(v),
            exp: 0,
        }
    }

    /// A decimal literal's text (`19.99`, `-5`, `1.5e3`), or `Decimal.parse` input: an
    /// optional `-`, ASCII digits, an optional `.` and digits, an optional exponent (an
    /// optional sign and digits). The exact value written, clamped as decimal128 clamps;
    /// [`ParseFail::OutOfRange`] when it cannot be held. The type checker accepts exactly
    /// the `Decimal` literals this accepts (K0126).
    pub fn parse(text: &str) -> Result<Decimal, ParseFail> {
        if text.is_empty() {
            return Err(ParseFail::Empty);
        }
        let (neg, body) = match text.strip_prefix('-') {
            Some(r) => (true, r),
            None => (false, text),
        };
        let (mantissa, exp) = match body.find(['e', 'E']) {
            Some(i) => (
                &body[..i],
                parse_exponent(&body[i + 1..]).ok_or(ParseFail::Invalid)?,
            ),
            None => (body, 0),
        };
        let (int, frac) = match mantissa.split_once('.') {
            Some((a, b)) => (a, b),
            None => (mantissa, ""),
        };
        let ok = |s: &str| s.chars().all(|c| c.is_ascii_digit());
        if int.is_empty() || !ok(int) || !ok(frac) || (mantissa.contains('.') && frac.is_empty()) {
            return Err(ParseFail::Invalid);
        }
        let mut exp = saturate(i128::from(exp) - frac.len() as i128);
        let all: Vec<u8> = int.bytes().chain(frac.bytes()).collect();
        let first = all.iter().position(|&b| b != b'0').unwrap_or(all.len());
        let mut digits = &all[first..];
        if digits.len() > DEC_DIGITS {
            // Only trailing zeros may go (into the exponent), as `make` drops them.
            if digits[DEC_DIGITS..].iter().any(|&b| b != b'0') {
                return Err(ParseFail::OutOfRange);
            }
            exp = saturate(i128::from(exp) + (digits.len() - DEC_DIGITS) as i128);
            digits = &digits[..DEC_DIGITS];
        }
        let mut coef = BigInt::zero();
        for b in digits {
            coef = coef.mul_small(10).add_small(u32::from(b - b'0'));
        }
        if neg {
            coef = coef.negated();
        }
        Decimal::make(coef, exp).map_err(|_| ParseFail::OutOfRange)
    }

    /// Both coefficients at the smaller exponent.
    fn aligned(&self, o: &Decimal) -> (BigInt, BigInt, i64) {
        let e = self.exp.min(o.exp);
        let a = self.coef.mul(&BigInt::pow10((self.exp - e) as u32));
        let b = o.coef.mul(&BigInt::pow10((o.exp - e) as u32));
        (a, b, e)
    }

    pub fn add(&self, o: &Decimal) -> Result<Decimal, DecFail> {
        let (a, b, e) = self.aligned(o);
        Decimal::make(a.add(&b), e)
    }

    pub fn sub(&self, o: &Decimal) -> Result<Decimal, DecFail> {
        let (a, b, e) = self.aligned(o);
        Decimal::make(a.sub(&b), e)
    }

    pub fn mul(&self, o: &Decimal) -> Result<Decimal, DecFail> {
        Decimal::make(self.coef.mul(&o.coef), self.exp + o.exp)
    }

    /// `self.pow(k)` (FOUNDER-QUEUE Q45): exactly `coef^k × 10^(exp·k)`, held as `make`
    /// holds it, so `x.pow(3)` is `x * x * x` in value, scale and trap. `x.pow(0)` is 1; a
    /// negative exponent panics, as for `Int.pow`.
    pub fn pow(&self, k: i64) -> Result<Decimal, DecPowFail> {
        if k < 0 {
            return Err(DecPowFail::NegativeExponent);
        }
        if k == 0 {
            return Ok(Decimal::from_i64(1));
        }
        if self.coef.is_zero() {
            let exp = saturate(i128::from(self.exp) * i128::from(k));
            return Decimal::make(BigInt::zero(), exp).map_err(DecPowFail::Overflow);
        }
        // coef = sig × 10^tz, where `sig` has no factor 10 (and so no power of it has one).
        let (mut sig, mut tz) = (self.coef.clone(), 0i128);
        loop {
            let (q, r) = sig
                .divmod_trunc(&BigInt::from_i64(10))
                .expect("10 is not 0");
            if !r.is_zero() {
                break;
            }
            sig = q;
            tz += 1;
        }
        let mut p = BigInt::from_i64(1);
        if sig.clone().abs() == p {
            if sig.is_negative() && k % 2 == 1 {
                p = p.negated();
            }
        } else {
            // |sig| >= 2: at most 113 rounds before the power passes 34 digits.
            for _ in 0..k {
                p = p.mul(&sig);
                if p.digits() > DEC_DIGITS {
                    return Err(DecPowFail::Overflow(DecFail::Digits));
                }
            }
        }
        let k = i128::from(k);
        Decimal::make_padded(p, tz * k, i128::from(self.exp) * k).map_err(DecPowFail::Overflow)
    }

    pub fn negated(&self) -> Decimal {
        Decimal {
            coef: self.coef.clone().negated(),
            exp: self.exp,
        }
    }

    /// `num / den` rounded to an integer with `mode` (`den` > 0 after sign handling).
    fn div_round(num: &BigInt, den: &BigInt, mode: Rounding) -> Option<BigInt> {
        let (q, r) = num.divmod_trunc(den)?;
        if r.is_zero() {
            return Some(q);
        }
        let negative = num.is_negative() != den.is_negative();
        let twice = r.clone().abs().mul(&BigInt::from_i64(2));
        let half = twice.cmp(&den.clone().abs());
        let away = match mode {
            Rounding::Down => false,
            Rounding::Up => true,
            Rounding::Floor => negative,
            Rounding::Ceiling => !negative,
            Rounding::HalfUp => half != Ordering::Less,
            Rounding::HalfEven => match half {
                Ordering::Greater => true,
                Ordering::Less => false,
                Ordering::Equal => {
                    let (_, odd) = q.divmod_trunc(&BigInt::from_i64(2))?;
                    !odd.is_zero()
                }
            },
        };
        Some(if !away {
            q
        } else if negative {
            q.sub(&BigInt::from_i64(1))
        } else {
            q.add(&BigInt::from_i64(1))
        })
    }

    /// `self.div(other, scale, rounding)`: the quotient rounded to `scale` decimal places.
    pub fn div(&self, o: &Decimal, scale: i64, mode: Rounding) -> Result<Decimal, DivFail> {
        if o.coef.is_zero() {
            return Err(DivFail::DivisionByZero);
        }
        let exp = -i128::from(scale);
        if self.coef.is_zero() {
            return Decimal::make(BigInt::zero(), saturate(exp)).map_err(DivFail::Overflow);
        }
        // value × 10^scale = c1 × 10^k / c2
        let k = i128::from(self.exp) - i128::from(o.exp) + i128::from(scale);
        let limit = i128::from(SCALE_LIMIT);
        if k > limit {
            // An exact quotient keeps its zeros past the limit; an inexact one has more
            // than 34 significant digits there (the repeating expansion of a fraction whose
            // denominator is below 10^34 has no run of 34 zeros or nines).
            let num = self.coef.mul(&BigInt::pow10(SCALE_LIMIT as u32));
            let (q, r) = num.divmod_trunc(&o.coef).expect("divisor is not 0");
            if !r.is_zero() {
                return Err(DivFail::Overflow(DecFail::Digits));
            }
            return Decimal::make_padded(q, k - limit, exp).map_err(DivFail::Overflow);
        }
        // A non-zero quotient below 10^-limit rounds as any quotient below one half does.
        let k = k.max(-limit);
        let (num, den) = if k >= 0 {
            (self.coef.mul(&BigInt::pow10(k as u32)), o.coef.clone())
        } else {
            (self.coef.clone(), o.coef.mul(&BigInt::pow10((-k) as u32)))
        };
        let q = Decimal::div_round(&num, &den, mode).expect("divisor is not 0");
        Decimal::make(q, saturate(exp)).map_err(DivFail::Overflow)
    }

    /// `self.round(scale, rounding)`.
    pub fn round(&self, scale: i64, mode: Rounding) -> Result<Decimal, DecFail> {
        let exp = -i128::from(scale);
        if self.coef.is_zero() {
            return Decimal::make(BigInt::zero(), saturate(exp));
        }
        let k = i128::from(self.exp) + i128::from(scale);
        if k >= 0 {
            return Decimal::make_padded(self.coef.clone(), k, exp);
        }
        // A non-zero value below 10^-limit rounds as any value below one half does.
        let k = (-k).min(i128::from(SCALE_LIMIT));
        let q =
            Decimal::div_round(&self.coef, &BigInt::pow10(k as u32), mode).expect("10^k is not 0");
        Decimal::make(q, saturate(exp))
    }

    /// The value with trailing zeros removed (for hashing: `1.5` and `1.50` are one key).
    pub fn canonical(&self) -> (BigInt, i64) {
        let mut c = self.coef.clone();
        let mut e = self.exp;
        if c.is_zero() {
            return (c, 0);
        }
        loop {
            let (q, r) = c.divmod_trunc(&BigInt::from_i64(10)).expect("10 is not 0");
            if !r.is_zero() {
                break;
            }
            c = q;
            e += 1;
        }
        (c, e)
    }
}

impl PartialEq for Decimal {
    fn eq(&self, o: &Decimal) -> bool {
        self.cmp(o) == Ordering::Equal
    }
}

impl Eq for Decimal {}

impl Ord for Decimal {
    fn cmp(&self, o: &Decimal) -> Ordering {
        let (a, b, _) = self.aligned(o);
        a.cmp(&b)
    }
}

impl PartialOrd for Decimal {
    fn partial_cmp(&self, o: &Decimal) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}

impl fmt::Display for Decimal {
    /// Plain notation with the scale kept: `19.99`, `-0.50`, `1000` (never an exponent).
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let digits = self.coef.clone().abs().to_string();
        let sign = if self.coef.is_negative() { "-" } else { "" };
        if self.exp >= 0 {
            let zeros = if self.coef.is_zero() {
                String::new()
            } else {
                "0".repeat(self.exp as usize)
            };
            return write!(f, "{sign}{digits}{zeros}");
        }
        let places = (-self.exp) as usize;
        let padded = if digits.len() <= places {
            format!("{}{digits}", "0".repeat(places - digits.len() + 1))
        } else {
            digits
        };
        let (int, frac) = padded.split_at(padded.len() - places);
        write!(f, "{sign}{int}.{frac}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn int_division_follows_python_semantics_and_traps() {
        assert_eq!(int_floor_div(-7, 2), Some(-4));
        assert_eq!(int_mod(-7, 2), Some(1));
        assert_eq!(int_mod(7, -2), Some(-1));
        assert_eq!(int_floor_div(i64::MIN, -1), None);
        assert_eq!(int_mod(i64::MIN, -1), Some(0));
        assert_eq!(int_floor_div(1, 0), None);
        assert_eq!(int_mod(1, 0), None);
    }

    #[test]
    fn int_pow_is_exact_and_checked() {
        assert_eq!(int_pow(2, 62), Ok(1 << 62));
        assert_eq!(int_pow(2, 63), Err(PowFail::Overflow));
        assert_eq!(int_pow(-2, 63), Ok(i64::MIN));
        assert_eq!(int_pow(-2, 64), Err(PowFail::Overflow));
        assert_eq!(int_pow(0, 0), Ok(1));
        assert_eq!(int_pow(0, i64::MAX), Ok(0));
        assert_eq!(int_pow(1, i64::MAX), Ok(1));
        assert_eq!(int_pow(-1, i64::MAX), Ok(-1));
        assert_eq!(int_pow(-1, i64::MAX - 1), Ok(1));
        assert_eq!(int_pow(2, i64::MAX), Err(PowFail::Overflow));
        assert_eq!(int_pow(3, -1), Err(PowFail::NegativeExponent));
        assert_eq!(int_pow(1, -1), Err(PowFail::NegativeExponent));
        assert_eq!(int_pow(3_037_000_499, 2), Ok(9_223_372_030_926_249_001));
        assert_eq!(int_pow(3_037_000_500, 2), Err(PowFail::Overflow));
    }

    #[test]
    fn shift_amounts_are_0_to_63() {
        assert_eq!(shift_amount(0), Some(0));
        assert_eq!(shift_amount(63), Some(63));
        assert_eq!(shift_amount(64), None);
        assert_eq!(shift_amount(-1), None);
        assert_eq!(shift_amount(i64::MIN), None);
    }

    #[test]
    fn float_to_int_rejects_the_exclusive_upper_bound() {
        assert_eq!(float_to_int(-2.7), Some(-2));
        assert_eq!(float_to_int(9_223_372_036_854_775_808.0), None);
        assert_eq!(float_to_int(-9_223_372_036_854_775_808.0), Some(i64::MIN));
        assert_eq!(float_to_int(f64::NAN), None);
    }

    #[test]
    fn parse_int_accepts_one_digit_system_and_the_full_range() {
        assert_eq!(parse_int("42"), Ok(42));
        assert_eq!(parse_int("٤٢"), Ok(42));
        assert_eq!(parse_int("-7"), Ok(-7));
        assert_eq!(parse_int("9223372036854775807"), Ok(i64::MAX));
        assert_eq!(parse_int("-9223372036854775808"), Ok(i64::MIN));
        assert_eq!(parse_int("9223372036854775808"), Err(ParseFail::OutOfRange));
        assert_eq!(parse_int(""), Err(ParseFail::Empty));
        assert_eq!(parse_int("-"), Err(ParseFail::Invalid));
        assert_eq!(parse_int("+1"), Err(ParseFail::Invalid));
        assert_eq!(parse_int("4٢"), Err(ParseFail::Invalid));
        assert_eq!(parse_int("۴۲"), Err(ParseFail::Invalid)); // Persian digits
        assert_eq!(parse_int("1.5"), Err(ParseFail::Invalid));
    }

    #[test]
    fn bigint_arithmetic_is_exact() {
        let a = parse_big("123456789012345678901234567890").unwrap();
        let b = parse_big("-987654321098765432109876543210").unwrap();
        assert_eq!(
            a.mul(&b).to_string(),
            "-121932631137021795226185032733622923332237463801111263526900"
        );
        assert_eq!(a.add(&b).to_string(), "-864197532086419753208641975320");
        let (q, r) = b.divmod_floor(&a).unwrap();
        assert_eq!(
            (q.to_string(), r.to_string()),
            ("-9".into(), "123456780012345678001234567800".into())
        );
        let (q, r) = BigInt::from_i64(-7)
            .divmod_floor(&BigInt::from_i64(2))
            .unwrap();
        assert_eq!((q.to_string(), r.to_string()), ("-4".into(), "1".into()));
        assert!(a.divmod_floor(&BigInt::zero()).is_none());
        assert_eq!(
            BigInt::from_i64(i64::MIN).to_string(),
            "-9223372036854775808"
        );
        assert_eq!(BigInt::from_i64(i64::MIN).to_i64(), Some(i64::MIN));
        assert_eq!(
            BigInt::from_i64(i64::MAX)
                .add(&BigInt::from_i64(1))
                .to_i64(),
            None
        );
    }

    #[test]
    fn decimal_is_exact_keeps_scale_and_rounds() {
        let d = |s: &str| Decimal::parse(s).unwrap();
        assert_eq!(d("0.1").add(&d("0.2")).unwrap().to_string(), "0.3");
        assert_eq!(d("19.99").mul(&d("3")).unwrap().to_string(), "59.97");
        assert_eq!(d("1.50").to_string(), "1.50");
        assert_eq!(d("1.5"), d("1.50"));
        assert_eq!(d("-0.005").to_string(), "-0.005");
        assert_eq!(d("1e3").to_string(), "1000");
        assert_eq!(
            d("10")
                .div(&d("3"), 2, Rounding::HalfEven)
                .unwrap()
                .to_string(),
            "3.33"
        );
        assert_eq!(
            d("2.5").round(0, Rounding::HalfEven).unwrap().to_string(),
            "2"
        );
        assert_eq!(
            d("2.5").round(0, Rounding::HalfUp).unwrap().to_string(),
            "3"
        );
        assert_eq!(
            d("-2.5").round(0, Rounding::Floor).unwrap().to_string(),
            "-3"
        );
        assert_eq!(
            d("1").div(&d("0"), 2, Rounding::Down).unwrap_err(),
            DivFail::DivisionByZero
        );
        // 35 significant digits cannot be held exactly.
        let big = d("9999999999999999999999999999999999");
        assert_eq!(big.add(&d("0.1")).unwrap_err(), DecFail::Digits);
        assert!(big.mul(&d("10")).is_ok()); // trailing zero: exact
    }

    /// Decimal literals and `Decimal.parse` text at the edges of decimal128 (Q48, Q52):
    /// `(text, Ok(coefficient digits, exponent) | Err(…))`. The type checker's constant
    /// evaluator (`kalam-typeck` `const_num`) has the same table.
    #[test]
    fn decimal128_clamping_vectors() {
        let thousand_zeros = format!("1{}", "0".repeat(2000));
        let not_zeros = format!("1{}1", "0".repeat(2000));
        type Want = Result<(String, i64), ParseFail>;
        let cases: Vec<(&str, Want)> = vec![
            ("1e6111", Ok(("1".into(), 6111))),
            ("1e6112", Ok(("10".into(), 6111))),
            ("1e6144", Ok((format!("1{}", "0".repeat(33)), 6111))),
            ("12e6143", Ok((format!("12{}", "0".repeat(32)), 6111))),
            ("1e6145", Err(ParseFail::OutOfRange)),
            ("123e6143", Err(ParseFail::OutOfRange)),
            ("1e-6176", Ok(("1".into(), -6176))),
            ("1e-6177", Err(ParseFail::OutOfRange)),
            ("10e-6177", Ok(("1".into(), -6176))),
            ("1.0e-6176", Ok(("1".into(), -6176))),
            ("0e99999999999999999999999", Ok(("0".into(), 6111))),
            ("0.000e-99999", Ok(("0".into(), -6176))),
            ("1e99999999999999999999999", Err(ParseFail::OutOfRange)),
            ("1e-99999999999999999999999", Err(ParseFail::OutOfRange)),
            (&thousand_zeros, Ok((format!("1{}", "0".repeat(33)), 1967))),
            (&not_zeros, Err(ParseFail::OutOfRange)),
            (
                "00000000000000000000000000000000000000001.5",
                Ok(("15".into(), -1)),
            ),
            ("1.50", Ok(("150".into(), -2))),
            ("-2.5e+3", Ok(("-25".into(), 2))),
            ("1e", Err(ParseFail::Invalid)),
            ("1e+", Err(ParseFail::Invalid)),
            ("1e+-5", Err(ParseFail::Invalid)),
            ("1.", Err(ParseFail::Invalid)),
            (".5", Err(ParseFail::Invalid)),
        ];
        for (text, want) in cases {
            let got = Decimal::parse(text).map(|d| (d.coef.to_string(), d.exp));
            assert_eq!(got, want, "{text}");
        }
        // Arithmetic results are clamped the same way, and the two traps are told apart.
        let d = |s: &str| Decimal::parse(s).unwrap();
        assert_eq!(d("1e6111").mul(&d("10")).unwrap().exp, 6111);
        assert_eq!(d("1e6144").mul(&d("10")).unwrap_err(), DecFail::Exponent);
        assert_eq!(
            d("1e6111").mul(&d("1e6111")).unwrap_err(),
            DecFail::Exponent
        );
        assert_eq!(d("1e-6176").mul(&d("0.1")).unwrap_err(), DecFail::Exponent);
        assert_eq!(
            d("9999999999999999999999999999999999")
                .add(&d("2"))
                .unwrap_err(),
            DecFail::Digits
        );
        assert_eq!(DecFail::Exponent.message(), "Decimal exponent out of range");
    }

    #[test]
    fn decimal_pow_is_repeated_multiplication() {
        let d = |s: &str| Decimal::parse(s).unwrap();
        let shown = |r: Result<Decimal, DecFail>| r.map(|x| format!("{}e{}", x.coef, x.exp));
        for base in [
            "1.50",
            "-2",
            "0.00",
            "10",
            "-1.0",
            "7e-3",
            "123456789",
            "1e6000",
            "2e-3000",
            "9999999999999999",
            "100",
        ] {
            let x = d(base);
            let mut acc = Ok(Decimal::from_i64(1));
            for k in 0..=6 {
                let pow = x.pow(k).map_err(|f| match f {
                    DecPowFail::Overflow(f) => f,
                    DecPowFail::NegativeExponent => unreachable!(),
                });
                assert_eq!(shown(pow), shown(acc.clone()), "{base}.pow({k})");
                acc = acc.and_then(|a: Decimal| a.mul(&x));
            }
        }
        assert_eq!(d("1.50").pow(2).unwrap().to_string(), "2.2500");
        assert!(d("2").pow(112).is_ok());
        assert_eq!(
            d("2").pow(113).unwrap_err(),
            DecPowFail::Overflow(DecFail::Digits)
        );
        assert_eq!(d("10").pow(6144).unwrap().exp, 6111);
        assert_eq!(
            d("10").pow(6145).unwrap_err(),
            DecPowFail::Overflow(DecFail::Exponent)
        );
        assert!(d("0.1").pow(6176).is_ok());
        assert_eq!(
            d("0.1").pow(6177).unwrap_err(),
            DecPowFail::Overflow(DecFail::Exponent)
        );
        assert_eq!(d("-1").pow(i64::MAX).unwrap().to_string(), "-1");
        // 1.0^k is 1 with as many places as 34 digits hold (as `1.0 * 1.0 * …` is).
        assert_eq!(
            d("1.0").pow(i64::MAX).unwrap().to_string(),
            format!("1.{}", "0".repeat(33))
        );
        assert_eq!(d("0").pow(0).unwrap().to_string(), "1");
        assert_eq!(
            d("0.0").pow(i64::MAX).unwrap().to_string(),
            d("0e-6176").to_string()
        );
        assert_eq!(d("3").pow(-1).unwrap_err(), DecPowFail::NegativeExponent);
    }

    #[test]
    fn bigint_pow_is_exact_and_bounded() {
        let b = BigInt::from_i64;
        assert_eq!(
            b(2).pow(100).unwrap().to_string(),
            "1267650600228229401496703205376"
        );
        assert_eq!(b(-3).pow(3).unwrap(), b(-27));
        assert_eq!(b(0).pow(0).unwrap(), b(1));
        assert_eq!(b(0).pow(i64::MAX).unwrap(), b(0));
        assert_eq!(b(-1).pow(i64::MAX).unwrap(), b(-1));
        assert_eq!(b(1).pow(i64::MAX).unwrap(), b(1));
        assert_eq!(b(2).pow(-1).unwrap_err(), PowFail::NegativeExponent);
        assert_eq!(b(2).pow(65534).unwrap().to_string().len(), 19728);
        assert_eq!(b(2).pow(65535).unwrap_err(), PowFail::Overflow);
        assert_eq!(b(3).pow(i64::MAX).unwrap_err(), PowFail::Overflow);
        assert!(big_pow_too_large(2, 65535));
        assert!(!big_pow_too_large(2, 65534));
    }

    #[test]
    fn decimal_div_and_round_handle_extreme_scales() {
        let d = |s: &str| Decimal::parse(s).unwrap();
        let ok = |r: Result<Decimal, DivFail>| r.unwrap().to_string();
        // The quotient 1e6000 at 15000 places: exact, so its zeros are kept as the exponent.
        assert_eq!(
            ok(d("1e6000").div(&d("1"), 15000, Rounding::Down)),
            d("1e6000").to_string()
        );
        assert_eq!(
            d("1").div(&d("3"), i64::MAX, Rounding::Down).unwrap_err(),
            DivFail::Overflow(DecFail::Digits)
        );
        // 1/4 is exact: 0.25 with as many places as 34 digits hold.
        assert_eq!(
            ok(d("1").div(&d("4"), i64::MAX, Rounding::Down)),
            format!("0.25{}", "0".repeat(32))
        );
        assert_eq!(
            d("1")
                .div(&d("1e-6176"), i64::MAX, Rounding::Down)
                .unwrap_err(),
            DivFail::Overflow(DecFail::Exponent)
        );
        assert_eq!(
            ok(d("1").div(&d("3"), i64::MIN, Rounding::Down)),
            d("0e6111").to_string()
        );
        assert_eq!(
            d("1").div(&d("3"), i64::MIN, Rounding::Up).unwrap_err(),
            DivFail::Overflow(DecFail::Exponent)
        );
        assert_eq!(
            d("1e6000")
                .round(15000, Rounding::Down)
                .unwrap()
                .to_string(),
            d("1e6000").to_string()
        );
        assert_eq!(
            d("1e-6000").round(-15000, Rounding::Up).unwrap_err(),
            DecFail::Exponent
        );
        assert!(d("1e-6000").round(-15000, Rounding::Down).is_ok());
        assert!(d("5").round(i64::MIN, Rounding::Down).is_ok());
        assert_eq!(
            d("2.5").round(0, Rounding::HalfEven).unwrap().to_string(),
            "2"
        );
    }
}
