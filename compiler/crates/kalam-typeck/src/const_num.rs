//! Exact numbers for the constant evaluator (`consts`): a `BigInt` and a decimal128
//! `Decimal`, with the same rules as the reference interpreter's (`kalam-interp`'s `num`
//! module), so a constant the checker says would panic is one the interpreter panics on
//! (FOUNDER-QUEUE Q33, Q40). The interpreter depends on this crate, so the arithmetic is
//! mirrored here rather than shared.
//!
//! The type checker is a fuzz target, so every operation here is bounded: a `BigInt` result
//! larger than [`MAX_BIG_BITS`] is not computed (the caller stops evaluating that value),
//! and `Decimal` arithmetic never builds a number of more than about 120 digits (an exponent
//! gap too large for an exact result is decided without aligning the operands).

use std::cmp::Ordering;

/// The largest `BigInt` magnitude the evaluator keeps, in bits. Beyond it the value is not
/// evaluated (no verdict is guessed).
pub(crate) const MAX_BIG_BITS: usize = 4096;

// ---------------------------------------------------------------------------------------
// BigInt: sign and magnitude, base 2^32 limbs, little-endian, no trailing zero limbs.
// ---------------------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Big {
    /// Never true for zero.
    neg: bool,
    mag: Vec<u32>,
}

impl Big {
    pub(crate) fn zero() -> Big {
        Big {
            neg: false,
            mag: Vec::new(),
        }
    }

    pub(crate) fn from_u128(mut m: u128) -> Big {
        let mut mag = Vec::new();
        while m > 0 {
            mag.push(m as u32);
            m >>= 32;
        }
        Big { neg: false, mag }
    }

    #[cfg(test)]
    fn from_i64(v: i64) -> Big {
        let b = Big::from_u128(u128::from(v.unsigned_abs()));
        if v < 0 {
            b.negated()
        } else {
            b
        }
    }

    pub(crate) fn is_zero(&self) -> bool {
        self.mag.is_empty()
    }

    pub(crate) fn is_negative(&self) -> bool {
        self.neg
    }

    /// The number of bits of the magnitude.
    pub(crate) fn bits(&self) -> usize {
        match self.mag.last() {
            None => 0,
            Some(top) => (self.mag.len() - 1) * 32 + (32 - top.leading_zeros() as usize),
        }
    }

    fn norm(mut self) -> Big {
        while self.mag.last() == Some(&0) {
            self.mag.pop();
        }
        if self.mag.is_empty() {
            self.neg = false;
        }
        self
    }

    pub(crate) fn negated(self) -> Big {
        let neg = !self.neg;
        Big { neg, ..self }.norm()
    }

    fn abs(self) -> Big {
        Big { neg: false, ..self }
    }

    fn mul_small(&self, k: u32) -> Big {
        let mut out = Vec::with_capacity(self.mag.len() + 1);
        let mut carry: u64 = 0;
        for l in &self.mag {
            let v = u64::from(*l) * u64::from(k) + carry;
            out.push(v as u32);
            carry = v >> 32;
        }
        if carry > 0 {
            out.push(carry as u32);
        }
        Big {
            neg: self.neg,
            mag: out,
        }
        .norm()
    }

    /// Magnitude division by a small non-zero divisor: (quotient, remainder).
    fn divmod_small(mag: &[u32], k: u32) -> (Vec<u32>, u32) {
        let mut out = vec![0u32; mag.len()];
        let mut rem: u64 = 0;
        for i in (0..mag.len()).rev() {
            let cur = (rem << 32) | u64::from(mag[i]);
            out[i] = (cur / u64::from(k)) as u32;
            rem = cur % u64::from(k);
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
            let v = u64::from(*a.get(i).unwrap_or(&0)) + u64::from(*b.get(i).unwrap_or(&0)) + carry;
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
            let mut v = i64::from(*x) - i64::from(*b.get(i).unwrap_or(&0)) - borrow;
            borrow = 0;
            if v < 0 {
                v += 1 << 32;
                borrow = 1;
            }
            out.push(v as u32);
        }
        out
    }

