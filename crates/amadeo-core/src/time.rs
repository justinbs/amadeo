//! The simulation clock.
//!
//! Amadeo simulates at a fixed rate (ADR 0005, ADR 0007). Inside the deterministic zone there is
//! exactly one notion of time: the [`Tick`] counter and the constant [`FIXED_DT`]. Gameplay code
//! must never read a wall clock, a frame delta, or anything derived from real elapsed time —
//! doing so voids every replay test in the project.

/// Simulation ticks per second. Fixed by ADR 0007; not configurable.
///
/// Recorded replays embed this value and are rejected if it does not match, because a replay
/// played back at a different rate would silently produce different behaviour.
pub const TICK_RATE_HZ: u32 = 60;

/// Seconds of simulated time per tick. The only timestep gameplay code may use.
pub const FIXED_DT: f32 = 1.0 / TICK_RATE_HZ as f32;

/// Nanoseconds per tick, for the wall-clock accumulator in the main loop.
///
/// This truncates (1e9 / 60 is not an integer), and that is deliberately fine: the accumulator
/// lives *outside* the deterministic zone. Real elapsed time only decides *how many* ticks to run,
/// never what happens inside one. Simulation results therefore do not depend on this constant's
/// rounding, which is why a headless run and a windowed run produce identical state (invariant I7).
pub const FIXED_DT_NANOS: u64 = 1_000_000_000 / TICK_RATE_HZ as u64;

/// A simulation tick number.
///
/// Monotonic, starts at zero, and increments once per simulation step. This is the simulation's
/// only clock. Two runs that reach the same tick with the same inputs are in the same state — that
/// property is what makes replays, snapshots and golden tests possible.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Tick(pub u64);

impl Tick {
    /// The tick before any simulation has run.
    pub const ZERO: Tick = Tick(0);

    /// The next tick after this one.
    #[must_use]
    pub fn next(self) -> Tick {
        Tick(self.0 + 1)
    }

    /// Advances this tick in place. Called once per simulation step by the app loop.
    pub fn advance(&mut self) {
        self.0 += 1;
    }

    /// Simulated seconds elapsed since [`Tick::ZERO`].
    ///
    /// Derived from the tick count rather than measured, so it is identical across runs. Safe to
    /// use in gameplay code, unlike a real clock.
    #[must_use]
    pub fn elapsed_secs(self) -> f64 {
        self.0 as f64 / f64::from(TICK_RATE_HZ)
    }

    /// Whether this tick falls on an `every_n`-tick cadence.
    ///
    /// Useful for staggering periodic work (AI re-planning, expensive checks) without a timer.
    /// Returns `false` for `every_n == 0` rather than dividing by zero.
    #[must_use]
    pub fn is_multiple_of(self, every_n: u64) -> bool {
        every_n != 0 && self.0 % every_n == 0
    }
}

impl std::fmt::Display for Tick {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "tick {}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tick_advances_monotonically() {
        let mut t = Tick::ZERO;
        assert_eq!(t.0, 0);
        t.advance();
        assert_eq!(t, Tick(1));
        assert_eq!(t.next(), Tick(2));
        // next() must not mutate.
        assert_eq!(t, Tick(1));
    }

    #[test]
    fn elapsed_secs_matches_tick_rate() {
        assert_eq!(Tick(0).elapsed_secs(), 0.0);
        assert_eq!(Tick(60).elapsed_secs(), 1.0);
        assert_eq!(Tick(30).elapsed_secs(), 0.5);
    }

    #[test]
    fn fixed_dt_is_consistent_with_tick_rate() {
        // 60 ticks of FIXED_DT should add up to one second, within f32 tolerance.
        let total: f32 = (0..TICK_RATE_HZ).map(|_| FIXED_DT).sum();
        assert!((total - 1.0).abs() < 1e-5, "60 ticks summed to {total}");
    }

    #[test]
    fn multiple_of_zero_does_not_panic() {
        assert!(!Tick(10).is_multiple_of(0));
        assert!(Tick(10).is_multiple_of(5));
        assert!(!Tick(10).is_multiple_of(3));
    }
}
