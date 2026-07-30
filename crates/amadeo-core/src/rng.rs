//! Deterministic pseudo-random number generation.
//!
//! # Why this is hand-written rather than a dependency
//!
//! Determinism is invariant I3, and golden replay tests assert on exact state hashes. If the
//! random number generator came from a third-party crate, a patch-level update that changed its
//! internals would look exactly like a behavioural regression in our engine — every replay test
//! would fail at once, for a reason that has nothing to do with our code.
//!
//! So the generator lives here, pinned, auditable, and roughly thirty lines long.
//!
//! The algorithm is PCG-XSH-RR (32-bit output, 64-bit state), chosen because it is small, has
//! well-documented statistical quality, and is trivially reproducible from its specification.
//!
//! # Streams, not a global
//!
//! Never share one generator across systems. System execution order would then leak into the
//! values each system receives, which is a nondeterminism source that only appears once you
//! reorder systems. Use [`Rng::fork`] to derive an independent stream per system or per entity.

/// A deterministic random number generator.
///
/// Same seed plus same call sequence always produces the same values, on every machine and every
/// build. See the module docs for why this matters and how to scope one correctly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rng {
    /// The generator's evolving state.
    state: u64,
    /// The stream selector. Two generators with different increments produce different sequences
    /// even from the same seed, which is what makes [`Rng::fork`] work.
    increment: u64,
}

/// The LCG multiplier specified by PCG for a 64-bit state.
const PCG_MULTIPLIER: u64 = 6_364_136_223_846_793_005;

impl Rng {
    /// Creates a generator from a seed.
    #[must_use]
    pub fn new(seed: u64) -> Self {
        Self::with_stream(seed, 0)
    }

    /// Creates a generator on an explicit stream.
    ///
    /// Two generators with the same seed but different `stream` values produce unrelated
    /// sequences. Prefer [`Rng::fork`] over calling this directly.
    #[must_use]
    pub fn with_stream(seed: u64, stream: u64) -> Self {
        // The increment must be odd for the LCG to reach full period, hence the `| 1`.
        let increment = (stream << 1) | 1;
        let mut rng = Self {
            state: 0,
            increment,
        };
        // Two steps to mix the seed thoroughly into the state; a bare `state = seed` leaves the
        // first few outputs correlated with the seed.
        rng.step();
        rng.state = rng.state.wrapping_add(seed);
        rng.step();
        rng
    }

    /// Derives an independent generator from this one.
    ///
    /// Use this to give each system or entity its own stream, so that system ordering cannot
    /// affect the values any of them sees.
    ///
    /// ```
    /// use amadeo_core::Rng;
    /// let mut parent = Rng::new(42);
    /// let mut a = parent.fork();
    /// let mut b = parent.fork();
    /// // Independent streams: overwhelmingly unlikely to agree.
    /// assert_ne!(a.next_u32(), b.next_u32());
    /// ```
    #[must_use]
    pub fn fork(&mut self) -> Rng {
        let seed = self.next_u64();
        let stream = self.next_u64();
        Rng::with_stream(seed, stream)
    }

    /// Advances the internal state by one LCG step.
    fn step(&mut self) {
        self.state = self
            .state
            .wrapping_mul(PCG_MULTIPLIER)
            .wrapping_add(self.increment);
    }

    /// Returns the next `u32`, uniformly distributed over the whole range.
    pub fn next_u32(&mut self) -> u32 {
        let old_state = self.state;
        self.step();

        // PCG's XSH-RR output function: xorshift the high bits down, then rotate by an amount
        // taken from the top bits. The rotation is what defeats the lattice structure that plain
        // LCGs suffer from.
        let xorshifted = (((old_state >> 18) ^ old_state) >> 27) as u32;
        let rotation = (old_state >> 59) as u32;
        xorshifted.rotate_right(rotation)
    }

    /// Returns the next `u64`, built from two 32-bit outputs.
    pub fn next_u64(&mut self) -> u64 {
        let high = u64::from(self.next_u32());
        let low = u64::from(self.next_u32());
        (high << 32) | low
    }

    /// Returns an `f32` in `[0.0, 1.0)`.
    ///
    /// Uses the top 24 bits, which is exactly the mantissa width of `f32`, so every value is
    /// representable and the distribution has no gaps or duplicates.
    pub fn next_f32(&mut self) -> f32 {
        const MANTISSA_BITS: u32 = 24;
        let bits = self.next_u32() >> (32 - MANTISSA_BITS);
        bits as f32 / (1u32 << MANTISSA_BITS) as f32
    }

    /// Returns an `f32` in `[min, max)`.
    ///
    /// If `min >= max`, returns `min` rather than producing a nonsensical range.
    pub fn range_f32(&mut self, min: f32, max: f32) -> f32 {
        if min >= max {
            return min;
        }
        min + self.next_f32() * (max - min)
    }