    pub(crate) fn add(&self, o: &Big) -> Big {
        if self.neg == o.neg {
            return Big {
                neg: self.neg,
                mag: Big::add_mag(&self.mag, &o.mag),
            }
            .norm();
        }
        match Big::cmp_mag(&self.mag, &o.mag) {
            Ordering::Less => Big {
                neg: o.neg,
                mag: Big::sub_mag(&o.mag, &self.mag),
            },
            _ => Big {
                neg: self.neg,
                mag: Big::sub_mag(&self.mag, &o.mag),
            },
        }
        .norm()
    }

    pub(crate) fn sub(&self, o: &Big) -> Big {
        self.add(&o.clone().negated())
    }

    pub(crate) fn mul(&self, o: &Big) -> Big {
        let mut out = vec![0u32; self.mag.len() + o.mag.len()];
        for (i, a) in self.mag.iter().enumerate() {
            let mut carry = 0u64;
            for (j, b) in o.mag.iter().enumerate() {
                let v = u64::from(out[i + j]) + u64::from(*a) * u64::from(*b) + carry;
                out[i + j] = v as u32;
                carry = v >> 32;
            }
            let mut k = i + o.mag.len();
            while carry > 0 {
                let v = u64::from(out[k]) + carry;
                out[k] = v as u32;
                carry = v >> 32;
                k += 1;
            }
        }
        Big {
            neg: self.neg != o.neg,
            mag: out,
        }
        .norm()
    }

    /// Truncating division of magnitudes (shift-subtract): (quotient, remainder).
    fn divmod_mag(a: &[u32], b: &[u32]) -> (Vec<u32>, Vec<u32>) {
        if b.len() == 1 {
            let (q, r) = Big::divmod_small(a, b[0]);
            return (q, vec![r]);
        }
        let bits = Big {
            neg: false,
            mag: a.to_vec(),
        }
        .bits();
        let mut q = vec![0u32; a.len()];
        let mut r = Big::zero();
        for i in (0..bits).rev() {
            r = r.mul_small(2);
            if (a[i / 32] >> (i % 32)) & 1 == 1 {
                r = r.add(&Big::from_u128(1));
            }
            if Big::cmp_mag(&r.mag, b) != Ordering::Less {
                r = Big {
                    neg: false,
                    mag: Big::sub_mag(&r.mag, b),
                }
                .norm();
                q[i / 32] |= 1 << (i % 32);
            }
        }
        (q, r.mag)
    }

    /// Truncating division: (quotient, remainder with the sign of `self`). `None` on zero.
    fn divmod_trunc(&self, o: &Big) -> Option<(Big, Big)> {
        if o.is_zero() {
            return None;
        }
        let (q, r) = Big::divmod_mag(&self.mag, &o.mag);
        let q = Big {
            neg: self.neg != o.neg,
            mag: q,
        }
        .norm();
        let r = Big {
            neg: self.neg,
            mag: r,
        }
        .norm();
        Some((q, r))
    }

    /// Floor division and the modulo with the divisor's sign (Python `//`, `%`, §9.2).
    /// `None` when `o` is zero.
    pub(crate) fn divmod_floor(&self, o: &Big) -> Option<(Big, Big)> {
        let (q, r) = self.divmod_trunc(o)?;
        if !r.is_zero() && (r.neg != o.neg) {
            Some((q.sub(&Big::from_u128(1)), r.add(o)))
        } else {
            Some((q, r))
        }
    }

    pub(crate) fn pow10(n: u32) -> Big {
        let mut v = Big::from_u128(1);
        for _ in 0..n {
            v = v.mul_small(10);
        }
        v
    }

    /// The decimal digits of the magnitude, least significant first (`[0]` for zero).
    fn decimal_digits(&self) -> Vec<u8> {
        if self.is_zero() {
            return vec![0];
        }
        let mut out = Vec::new();
        let mut mag = self.mag.clone();
        while !mag.is_empty() {
            let (q, mut r) = Big::divmod_small(&mag, 1_000_000_000);
            mag = q;
            while mag.last() == Some(&0) {
                mag.pop();
            }
            for _ in 0..9 {
                out.push((r % 10) as u8);
                r /= 10;
            }
        }
        while out.len() > 1 && out.last() == Some(&0) {
            out.pop();
        }
        out
    }
}

