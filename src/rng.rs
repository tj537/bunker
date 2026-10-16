//! Small, fast random numbers (PCG32). Seedable, so tests and WAV exports are
//! repeatable; seeded from the clock for normal use.

#[derive(Clone, Debug)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        let mut r = Rng(seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407));
        r.next_u32();
        r
    }

    pub fn from_clock() -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x5eed);
        Self::new(nanos)
    }

    pub fn next_u32(&mut self) -> u32 {
        let old = self.0;
        self.0 = old.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        let xorshifted = (((old >> 18) ^ old) >> 27) as u32;
        let rot = (old >> 59) as u32;
        xorshifted.rotate_right(rot)
    }

    /// Uniform in [0, 1)
    pub fn f32(&mut self) -> f32 {
        (self.next_u32() >> 8) as f32 / (1u32 << 24) as f32
    }

    /// Uniform in [-1, 1) – white noise
    pub fn noise(&mut self) -> f32 {
        self.f32() * 2.0 - 1.0
    }

    /// Uniform integer in [0, n)
    pub fn below(&mut self, n: u32) -> u32 {
        if n == 0 {
            return 0;
        }
        ((self.next_u32() as u64 * n as u64) >> 32) as u32
    }

    pub fn range(&mut self, lo: i32, hi: i32) -> i32 {
        lo + self.below((hi - lo + 1).max(1) as u32) as i32
    }

    pub fn chance(&mut self, p: f32) -> bool {
        self.f32() < p
    }

    pub fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len() as u32) as usize]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeatable_and_in_range() {
        let (mut a, mut b) = (Rng::new(7), Rng::new(7));
        for _ in 0..1000 {
            assert_eq!(a.next_u32(), b.next_u32());
            let f = a.f32();
            assert!((0.0..1.0).contains(&f));
            assert!(a.below(5) < 5);
            let r = a.range(-3, 3);
            assert!((-3..=3).contains(&r));
            b.f32();
            b.below(5);
            b.range(-3, 3);
        }
    }

    #[test]
    fn roughly_uniform() {
        let mut r = Rng::new(1);
        let mut counts = [0u32; 4];
        for _ in 0..40_000 {
            counts[r.below(4) as usize] += 1;
        }
        for c in counts {
            assert!((9_000..11_000).contains(&c), "{counts:?}");
        }
    }
}
