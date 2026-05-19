// ABOUTME: BigDecimal numeric helpers (integer detection, exact pow).
// ABOUTME: Keeps exactness; non-integer powers stay symbolic upstream.

use bigdecimal::{BigDecimal, Context, One, RoundingMode, Zero};
use std::num::NonZeroU64;

const DIVISION_PRECISION: u64 = 34;

/// Returns the value as an `i64` exponent iff it is an exact integer that
/// fits. Used to decide when `^` can be folded exactly.
pub fn as_integer_exponent(n: &BigDecimal) -> Option<i64> {
    let normalized = n.normalized();
    if normalized.fractional_digit_count() > 0 {
        return None;
    }
    use std::str::FromStr;
    i64::from_str(&normalized.with_scale(0).to_string()).ok()
}

/// Exact integer power. Negative exponents produce a reciprocal.
pub fn pow_int(base: &BigDecimal, exp: i64) -> BigDecimal {
    if exp == 0 {
        return BigDecimal::one();
    }
    let mut acc = BigDecimal::one();
    let mut b = base.clone();
    let mut e = exp.unsigned_abs();
    // Exponentiation by squaring.
    while e > 0 {
        if e & 1 == 1 {
            acc *= &b;
        }
        e >>= 1;
        if e > 0 {
            b = &b * &b;
        }
    }
    if exp < 0 {
        if acc.is_zero() {
            // Caller must guard; defensively return zero.
            return BigDecimal::zero();
        }
        bounded_div(&BigDecimal::one(), &acc)
    } else {
        acc.normalized()
    }
}

/// Decimal division with an explicit precision cap.
pub fn bounded_div(a: &BigDecimal, b: &BigDecimal) -> BigDecimal {
    let ctx = Context::new(
        NonZeroU64::new(DIVISION_PRECISION).expect("precision is non-zero"),
        RoundingMode::HalfEven,
    );
    ctx.multiply(a, &ctx.invert(b)).normalized()
}