/// The bit-length bound of `BigInt.pow` results, as the interpreter applies it
/// (`kalam_interp::num::BIG_POW_MAX_BITS`, FOUNDER-QUEUE Q45 erratum).
const BIG_POW_MAX_BITS: u128 = 1 << 16;

/// The interpreter's panic message for a `BigInt.pow` past [`BIG_POW_MAX_BITS`].
pub(crate) const BIG_POW_TOO_LARGE: &str = "BigInt.pow result too large (65536 bits or more)";

/// The outcome of `BigInt.pow` for the constant evaluator.
#[derive(Debug, PartialEq)]
pub(crate) enum BigPow {
    Value(Big),
    NegativeExponent,
    /// The interpreter refuses the size and panics with [`BIG_POW_TOO_LARGE`].
    TooLarge,
    /// Allowed, but beyond [`MAX_BIG_BITS`]: not evaluated.
    Unknown,
}

impl Big {
    /// `self.pow(k)` with the interpreter's rules (Q45): exact; `x.pow(0)` is 1; the bases
    /// 0, 1 and −1 take any exponent; for |x| >= 2 a result of at least
    /// `(bits - 1) × k + 1` bits reaching [`BIG_POW_MAX_BITS`] is refused.
    pub(crate) fn pow(&self, k: i64) -> BigPow {
        if k < 0 {
            return BigPow::NegativeExponent;
        }
        let one = Big::from_u128(1);
        if k == 0 {
            return BigPow::Value(one);
        }
        if self.is_zero() {
            return BigPow::Value(Big::zero());
        }
        if self.clone().abs() == one {
            return BigPow::Value(if self.neg && k % 2 == 1 {
                self.clone()
            } else {
                one
            });
        }
        let bits = self.bits() as u128;
        let k = k as u128;
        if (bits - 1) * k + 1 >= BIG_POW_MAX_BITS {
            return BigPow::TooLarge;
        }
        if bits * k > MAX_BIG_BITS as u128 {
            return BigPow::Unknown;
        }
        let (mut acc, mut base, mut e) = (one, self.clone(), k);
        while e > 0 {
            if e & 1 == 1 {
                acc = acc.mul(&base);
            }
            e >>= 1;
            if e > 0 {
                base = base.mul(&base);
            }
        }
        BigPow::Value(acc)
    }
}

/// The decimal digits of an integer literal's ASCII-normalized text (`_` separators,
/// `0x`/`0o`/`0b` prefixes allowed), for a literal too large for `u128`.
pub(crate) fn literal_digits(text: &str) -> String {
    let clean: String = text.chars().filter(|c| *c != '_').collect();
    let (radix, digits, chunk) = match clean.get(..2) {
        Some("0x") => (16, &clean[2..], 7),
        Some("0o") => (8, &clean[2..], 10),
        Some("0b") => (2, &clean[2..], 31),
        _ => return clean,
    };
    // Several digits per step: `radix^chunk` fits in a `u32`.
    let mut v = Big::zero();
    let chars: Vec<u32> = digits.chars().filter_map(|c| c.to_digit(radix)).collect();
    for part in chars.chunks(chunk) {
        let mut scale = 1u32;
        let mut add = 0u32;
        for d in part {
            scale *= radix;
            add = add * radix + d;
        }
        v = v.mul_small(scale).add(&Big::from_u128(u128::from(add)));
    }
    v.decimal_digits()
        .iter()
        .rev()
        .map(|d| char::from(b'0' + d))
        .collect()
}

impl Ord for Big {
    fn cmp(&self, o: &Big) -> Ordering {
        match (self.neg, o.neg) {
            (false, true) => Ordering::Greater,
            (true, false) => Ordering::Less,
            (false, false) => Big::cmp_mag(&self.mag, &o.mag),
            (true, true) => Big::cmp_mag(&o.mag, &self.mag),
        }
    }
}