    /// Returns a `u32` in `[0, bound)` with no modulo bias.
    ///
    /// Returns 0 if `bound` is 0. The rejection loop is what removes the bias a plain `% bound`
    /// would introduce; it retries rarely and terminates with probability 1.
    pub fn below_u32(&mut self, bound: u32) -> u32 {
        if bound == 0 {
            return 0;
        }
        // Values at or above this threshold would map unevenly onto [0, bound), so discard them.
        let threshold = bound.wrapping_neg() % bound;
        loop {
            let value = self.next_u32();
            if value >= threshold {
                return value % bound;
            }
        }
    }

    /// Returns `true` with the given probability, clamped to `[0.0, 1.0]`.
    pub fn chance(&mut self, probability: f32) -> bool {
        if probability <= 0.0 {
            return false;
        }
        if probability >= 1.0 {
            return true;
        }
        self.next_f32() < probability
    }

    /// Picks an index into a collection of `len` items, or `None` if empty.
    ///
    /// Returns an index rather than a reference so it works for any collection type.
    pub fn pick_index(&mut self, len: usize) -> Option<usize> {
        if len == 0 {
            return None;
        }
        // u32 is sufficient: a collection with more than 4 billion elements is not a case this
        // engine needs to serve, and silently truncating would be worse than being explicit.
        let bounded = u32::try_from(len).unwrap_or(u32::MAX);
        Some(self.below_u32(bounded) as usize)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_gives_same_sequence() {
        let mut a = Rng::new(12345);
        let mut b = Rng::new(12345);
        for _ in 0..1000 {
            assert_eq!(a.next_u32(), b.next_u32());
        }
    }

    #[test]
    fn different_seeds_diverge() {
        let mut a = Rng::new(1);
        let mut b = Rng::new(2);
        let a_values: Vec<u32> = (0..16).map(|_| a.next_u32()).collect();
        let b_values: Vec<u32> = (0..16).map(|_| b.next_u32()).collect();
        assert_ne!(a_values, b_values);
    }

    #[test]
    fn forked_streams_are_independent() {
        let mut parent = Rng::new(999);
        let mut a = parent.fork();
        let mut b = parent.fork();
        let a_values: Vec<u32> = (0..16).map(|_| a.next_u32()).collect();
        let b_values: Vec<u32> = (0..16).map(|_| b.next_u32()).collect();
        assert_ne!(a_values, b_values);
    }

    #[test]
    fn fork_is_itself_deterministic() {
        // Forking must reproduce across runs, or per-system streams would vary between runs.
        let child_a = Rng::new(7).fork();
        let child_b = Rng::new(7).fork();
        assert_eq!(child_a, child_b);
    }

    #[test]
    fn f32_output_stays_in_unit_range() {
        let mut rng = Rng::new(4);
        for _ in 0..10_000 {
            let v = rng.next_f32();
            assert!((0.0..1.0).contains(&v), "{v} out of range");
        }
    }

    #[test]
    fn range_f32_respects_bounds_and_degenerate_input() {
        let mut rng = Rng::new(5);
        for _ in 0..1000 {
            let v = rng.range_f32(-2.0, 3.0);
            assert!((-2.0..3.0).contains(&v), "{v} out of range");
        }
        // Inverted and empty ranges return min rather than misbehaving.
        assert_eq!(rng.range_f32(1.0, 1.0), 1.0);
        assert_eq!(rng.range_f32(5.0, 2.0), 5.0);
    }

    #[test]
    fn below_u32_respects_bound() {
        let mut rng = Rng::new(6);
        for _ in 0..10_000 {
            assert!(rng.below_u32(10) < 10);
        }
        assert_eq!(rng.below_u32(0), 0);
        assert_eq!(rng.below_u32(1), 0);
    }

    #[test]
    fn below_u32_is_reasonably_uniform() {
        // Not a rigorous statistical test -- just enough to catch a badly broken implementation.
        let mut rng = Rng::new(8);
        let mut buckets = [0u32; 10];
        for _ in 0..100_000 {
            buckets[rng.below_u32(10) as usize] += 1;
        }
        for (i, &count) in buckets.iter().enumerate() {
            assert!(
                (8_000..12_000).contains(&count),
                "bucket {i} had {count} hits, expected ~10000"
            );
        }
    }

    #[test]
    fn chance_handles_certainties() {
        let mut rng = Rng::new(9);
        assert!(!rng.chance(0.0));
        assert!(rng.chance(1.0));
        assert!(!rng.chance(-1.0));
        assert!(rng.chance(2.0));
    }

    #[test]
    fn pick_index_stays_in_bounds() {
        let mut rng = Rng::new(10);
        assert_eq!(rng.pick_index(0), None);
        for _ in 0..1000 {
            let i = rng.pick_index(5).expect("non-empty");
            assert!(i < 5);
        }
    }
}
