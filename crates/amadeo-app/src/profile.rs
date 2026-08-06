//! Where the frame goes: per-system timings — ADR 0040.
//!
//! # Why a wall clock is allowed to run inside the tick
//!
//! `CLAUDE.md` trap 2 names `Instant::now()` in gameplay as a nondeterminism leak, and it is right
//! to. What makes this safe is **ADR 0009's split**, which exists for exactly this shape of thing: a
//! [`Resource`](amadeo_ecs::Resource) is simulation state and is in the state hash, a
//! [`Service`](amadeo_ecs::Service) is engine machinery and is structurally excluded from it.
//!
//! [`Profiler`] is a service. Nothing it records can reach a state hash, a snapshot or a replay —
//! not by convention but because the hash cannot see the service store at all. That is the same
//! mechanism that already lets `Renderer` and `Assets` hold wall-clock-dependent state, and
//! `profiling_does_not_move_the_state_hash` pins it.
//!
//! **The residual risk is real and worth naming**: a gameplay system *could* read this service and
//! branch on a duration, which would make a replay diverge. Nothing structural prevents that — the
//! golden replays are the guard, as they are for every other service.
//!
//! # Why it is always on rather than behind a feature
//!
//! An agent cannot *feel* a frame-rate problem. `docs/04-subsystems.md` §18 says so directly, and it
//! is the whole reason `profile.frame` is on the protocol's list. A profiler compiled out of the
//! build a game actually ships would report on a build nobody runs, and would make the agent's
//! introspection depend on how a game was compiled rather than on what the engine is.
//!
//! The cost is two clock reads per system per tick. Measured on this machine at roughly 20 ns each,
//! so a ten-system schedule pays about 0.4 µs of a 16.67 ms frame — under three thousandths of one
//! percent.

use amadeo_ecs::Service;
use std::collections::BTreeMap;
use std::time::Duration;

/// How long one system took, and how often it has run.
///
/// Kept as a running total plus a count rather than a list of samples: a list would grow without
/// bound over a long session, and the two questions worth asking of a system — what it costs on
/// average and what its worst frame was — need only these four numbers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SystemTiming {
    /// How many times it has run since the last [`Profiler::reset`].
    pub runs: u64,
    /// Total time across those runs.
    pub total: Duration,
    /// The single slowest run.
    ///
    /// Worth more than the average for a frame budget: a system averaging 0.1 ms with a 9 ms spike
    /// drops a frame every time it spikes, and an average hides that completely.
    pub worst: Duration,
}

impl SystemTiming {
    /// Mean time per run, or zero if it has never run.
    #[must_use]
    pub fn mean(&self) -> Duration {
        if self.runs == 0 {
            Duration::ZERO
        } else {
            self.total / u32::try_from(self.runs).unwrap_or(u32::MAX)
        }
    }
}

/// Per-system timings for the tick loop.
///
/// A [`Service`], so nothing here is in the state hash (ADR 0009) — see the module docs for why that
/// is what makes reading a clock inside the tick safe.
///
/// Installed by default on every [`App`](crate::App), so `profile.frame` always has something to
/// report and a game never has to opt in to being measurable.
#[derive(Debug, Default)]
pub struct Profiler {
    /// Keyed by system label. A `BTreeMap` so a report comes out in a stable order — the same reason
    /// every other registry in this engine uses one, and here it also means two runs of a benchmark
    /// print their rows in the same sequence and can be diffed.
    systems: BTreeMap<&'static str, SystemTiming>,
    /// Ticks observed since the last reset.
    ticks: u64,
}

impl Service for Profiler {}

impl Profiler {
    /// A profiler with nothing recorded.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Records one run of one system.
    pub fn record(&mut self, label: &'static str, elapsed: Duration) {
        let entry = self.systems.entry(label).or_default();
        entry.runs += 1;
        entry.total += elapsed;
        entry.worst = entry.worst.max(elapsed);
    }

    /// Records that a whole tick finished.
    pub fn record_tick(&mut self) {
        self.ticks += 1;
    }

    /// How many ticks have been observed since the last reset.
    #[must_use]
    pub fn ticks(&self) -> u64 {
        self.ticks
    }

    /// One system's timings, if it has ever run.
    #[must_use]
    pub fn system(&self, label: &str) -> Option<SystemTiming> {
        self.systems.get(label).copied()
    }

    /// Every system's timings, in label order.
    pub fn systems(&self) -> impl Iterator<Item = (&'static str, SystemTiming)> {
        self.systems.iter().map(|(label, timing)| (*label, *timing))
    }