impl PartialOrd for Big {
    fn partial_cmp(&self, o: &Big) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}

// ---------------------------------------------------------------------------------------
// Decimal: coefficient × 10^exponent, at most 34 significant digits, exponents within the
// decimal128 range. Arithmetic is exact; a result that cannot be held exactly traps.
// ---------------------------------------------------------------------------------------

const DEC_DIGITS: usize = 34;
const DEC_EXP_MIN: i64 = -6176;
const DEC_EXP_MAX: i64 = 6111;
/// Above this exponent gap, an exact sum of two non-zero values needs more than 34 digits
/// (it has at least `gap - 33` significant digits), so it is decided without aligning.
const DEC_ALIGN_LIMIT: i64 = 80;
/// An exponent beyond this magnitude is read as this magnitude, as the interpreter reads it:
/// a non-zero value there is out of range either way, and zero clamps either way.
const EXP_SATURATE: i64 = 1_000_000_000_000_000;

/// Why a `Decimal` result cannot be held (FOUNDER-QUEUE Q48), with the interpreter's panic
/// messages.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DecFail {
    /// The exact value needs more than 34 significant digits.
    Digits,
    /// The exponent is outside the decimal128 range even after clamping.
    Exponent,
}

impl DecFail {
    pub(crate) fn message(self) -> &'static str {
        match self {
            DecFail::Digits => "Decimal overflow (more than 34 digits)",
            DecFail::Exponent => "Decimal exponent out of range",
        }
    }
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

/// A `Decimal` value as the interpreter holds it: the coefficient keeps its scale (`1.50`
/// is 150 × 10^-2), and equality and order compare values.
#[derive(Clone, Debug)]
pub(crate) struct Dec {
    coef: Big,
    exp: i64,
}

impl Dec {
    /// `coef × 10^exp` as decimal128 holds it: trailing zeros leave a coefficient longer than
    /// 34 digits (anything else is [`DecFail::Digits`]), then the exponent is clamped into
    /// range by moving zeros into, or exactly out of, the coefficient (otherwise
    /// [`DecFail::Exponent`]). Zero takes any exponent, clamped.
    fn make(coef: Big, exp: i64) -> Result<Dec, DecFail> {
        let mut d = Dec { coef, exp };
        let n = d.coef.decimal_digits().len();
        if n > DEC_DIGITS {
            let extra = n - DEC_DIGITS;
            let (q, r) = d
                .coef
                .divmod_trunc(&Big::pow10(extra as u32))
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
            if pad > (DEC_DIGITS - d.coef.decimal_digits().len()) as i64 {
                return Err(DecFail::Exponent);
            }
            d.coef = d.coef.mul(&Big::pow10(pad as u32));
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
                .divmod_trunc(&Big::pow10(need as u32))
                .expect("10^k is not 0");
            if !r.is_zero() {
                return Err(DecFail::Exponent);
            }
            d.coef = q;
            d.exp = DEC_EXP_MIN;
        }
        Ok(d)
    }

    /// `make(coef × 10^zeros, exp)` (`zeros` >= 0, `coef` of at most 34 digits) without
    /// building zeros that `make` would drop again.
    fn make_padded(coef: Big, zeros: i128, exp: i128) -> Result<Dec, DecFail> {
        if coef.is_zero() {
            return Dec::make(coef, saturate(exp));
        }
        let room = (DEC_DIGITS - coef.decimal_digits().len().min(DEC_DIGITS)) as i128;
        let pad = zeros.clamp(0, room);
        Dec::make(
            coef.mul(&Big::pow10(pad as u32)),
            saturate(exp + zeros - pad),
        )
    }

    /// An integer literal typed `Decimal` (§5.6): exactly `v`, or why it cannot be held.
    pub(crate) fn from_u128(v: u128) -> Result<Dec, DecFail> {
        Dec::make(Big::from_u128(v), 0)
    }

