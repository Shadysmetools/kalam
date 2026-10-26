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

/// A `Decimal` value as the interpreter holds it: the coefficient keeps its scale (`1.50`
/// is 150 × 10^-2), and equality and order compare values.
#[derive(Clone, Debug)]
pub(crate) struct Dec {
    coef: Big,
    exp: i64,
}

impl Dec {
    /// `None` when the value needs more than 34 digits or leaves the exponent range.
    fn make(coef: Big, exp: i64) -> Option<Dec> {
        let digits = coef.decimal_digits();
        let mut d = Dec { coef, exp };
        if digits.len() > DEC_DIGITS {
            // Exact reductions only: drop just as many trailing zeros as the length needs.
            let need = digits.len() - DEC_DIGITS;
            if digits.iter().take(need).any(|&x| x != 0) {
                return None;
            }
            let (q, _) = d.coef.divmod_trunc(&Big::pow10(need as u32))?;
            d.coef = q;
            d.exp = d.exp.checked_add(need as i64)?;
        }
        if d.coef.is_zero() {
            d.exp = d.exp.clamp(DEC_EXP_MIN, DEC_EXP_MAX);
        }
        if !(DEC_EXP_MIN..=DEC_EXP_MAX).contains(&d.exp) {
            return None;
        }
        Some(d)
    }

    /// An integer literal typed `Decimal` (§5.6): exactly `v`, or `None` when it cannot be
    /// held.
    pub(crate) fn from_u128(v: u128) -> Option<Dec> {
        Dec::make(Big::from_u128(v), 0)
    }

    /// A float literal typed `Decimal` (§5.6: the exact value written): ASCII digits, an
    /// optional `.` and fraction, an optional exponent, `_` separators allowed. `None` when
    /// the text is not one or the value cannot be held.
    pub(crate) fn parse(text: &str) -> Option<Dec> {
        let text: String = text.chars().filter(|c| *c != '_').collect();
        let (neg, body) = match text.strip_prefix('-') {
            Some(r) => (true, r),
            None => (false, text.as_str()),
        };
        let (mantissa, exp) = match body.find(['e', 'E']) {
            Some(i) => (&body[..i], body[i + 1..].parse::<i64>().ok()?),
            None => (body, 0),
        };
        let (int, frac) = mantissa.split_once('.').unwrap_or((mantissa, ""));
        let ok = |s: &str| s.chars().all(|c| c.is_ascii_digit());
        if int.is_empty() || !ok(int) || !ok(frac) || (mantissa.contains('.') && frac.is_empty()) {
            return None;
        }
        let mut coef = Big::zero();
        for c in int.chars().chain(frac.chars()) {
            coef = coef
                .mul_small(10)
                .add(&Big::from_u128(u128::from(c as u32 - '0' as u32)));
            if coef.bits() > MAX_BIG_BITS {
                return None;
            }
        }
        if neg {
            coef = coef.negated();
        }
        let exp = exp.checked_sub(i64::try_from(frac.len()).ok()?)?;
        Dec::make(coef, exp)
    }

    pub(crate) fn negated(&self) -> Dec {
        Dec {
            coef: self.coef.clone().negated(),
            exp: self.exp,
        }
    }

    /// `self + o` (`o` negated for `-`), or `None` when the exact result cannot be held.
    fn add_signed(&self, o: &Dec) -> Option<Dec> {
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
                return None;
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

    pub(crate) fn add(&self, o: &Dec) -> Option<Dec> {
        self.add_signed(o)
    }

    pub(crate) fn sub(&self, o: &Dec) -> Option<Dec> {
        self.add_signed(&o.negated())
    }

    pub(crate) fn mul(&self, o: &Dec) -> Option<Dec> {
        Dec::make(self.coef.mul(&o.coef), self.exp.checked_add(o.exp)?)
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
        Dec::parse(s).unwrap_or_else(|| panic!("{s} parses"))
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
        assert!(nines.mul(&nines).is_none());
        let one = dec("1");
        assert_eq!(
            show(&nines.add(&one).unwrap()),
            "1000000000000000000000000000000000e1"
        );
        assert!(nines.add(&dec("2")).is_none());
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
        assert!(dec("1e6111").mul(&dec("1e6111")).is_none());
        assert_eq!(show(&dec("1e6111").mul(&dec("10")).unwrap()), "10e6111");
        assert!(dec("1e-6176").mul(&dec("0.1")).is_none());
        assert!(Dec::parse("1e6112").is_none());
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
}
