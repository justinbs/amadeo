//! Q1 candidate A: gameplay compiled straight into the binary.
//!
//! No reload mechanism at all. A change to [`ai`] means recompile, relink, restart the process, and
//! re-simulate from tick zero to wherever you were looking. This measures that whole cost, because
//! that whole cost is what an agent actually pays per iteration.
//!
//! ```text
//! cargo run -p a-rust -- [ticks]
//! ```

mod ai;

use std::time::Instant;

fn main() {
    let ticks: u64 = std::env::args()
        .nth(1)
        .and_then(|argument| argument.parse().ok())
        .unwrap_or(scenario::SCENARIO_TICKS);

    // Process start to first observable result is part of the latency this candidate pays, so the
    // clock starts before the world exists rather than at the first tick.
    let started = Instant::now();

    let mut app = scenario::build_app(0xA1AD_E000);
    scenario::install_ai(&mut app, ai::enemy_ai);
    let built = started.elapsed();

    let simulation_started = Instant::now();
    if let Err(error) = app.run_ticks(ticks) {
        eprintln!("schedule error: {error}");
        std::process::exit(1);
    }
    let simulated = simulation_started.elapsed();

    println!("candidate    : A (pure Rust, compiled in)");
    println!("{}", scenario::report(&app));
    println!("build world  : {:.3} ms", built.as_secs_f64() * 1000.0);
    println!(
        "simulate     : {:.3} ms for {} ticks ({:.1} us/tick)",
        simulated.as_secs_f64() * 1000.0,
        ticks,
        simulated.as_secs_f64() * 1_000_000.0 / ticks as f64
    );
    println!(
        "total in-proc: {:.3} ms",
        started.elapsed().as_secs_f64() * 1000.0
    );
}