    /// A literal typed `Decimal` (§5.6: the exact value written), with the interpreter's
    /// `Decimal.parse` rules: ASCII digits, an optional `.` and fraction, an optional
    /// exponent (an optional sign and digits), `_` separators allowed. `None` when the text
    /// is not one; otherwise the value clamped as decimal128 clamps, or why it cannot be
    /// held (K0126). Only the first 34 significant digits are ever built.
    pub(crate) fn parse(text: &str) -> Option<Result<Dec, DecFail>> {
        let text: String = text.chars().filter(|c| *c != '_').collect();
        let (neg, body) = match text.strip_prefix('-') {
            Some(r) => (true, r),
            None => (false, text.as_str()),
        };
        let (mantissa, exp) = match body.find(['e', 'E']) {
            Some(i) => (&body[..i], parse_exponent(&body[i + 1..])?),
            None => (body, 0),
        };
        let (int, frac) = mantissa.split_once('.').unwrap_or((mantissa, ""));
        let ok = |s: &str| s.chars().all(|c| c.is_ascii_digit());
        if int.is_empty() || !ok(int) || !ok(frac) || (mantissa.contains('.') && frac.is_empty()) {
            return None;
        }
        let mut exp = saturate(i128::from(exp) - frac.len() as i128);
        let all: Vec<u8> = int.bytes().chain(frac.bytes()).collect();
        let first = all.iter().position(|&b| b != b'0').unwrap_or(all.len());
        let mut digits = &all[first..];
        if digits.len() > DEC_DIGITS {
            // Only trailing zeros may go (into the exponent), as `make` drops them.
            if digits[DEC_DIGITS..].iter().any(|&b| b != b'0') {
                return Some(Err(DecFail::Digits));
            }
            exp = saturate(i128::from(exp) + (digits.len() - DEC_DIGITS) as i128);
            digits = &digits[..DEC_DIGITS];
        }
        let mut coef = Big::zero();
        for b in digits {
            coef = coef
                .mul_small(10)
                .add(&Big::from_u128(u128::from(b - b'0')));
        }
        if neg {
            coef = coef.negated();
        }
        Some(Dec::make(coef, exp))
    }

    /// `self.pow(k)` (FOUNDER-QUEUE Q45), as the interpreter computes it: exactly
    /// `coef^k × 10^(exp·k)`, held as `make` holds it. `None` for a negative exponent.
    pub(crate) fn pow(&self, k: i64) -> Option<Result<Dec, DecFail>> {
        if k < 0 {
            return None;
        }
        if k == 0 {
            return Some(Dec::from_u128(1));
        }
        if self.coef.is_zero() {
            let exp = saturate(i128::from(self.exp) * i128::from(k));
            return Some(Dec::make(Big::zero(), exp));
        }
        // coef = sig × 10^tz, where `sig` has no factor 10 (nor has any power of it).
        let (mut sig, mut tz) = (self.coef.clone(), 0i128);
        loop {
            let (q, r) = sig.divmod_trunc(&Big::from_u128(10)).expect("10 is not 0");
            if !r.is_zero() {
                break;
            }
            sig = q;
            tz += 1;
        }
        let one = Big::from_u128(1);
        let mut p = one.clone();
        if sig.clone().abs() == one {
            if sig.is_negative() && k % 2 == 1 {
                p = p.negated();
            }
        } else {
            // |sig| >= 2: at most 113 rounds before the power passes 34 digits.
            for _ in 0..k {
                p = p.mul(&sig);
                if p.decimal_digits().len() > DEC_DIGITS {
                    return Some(Err(DecFail::Digits));
                }
            }
        }
        let k = i128::from(k);
        Some(Dec::make_padded(p, tz * k, i128::from(self.exp) * k))
    }

    pub(crate) fn negated(&self) -> Dec {
        Dec {
            coef: self.coef.clone().negated(),
            exp: self.exp,
        }
    }

