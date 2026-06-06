//! A tiny, self-contained pseudo-random generator.
//!
//! We deliberately avoid an external RNG dependency so that the whole M1 build
//! is offline and, more importantly, so that scenarios are **exactly
//! reproducible** from a seed across machines and compiler versions. This is
//! PCG-XSH-RR (O'Neill 2014), a well-regarded 64→32 generator, plus a
//! Box–Muller transform for Gaussian draws.

/// A seedable PCG32 generator.
#[derive(Debug, Clone)]
pub struct Pcg32 {
    state: u64,
    inc: u64,
    // Cache for the second Box–Muller normal sample.
    spare_normal: Option<f64>,
}

impl Pcg32 {
    const MULTIPLIER: u64 = 6_364_136_223_846_793_005;

    /// Create a generator from a seed and a stream-selecting sequence number.
    pub fn new(seed: u64, seq: u64) -> Self {
        let mut rng = Pcg32 {
            state: 0,
            inc: (seq << 1) | 1,
            spare_normal: None,
        };
        // Standard PCG seeding ritual.
        let _ = rng.next_u32();
        rng.state = rng.state.wrapping_add(seed);
        let _ = rng.next_u32();
        rng
    }

    /// Convenience constructor with a fixed default stream.
    pub fn from_seed(seed: u64) -> Self {
        Self::new(seed, 0xda3e_39cb_94b9_5bdb)
    }

    /// Next raw 32-bit value.
    pub fn next_u32(&mut self) -> u32 {
        let old = self.state;
        self.state = old.wrapping_mul(Self::MULTIPLIER).wrapping_add(self.inc);
        // XSH-RR output permutation.
        let xorshifted = (((old >> 18) ^ old) >> 27) as u32;
        let rot = (old >> 59) as u32;
        xorshifted.rotate_right(rot)
    }

    /// Uniform float in the half-open interval [0, 1).
    pub fn next_f64(&mut self) -> f64 {
        // 53 bits of mantissa precision from two 32-bit draws.
        let hi = self.next_u32() as u64;
        let lo = self.next_u32() as u64;
        let bits = (hi << 21) ^ (lo >> 11);
        let mantissa = bits & ((1u64 << 53) - 1);
        mantissa as f64 / (1u64 << 53) as f64
    }

    /// A Bernoulli trial: `true` with probability `p`.
    pub fn bernoulli(&mut self, p: f64) -> bool {
        self.next_f64() < p
    }

    /// A standard normal draw (mean 0, variance 1) via Box–Muller.
    pub fn next_standard_normal(&mut self) -> f64 {
        if let Some(z) = self.spare_normal.take() {
            return z;
        }
        // Avoid log(0).
        let u1 = (self.next_f64()).max(f64::MIN_POSITIVE);
        let u2 = self.next_f64();
        let r = (-2.0 * u1.ln()).sqrt();
        let theta = std::f64::consts::TAU * u2;
        self.spare_normal = Some(r * theta.sin());
        r * theta.cos()
    }

    /// A normal draw with the given mean and standard deviation.
    pub fn next_normal(&mut self, mean: f64, std_dev: f64) -> f64 {
        mean + std_dev * self.next_standard_normal()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reproducible_from_seed() {
        let mut a = Pcg32::from_seed(42);
        let mut b = Pcg32::from_seed(42);
        for _ in 0..1000 {
            assert_eq!(a.next_u32(), b.next_u32());
        }
    }

    #[test]
    fn uniform_in_range() {
        let mut rng = Pcg32::from_seed(7);
        for _ in 0..100_000 {
            let x = rng.next_f64();
            assert!((0.0..1.0).contains(&x));
        }
    }

    #[test]
    fn uniform_mean_is_about_half() {
        let mut rng = Pcg32::from_seed(123);
        let n = 200_000;
        let sum: f64 = (0..n).map(|_| rng.next_f64()).sum();
        let mean = sum / n as f64;
        assert!((mean - 0.5).abs() < 0.01, "mean was {mean}");
    }

    #[test]
    fn normal_statistics_are_sane() {
        let mut rng = Pcg32::from_seed(99);
        let n = 200_000;
        let mut sum = 0.0;
        let mut sum_sq = 0.0;
        for _ in 0..n {
            let z = rng.next_standard_normal();
            sum += z;
            sum_sq += z * z;
        }
        let mean = sum / n as f64;
        let var = sum_sq / n as f64 - mean * mean;
        assert!(mean.abs() < 0.02, "mean was {mean}");
        assert!((var - 1.0).abs() < 0.05, "variance was {var}");
    }

    #[test]
    fn bernoulli_frequency() {
        let mut rng = Pcg32::from_seed(2024);
        let n = 200_000;
        let hits = (0..n).filter(|_| rng.bernoulli(0.3)).count();
        let freq = hits as f64 / n as f64;
        assert!((freq - 0.3).abs() < 0.01, "freq was {freq}");
    }
}
