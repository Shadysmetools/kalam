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

impl Decimal {
    /// `None` when the value needs more than 34 digits or leaves the exponent range.
    fn make(coef: BigInt, exp: i64) -> Option<Decimal> {
        let mut d = Decimal { coef, exp };
        // Exact reductions only: drop trailing zeros while the coefficient is too long.
        while d.coef.digits() > DEC_DIGITS {
            let (q, r) = d.coef.divmod_trunc(&BigInt::from_i64(10))?;
            if !r.is_zero() {
                return None;
            }
            d.coef = q;
            d.exp += 1;
        }
        if d.coef.is_zero() {
            d.exp = d.exp.clamp(DEC_EXP_MIN, DEC_EXP_MAX);
        }
        if !(DEC_EXP_MIN..=DEC_EXP_MAX).contains(&d.exp) {
            return None;
        }
        Some(d)
    }

    pub fn from_i64(v: i64) -> Decimal {
        Decimal {
            coef: BigInt::from_i64(v),
            exp: 0,
        }
    }

    /// A decimal literal's text (`19.99`, `-5`, `1.5e3`), or `Decimal.parse` input: an
    /// optional `-`, ASCII digits, an optional `.` and digits, an optional exponent.
    pub fn parse(text: &str) -> Result<Decimal, ParseFail> {
        if text.is_empty() {
            return Err(ParseFail::Empty);
        }
        let (neg, body) = match text.strip_prefix('-') {
            Some(r) => (true, r),
            None => (false, text),
        };
        let (mantissa, exp) = match body.find(['e', 'E']) {
            Some(i) => {
                let e: i64 = body[i + 1..].parse().map_err(|_| ParseFail::Invalid)?;
                (&body[..i], e)
            }
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
        let mut coef = BigInt::zero();
        for c in int.chars().chain(frac.chars()) {
            coef = coef.mul_small(10).add_small(c as u32 - '0' as u32);
        }
        if neg {
            coef = coef.negated();
        }
        let exp = exp
            .checked_sub(frac.len() as i64)
            .ok_or(ParseFail::OutOfRange)?;
        Decimal::make(coef, exp).ok_or(ParseFail::OutOfRange)
    }

    /// Both coefficients at the smaller exponent.
    fn aligned(&self, o: &Decimal) -> (BigInt, BigInt, i64) {
        let e = self.exp.min(o.exp);
        let a = self.coef.mul(&BigInt::pow10((self.exp - e) as u32));
        let b = o.coef.mul(&BigInt::pow10((o.exp - e) as u32));
        (a, b, e)
    }

    pub fn add(&self, o: &Decimal) -> Option<Decimal> {
        let (a, b, e) = self.aligned(o);
        Decimal::make(a.add(&b), e)
    }

    pub fn sub(&self, o: &Decimal) -> Option<Decimal> {
        let (a, b, e) = self.aligned(o);
        Decimal::make(a.sub(&b), e)
    }

    pub fn mul(&self, o: &Decimal) -> Option<Decimal> {
        Decimal::make(self.coef.mul(&o.coef), self.exp + o.exp)
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

    /// `self.div(other, scale, rounding)`: the quotient rounded to `scale` decimal
    /// places. `Err(true)` on division by zero, `Err(false)` when the result cannot be held.
    pub fn div(&self, o: &Decimal, scale: i64, mode: Rounding) -> Result<Decimal, bool> {
        if o.coef.is_zero() {
            return Err(true);
        }
        // value × 10^scale = c1 × 10^(e1 − e2 + scale) / c2
        let k = self.exp - o.exp + scale;
        if k.unsigned_abs() > 20_000 {
            return Err(false);
        }
        let (num, den) = if k >= 0 {
            (self.coef.mul(&BigInt::pow10(k as u32)), o.coef.clone())
        } else {
            (self.coef.clone(), o.coef.mul(&BigInt::pow10((-k) as u32)))
        };
        let q = Decimal::div_round(&num, &den, mode).ok_or(false)?;
        Decimal::make(q, -scale).ok_or(false)
    }

    /// `self.round(scale, rounding)`.
    pub fn round(&self, scale: i64, mode: Rounding) -> Option<Decimal> {
        let k = self.exp + scale;
        if k >= 0 {
            if k > 20_000 {
                return None;
            }
            return Decimal::make(self.coef.mul(&BigInt::pow10(k as u32)), -scale);
        }
        if -k > 20_000 {
            return Decimal::make(BigInt::zero(), -scale);
        }
        let q = Decimal::div_round(&self.coef, &BigInt::pow10((-k) as u32), mode)?;
        Decimal::make(q, -scale)
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
        assert!(d("1").div(&d("0"), 2, Rounding::Down).unwrap_err());
        // 35 significant digits cannot be held exactly.
        let big = d("9999999999999999999999999999999999");
        assert!(big.add(&d("0.1")).is_none());
        assert!(big.mul(&d("10")).is_some()); // trailing zero: exact
    }
}