    /// `self + o` (`o` negated for `-`), or why the exact result cannot be held.
    fn add_signed(&self, o: &Dec) -> Result<Dec, DecFail> {
        let e = self.exp.min(o.exp);
        let gap = (self.exp - o.exp).abs();
        if gap > DEC_ALIGN_LIMIT {
            let (hi, lo) = if self.exp > o.exp {
                (self, o)
            } else {
                (o, self)
            };
            if hi.coef.is_zero() {
                return Dec::make(lo.coef.clone(), e);
            }
            if !lo.coef.is_zero() {
                return Err(DecFail::Digits);
            }
            // `hi` aligned down to `lo`'s exponent and reduced again: as many of the added
            // zeros as fit stay in the coefficient.
            let len = hi.coef.decimal_digits().len();
            let keep = (DEC_DIGITS - len.min(DEC_DIGITS)) as u32;
            let coef = hi.coef.mul(&Big::pow10(keep));
            return Dec::make(coef, hi.exp - i64::from(keep));
        }
        let a = self.coef.mul(&Big::pow10((self.exp - e) as u32));
        let b = o.coef.mul(&Big::pow10((o.exp - e) as u32));
        Dec::make(a.add(&b), e)
    }

    pub(crate) fn add(&self, o: &Dec) -> Result<Dec, DecFail> {
        self.add_signed(o)
    }

    pub(crate) fn sub(&self, o: &Dec) -> Result<Dec, DecFail> {
        self.add_signed(&o.negated())
    }

    pub(crate) fn mul(&self, o: &Dec) -> Result<Dec, DecFail> {
        Dec::make(self.coef.mul(&o.coef), self.exp.saturating_add(o.exp))
    }