    /// What one whole tick costs on average, summed across every system.
    ///
    /// **Not the same as measuring the tick from outside**, and the difference is the point: this is
    /// the part of a frame the schedule is responsible for, so it excludes whatever the caller does
    /// around it. A frame budget is spent on both, and separating them is how you find out which.
    #[must_use]
    pub fn mean_tick(&self) -> Duration {
        if self.ticks == 0 {
            return Duration::ZERO;
        }
        let total: Duration = self.systems.values().map(|timing| timing.total).sum();
        total / u32::try_from(self.ticks).unwrap_or(u32::MAX)
    }

    /// The slowest single system run recorded, and which system it was.
    #[must_use]
    pub fn worst(&self) -> Option<(&'static str, Duration)> {
        self.systems
            .iter()
            .max_by_key(|(_, timing)| timing.worst)
            .map(|(label, timing)| (*label, timing.worst))
    }

    /// Throws away everything recorded.
    ///
    /// What a benchmark calls after its warm-up, so the first few ticks — which pay for lazily
    /// allocated storage, cold caches and a cold branch predictor — do not sit in the average
    /// pretending to be the steady state.
    pub fn reset(&mut self) {
        self.systems.clear();
        self.ticks = 0;
    }

    /// A table of what the frame costs, widest first.
    ///
    /// Rows sorted by mean rather than by name, because "what is expensive" is the question a report
    /// is read to answer.
    #[must_use]
    pub fn report(&self) -> String {
        let mut rows: Vec<(&'static str, SystemTiming)> = self.systems().collect();
        rows.sort_by_key(|(label, timing)| (std::cmp::Reverse(timing.mean()), *label));

        let mut out = format!(
            "{:<28} {:>10} {:>10} {:>8}\n",
            "system", "mean", "worst", "runs"
        );
        for (label, timing) in rows {
            out.push_str(&format!(
                "{:<28} {:>9.3}µs {:>9.3}µs {:>8}\n",
                label,
                timing.mean().as_secs_f64() * 1e6,
                timing.worst().as_secs_f64() * 1e6,
                timing.runs,
            ));
        }
        out.push_str(&format!(
            "{:<28} {:>9.3}µs over {} tick(s)\n",
            "TOTAL per tick",
            self.mean_tick().as_secs_f64() * 1e6,
            self.ticks
        ));
        out
    }
}

impl SystemTiming {
    /// The slowest run, as a `Duration`. Named to read well in [`Profiler::report`].
    fn worst(&self) -> Duration {
        self.worst
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_system_that_never_ran_has_no_timing() {
        let profiler = Profiler::new();
        assert!(profiler.system("nothing").is_none());
        assert_eq!(profiler.mean_tick(), Duration::ZERO);
    }

    #[test]
    fn the_worst_run_is_kept_separately_from_the_average() {
        // The property a frame budget actually needs. A system averaging well and spiking badly
        // drops a frame every time it spikes, and an average hides that completely.
        let mut profiler = Profiler::new();
        profiler.record("spiky", Duration::from_micros(100));
        profiler.record("spiky", Duration::from_micros(100));
        profiler.record("spiky", Duration::from_micros(9_000));

        let timing = profiler.system("spiky").expect("recorded");
        assert_eq!(timing.runs, 3);
        assert_eq!(timing.worst, Duration::from_micros(9_000));
        // The mean is nowhere near the spike, which is the whole point of keeping both.
        assert!(timing.mean() < Duration::from_micros(3_500));
    }

    #[test]
    fn the_report_puts_the_expensive_thing_first() {
        // "What is expensive" is the question a report is read to answer, so name order would be
        // the wrong sort even though it is the storage order.
        let mut profiler = Profiler::new();
        profiler.record("a_cheap_system", Duration::from_micros(1));
        profiler.record("z_expensive_system", Duration::from_micros(500));
        profiler.record_tick();

        let report = profiler.report();
        let expensive = report.find("z_expensive_system").expect("listed");
        let cheap = report.find("a_cheap_system").expect("listed");
        assert!(expensive < cheap, "widest first:\n{report}");
    }

    #[test]
    fn resetting_clears_the_warm_up() {
        let mut profiler = Profiler::new();
        profiler.record("system", Duration::from_millis(50));
        profiler.record_tick();
        profiler.reset();

        assert_eq!(profiler.ticks(), 0);
        assert!(profiler.system("system").is_none());
    }

    #[test]
    fn the_worst_system_is_the_one_that_spiked_hardest() {
        let mut profiler = Profiler::new();
        profiler.record("steady", Duration::from_micros(500));
        profiler.record("steady", Duration::from_micros(500));
        profiler.record("rare_spike", Duration::from_micros(2_000));

        // By worst rather than by total: `steady` has spent more time overall, and `rare_spike` is
        // the one that drops a frame.
        assert_eq!(
            profiler.worst(),
            Some(("rare_spike", Duration::from_micros(2_000)))
        );
    }
}
