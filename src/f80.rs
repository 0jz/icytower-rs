// Software x87 extended-precision arithmetic. The original exe keeps intermediate results in
// 80-bit FPU registers (64-bit mantissa) and only rounds to double when storing, so exact replay
// playback needs the same double rounding. Only what the game logic uses: + - * /, compare, casts.
use std::cmp::Ordering;

/// Mantissa bits the FPU rounds to (x87 precision control: 64 = extended, 53 = double).
pub const PREC: u32 = 64;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct F80 {
    neg: bool,
    exp: i32,  // value = mant * 2^(exp - 63)
    mant: u64, // normalized (top bit set), 0 for zero
}

impl F80 {
    pub const ZERO: F80 = F80 { neg: false, exp: 0, mant: 0 };

    /// Round |m| * 2^e (m != 0) to PREC bits, nearest-even.
    fn round(neg: bool, e: i32, m: u128) -> F80 {
        if m == 0 {
            return F80::ZERO;
        }
        let lz = m.leading_zeros();
        let m = m << lz;
        let e = e - lz as i32; // value = m * 2^e, top bit at 127
        let drop = 128 - PREC;
        let keep = m >> drop;
        let rest = m & ((1u128 << drop) - 1);
        let half = 1u128 << (drop - 1);
        let mut keep = keep;
        if rest > half || (rest == half && keep & 1 == 1) {
            keep += 1;
        }
        let mut e = e + drop as i32; // value = keep * 2^e
        if keep >> PREC != 0 {
            keep >>= 1;
            e += 1;
        }
        let mant = (keep << (64 - PREC)) as u64;
        F80 { neg, exp: e - (64 - PREC) as i32 + 63, mant }
    }

    pub fn from_f64(d: f64) -> F80 {
        if d == 0.0 {
            return F80::ZERO;
        }
        let b = d.to_bits();
        let e = ((b >> 52) & 0x7ff) as i32 - 1023;
        let m = ((b & ((1 << 52) - 1)) | 1 << 52) << 11;
        F80 { neg: d < 0.0, exp: e, mant: m }
    }

    pub fn from_int(i: i64) -> F80 {
        F80::round(i < 0, 0, i.unsigned_abs() as u128)
    }

    /// fst/fstp to a double: round to 53 bits, nearest-even.
    pub fn to_f64(self) -> f64 {
        if self.mant == 0 {
            return 0.0;
        }
        let mut m = self.mant >> 11;
        let rest = self.mant & 0x7ff;
        if rest > 0x400 || (rest == 0x400 && m & 1 == 1) {
            m += 1;
        }
        let v = m as f64 * 2f64.powi(self.exp - 52);
        if self.neg { -v } else { v }
    }

    /// fist with the truncating rounding mode the game sets, i.e. a C (int) cast.
    pub fn trunc(self) -> i64 {
        if self.mant == 0 || self.exp < 0 {
            return 0;
        }
        let v = if self.exp >= 63 { self.mant as i128 } else { (self.mant >> (63 - self.exp)) as i128 };
        let v = if self.exp > 63 { v << (self.exp - 63).min(64) } else { v };
        let v = if self.neg { -v } else { v };
        v as i64
    }

    pub fn add(self, o: F80) -> F80 {
        if self.mant == 0 {
            return F80::round(o.neg, o.exp - 63, o.mant as u128);
        }
        if o.mant == 0 {
            return F80::round(self.neg, self.exp - 63, self.mant as u128);
        }
        let (a, b) = if self.exp >= o.exp { (self, o) } else { (o, self) };
        // 64 guard bits below the mantissa, the rest collapses into a sticky bit
        let ma = (a.mant as u128) << 62;
        let shift = (a.exp - b.exp) as u32;
        let mb = (b.mant as u128) << 62;
        let mb = if shift >= 126 {
            1
        } else {
            let lost = mb & ((1u128 << shift) - 1);
            (mb >> shift) | (lost != 0) as u128
        };
        let e = a.exp - 63 - 62;
        if a.neg == b.neg {
            F80::round(a.neg, e, ma + mb)
        } else if ma >= mb {
            F80::round(a.neg, e, ma - mb)
        } else {
            F80::round(b.neg, e, mb - ma)
        }
    }

    pub fn neg(self) -> F80 {
        F80 { neg: !self.neg && self.mant != 0, ..self }
    }

    pub fn sub(self, o: F80) -> F80 {
        self.add(o.neg())
    }

    pub fn mul(self, o: F80) -> F80 {
        if self.mant == 0 || o.mant == 0 {
            return F80::ZERO;
        }
        F80::round(self.neg != o.neg, self.exp + o.exp - 126, self.mant as u128 * o.mant as u128)
    }

    pub fn div(self, o: F80) -> F80 {
        if self.mant == 0 {
            return F80::ZERO;
        }
        assert!(o.mant != 0, "division by zero");
        let d = o.mant as u128;
        let n = (self.mant as u128) << 64;
        // 64-65 quotient bits, then two more as guard bits, then the remainder as a sticky bit
        let (q1, r1) = (n / d, n % d);
        let (q2, r2) = ((r1 << 2) / d, (r1 << 2) % d);
        let q = (q1 << 2) | q2;
        F80::round(self.neg != o.neg, self.exp - o.exp - 64 - 2 - 1, (q << 1) | (r2 != 0) as u128)
    }

    fn key(self) -> (i32, u64) {
        (self.exp, self.mant)
    }

    pub fn cmp(self, o: F80) -> Ordering {
        match (self.mant == 0, o.mant == 0) {
            (true, true) => return Ordering::Equal,
            (true, false) => return if o.neg { Ordering::Greater } else { Ordering::Less },
            (false, true) => return if self.neg { Ordering::Less } else { Ordering::Greater },
            _ => {}
        }
        match (self.neg, o.neg) {
            (false, true) => Ordering::Greater,
            (true, false) => Ordering::Less,
            (false, false) => self.key().cmp(&o.key()),
            (true, true) => o.key().cmp(&self.key()),
        }
    }

    pub fn gt(self, o: F80) -> bool {
        self.cmp(o) == Ordering::Greater
    }
}

pub fn f(d: f64) -> F80 {
    F80::from_f64(d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_f64_where_exact_and_rounds_like_x87() {
        for (a, b) in [(1.5, 2.25), (431.0, -0.8), (12.2, 0.3), (-7.0, 3.0)] {
            assert_eq!(f(a).add(f(b)).to_f64(), a + b);
            assert_eq!(f(a).mul(f(b)).to_f64(), a * b);
            assert_eq!(f(a).sub(f(b)).to_f64(), a - b);
        }
        assert_eq!(F80::from_int(270).div(F80::from_int(300)).mul(F80::from_int(10)).trunc(), 9);
        assert_eq!(f(-3.7).trunc(), -3);
        assert_eq!(f(65535.9).trunc(), 65535);
        assert!(f(0.1).gt(f(-0.2)) && f(-0.1).gt(f(-0.2)) && !f(0.0).gt(F80::ZERO));
        // 1/3 is inexact: extended keeps 64 bits, so storing it gives the correctly rounded double
        assert_eq!(F80::from_int(1).div(F80::from_int(3)).to_f64(), 1.0 / 3.0);
    }
}