    /// The order of the two values (`1.5 == 1.50`).
    pub(crate) fn cmp_value(&self, o: &Dec) -> Ordering {
        let sign = |c: &Big| match (c.is_zero(), c.is_negative()) {
            (true, _) => 0,
            (false, true) => -1,
            (false, false) => 1,
        };
        let (sa, sb) = (sign(&self.coef), sign(&o.coef));
        if sa != sb || sa == 0 {
            return sa.cmp(&sb);
        }
        let gap = self.exp - o.exp;
        let magnitude = if gap.abs() > DEC_ALIGN_LIMIT {
            // Both are non-zero and have at most 34 digits: the larger exponent wins.
            gap.cmp(&0)
        } else {
            let e = self.exp.min(o.exp);
            let a = self
                .coef
                .clone()
                .abs()
                .mul(&Big::pow10((self.exp - e) as u32));
            let b = o.coef.clone().abs().mul(&Big::pow10((o.exp - e) as u32));
            a.cmp(&b)
        };
        if sa > 0 {
            magnitude
        } else {
            magnitude.reverse()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dec(s: &str) -> Dec {
        match Dec::parse(s) {
            Some(Ok(d)) => d,
            other => panic!("{s} parses: {other:?}"),
        }
    }

    fn show(d: &Dec) -> String {
        let digits: String = d
            .coef
            .decimal_digits()
            .iter()
            .rev()
            .map(|x| char::from(b'0' + x))
            .collect();
        format!(
            "{}{digits}e{}",
            if d.coef.is_negative() { "-" } else { "" },
            d.exp
        )
    }

    #[test]
    fn bigint_division_is_python_floor_division() {
        let b = |v: i64| Big::from_i64(v);
        let fd = |x: i64, y: i64| {
            let (q, r) = b(x).divmod_floor(&b(y)).unwrap();
            (q, r)
        };
        assert_eq!(fd(-7, 2), (b(-4), b(1)));
        assert_eq!(fd(7, -2), (b(-4), b(-1)));
        assert_eq!(fd(-7, -2), (b(3), b(-1)));
        assert!(b(1).divmod_floor(&Big::zero()).is_none());
        let big = Big::from_u128(u128::MAX);
        let sq = big.mul(&big);
        assert_eq!(sq.divmod_floor(&big).unwrap(), (big.clone(), Big::zero()));
        assert!(sq.sub(&sq).is_zero());
        assert_eq!(sq.bits(), 256);
    }

    #[test]
    fn decimal_keeps_34_digits_and_strips_only_what_it_must() {
        // 34 nines with a `.0`: the trailing zero goes into the exponent.
        let nines = dec("9999999999999999999999999999999999.0");
        assert_eq!(show(&nines), "9999999999999999999999999999999999e0");
        assert_eq!(nines.mul(&nines).unwrap_err(), DecFail::Digits);
        let one = dec("1");
        assert_eq!(
            show(&nines.add(&one).unwrap()),
            "1000000000000000000000000000000000e1"
        );
        assert_eq!(nines.add(&dec("2")).unwrap_err(), DecFail::Digits);
        // Scale is kept where it fits: 1.50 + 1 = 2.50.
        assert_eq!(show(&dec("1.50").add(&one).unwrap()), "250e-2");
        assert_eq!(dec("1.50").cmp_value(&dec("1.5")), Ordering::Equal);
        assert_eq!(dec("-2").cmp_value(&dec("1")), Ordering::Less);
        assert_eq!(dec("-2e100").cmp_value(&dec("-1")), Ordering::Less);
        assert_eq!(dec("1e100").cmp_value(&dec("2")), Ordering::Greater);
        // 10^21 written with a `.0` is 10^22 × 10^-1; squared, 10^44 × 10^-2 has 45 digits,
        // and 11 of its trailing zeros move into the exponent.
        let e21 = dec("1000000000000000000000.0");
        assert_eq!(show(&e21), "10000000000000000000000e-1");
        assert_eq!(
            show(&e21.mul(&e21).unwrap()),
            "1000000000000000000000000000000000e9"
        );
        // The exponent range.
        assert_eq!(
            dec("1e6111").mul(&dec("1e6111")).unwrap_err(),
            DecFail::Exponent
        );
        assert_eq!(show(&dec("1e6111").mul(&dec("10")).unwrap()), "10e6111");
        assert_eq!(
            dec("1e-6176").mul(&dec("0.1")).unwrap_err(),
            DecFail::Exponent
        );
        assert_eq!(show(&dec("1e6112")), "10e6111");
        assert_eq!(show(&dec("0e9999")), "0e6111");
    }

    #[test]
    fn decimal_sums_across_a_large_exponent_gap_match_exact_alignment() {
        // Decided without aligning the operands: compare with the exact computation.
        let exact = |a: &Dec, b: &Dec| {
            let e = a.exp.min(b.exp);
            let x = a.coef.mul(&Big::pow10((a.exp - e) as u32));
            let y = b.coef.mul(&Big::pow10((b.exp - e) as u32));
            Dec::make(x.add(&y), e).map(|d| show(&d))
        };
        for (a, b) in [
            ("1e100", "0"),
            ("0", "1e100"),
            ("1e100", "5"),
            ("123e200", "0.000"),
            ("0e300", "7e-10"),
            ("-4e150", "0e-50"),
            ("1e6111", "1e-6176"),
            ("5e1000", "0e-1000"),
        ] {
            let (a, b) = (dec(a), dec(b));
            assert_eq!(a.add(&b).map(|d| show(&d)), exact(&a, &b), "{a:?} + {b:?}");
            assert_eq!(b.add(&a).map(|d| show(&d)), exact(&b, &a), "{b:?} + {a:?}");
        }
    }

    /// The interpreter's table (`kalam-interp` `num::tests::decimal128_clamping_vectors`):
    /// literals at the edges of decimal128 (Q48, Q52).
    #[test]
    fn decimal128_clamping_vectors() {
        let thousand_zeros = format!("1{}", "0".repeat(2000));
        let not_zeros = format!("1{}1", "0".repeat(2000));
        let ok = |coef: String, exp: i64| Some(Ok(format!("{coef}e{exp}")));
        let out = Some(Err(()));
        let cases: Vec<(&str, Option<Result<String, ()>>)> = vec![
            ("1e6111", ok("1".into(), 6111)),
            ("1e6112", ok("10".into(), 6111)),
            ("1e6144", ok(format!("1{}", "0".repeat(33)), 6111)),
            ("12e6143", ok(format!("12{}", "0".repeat(32)), 6111)),
            ("1e6145", out.clone()),
            ("123e6143", out.clone()),
            ("1e-6176", ok("1".into(), -6176)),
            ("1e-6177", out.clone()),
            ("10e-6177", ok("1".into(), -6176)),
            ("1.0e-6176", ok("1".into(), -6176)),
            ("0e99999999999999999999999", ok("0".into(), 6111)),
            ("0.000e-99999", ok("0".into(), -6176)),
            ("1e99999999999999999999999", out.clone()),
            ("1e-99999999999999999999999", out.clone()),
            (&thousand_zeros, ok(format!("1{}", "0".repeat(33)), 1967)),
            (&not_zeros, out.clone()),
            (
                "00000000000000000000000000000000000000001.5",
                ok("15".into(), -1),
            ),
            ("1.50", ok("150".into(), -2)),
            ("-2.5e+3", ok("-25".into(), 2)),
            ("1_000.5", ok("10005".into(), -1)),
            ("1e", None),
            ("1e+", None),
            ("1e+-5", None),
            ("1.", None),
            (".5", None),
        ];
        for (text, want) in cases {
            let got = Dec::parse(text).map(|r| r.map(|d| show(&d)).map_err(|_| ()));
            assert_eq!(got, want, "{text}");
        }
        assert_eq!(
            dec("1e6144").mul(&dec("10")).unwrap_err(),
            DecFail::Exponent
        );
        assert_eq!(
            Dec::parse("1e6145").unwrap().unwrap_err().message(),
            "Decimal exponent out of range"
        );
        assert_eq!(
            Dec::parse(&not_zeros).unwrap().unwrap_err().message(),
            "Decimal overflow (more than 34 digits)"
        );
        assert_eq!(Dec::from_u128(u128::MAX).unwrap_err(), DecFail::Digits);
        assert!(Dec::from_u128(10u128.pow(38)).is_ok());
    }

    #[test]
    fn decimal_pow_is_repeated_multiplication() {
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
            let x = dec(base);
            let mut acc: Result<Dec, DecFail> = Ok(dec("1"));
            for k in 0..=6 {
                let pow = x.pow(k).expect("k >= 0");
                assert_eq!(
                    pow.map(|d| show(&d)),
                    acc.clone().map(|d| show(&d)),
                    "{base}.pow({k})"
                );
                acc = acc.and_then(|a| a.mul(&x));
            }
        }
        assert_eq!(show(&dec("1.50").pow(2).unwrap().unwrap()), "22500e-4");
        assert_eq!(dec("2").pow(113).unwrap().unwrap_err(), DecFail::Digits);
        assert_eq!(dec("10").pow(6145).unwrap().unwrap_err(), DecFail::Exponent);
        assert_eq!(
            dec("0.1").pow(6177).unwrap().unwrap_err(),
            DecFail::Exponent
        );
        assert_eq!(show(&dec("-1").pow(i64::MAX).unwrap().unwrap()), "-1e0");
        assert!(dec("3").pow(-1).is_none());
    }

    #[test]
    fn bigint_pow_follows_the_interpreter() {
        let b = |v: i64| Big::from_i64(v);
        assert_eq!(b(-3).pow(3), BigPow::Value(b(-27)));
        assert_eq!(b(0).pow(0), BigPow::Value(b(1)));
        assert_eq!(b(-1).pow(i64::MAX), BigPow::Value(b(-1)));
        assert_eq!(b(2).pow(-1), BigPow::NegativeExponent);
        assert_eq!(b(2).pow(65535), BigPow::TooLarge);
        assert_eq!(b(2).pow(65534), BigPow::Unknown);
        // Evaluated while `bits × k` stays within `MAX_BIG_BITS`.
        let mut two_2000 = b(1);
        for _ in 0..2000 {
            two_2000 = two_2000.mul_small(2);
        }
        assert_eq!(b(2).pow(2000), BigPow::Value(two_2000));
        assert_eq!(b(2).pow(4000), BigPow::Unknown);
    }
}
