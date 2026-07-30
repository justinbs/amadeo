//! Q1 candidate B, engine half: loads the gameplay library and swaps it without restarting.
//!
//! ```text
//! cargo build -p b-cdylib-logic          # produce the library
//! cargo run -p b-cdylib-host             # straight through, no reload
//! cargo run -p b-cdylib-host -- --reload-at 900
//! cargo run -p b-cdylib-host -- --reload-samples 20
//! ```
//!
//! # The two things this exists to prove
//!
//! 1. **State survives a reload.** Proven by hash equality rather than by inspection: running 1800
//!    ticks with a reload at tick 900 must produce *exactly* the state hash that candidate A
//!    produces running 1800 ticks with no reload at all. Any state loss, any re-initialisation, any
//!    RNG divergence, and the hashes differ.
//! 2. **How long the swap takes**, separated from how long the rebuild takes. The rebuild is
//!    measured externally with cargo; this measures only the unload/copy/load/resolve cycle.
//!
//! # Windows DLL locking
//!
//! Windows locks a loaded DLL, so cargo cannot overwrite `b_cdylib_logic.dll` while this process
//! holds it open — the build fails with a permission error rather than anything self-explanatory.
//! The fix is to never load the build output directly: copy it to a uniquely named staging file and
//! load that. The original stays writable and cargo is none the wiser.

use std::path::PathBuf;
use std::time::Instant;

use amadeo_ecs::World;
use libloading::{Library, Symbol};

/// The signature of the gameplay entry point.
///
/// Duplicated from `b-cdylib-logic` rather than imported — see the note in `Cargo.toml`. If the two
/// ever disagree, the result is undefined behaviour, not a link error.
type AiFn = extern "C" fn(&mut World);

/// What [`amadeo_abi_version`] in the library must return.
const EXPECTED_ABI_VERSION: u32 = 1;

/// The loaded gameplay library, plus the resolved entry point.
///
/// The two live in one struct so they cannot get out of sync: `run` is only valid while `library`
/// is loaded, and dropping this drops both together. Storing the function pointer separately from
/// the library it came from is the classic way to get a use-after-unload crash.
#[derive(Debug)]
struct HotLogic {
    /// Kept alive purely so `run` stays valid. Never called directly after construction.
    #[allow(dead_code)]
    library: Library,
    /// The gameplay entry point, copied out of the library's symbol table.
    run: AiFn,
    /// How many times the library has been loaded. Also names the staging file.
    generation: u32,
}

// Engine machinery, not simulation state: a loaded library must never influence a state hash.
// This is exactly the distinction ADR 0009 drew, and a script or WASM runtime would file here too.
impl amadeo_ecs::Service for HotLogic {}

/// Where cargo puts the gameplay library.
fn library_source() -> PathBuf {
    // `std::env::current_exe` lands in target/debug, which is where the cdylib is written too.
    let mut path = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("."));
    path.pop();
    path.join(format!("{}b_cdylib_logic{}", DLL_PREFIX, DLL_SUFFIX))
}

#[cfg(windows)]
const DLL_PREFIX: &str = "";
#[cfg(windows)]
const DLL_SUFFIX: &str = ".dll";
#[cfg(not(windows))]
const DLL_PREFIX: &str = "lib";
#[cfg(not(windows))]
const DLL_SUFFIX: &str = ".so";

/// A uniquely named copy to load, so the original stays writable while we hold this one open.
fn staging_path(generation: u32) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push("amadeo-q1-spike");
    let _ = std::fs::create_dir_all(&path);
    path.push(format!(
        "{DLL_PREFIX}logic-{}-{generation}{DLL_SUFFIX}",
        std::process::id()
    ));
    path
}

/// Copies the gameplay library aside and loads it.
///
/// Returns a human-readable error rather than panicking: a failed reload should leave the previous
/// version running so the session can continue, which is half the value of hot reload in the first
/// place (`docs/03-ai-native-design.md` Pillar 5).
fn load(generation: u32) -> Result<HotLogic, String> {
    let source = library_source();
    let staged = staging_path(generation);

    std::fs::copy(&source, &staged).map_err(|error| {
        format!(
            "could not stage the gameplay library from {} to {}: {error}\n\
             hint: run `cargo build -p b-cdylib-logic` first",
            source.display(),
            staged.display()
        )
    })?;

    // SAFETY: loading a library runs its initialisers, which can execute arbitrary code. We built
    // this file ourselves moments ago from our own workspace, which is as much as any dynamic
    // loading scheme can promise. This is the `unsafe` that invariant-level `forbid` would reject.
    let library = unsafe { Library::new(&staged) }
        .map_err(|error| format!("could not load {}: {error}", staged.display()))?;

    let version = {
        // SAFETY: the symbol's type must match its definition in the library. Duplicated by hand;
        // nothing verifies it.
        let symbol: Symbol<extern "C" fn() -> u32> =
            unsafe { library.get(b"amadeo_abi_version\0") }
                .map_err(|error| format!("missing symbol `amadeo_abi_version`: {error}"))?;
        symbol()
    };
    if version != EXPECTED_ABI_VERSION {
        return Err(format!(
            "gameplay library reports ABI version {version}, host expects {EXPECTED_ABI_VERSION}; \
             rebuild both sides"
        ));
    }

    let run = {
        // SAFETY: as above. The copy out of the `Symbol` is what lets the pointer be stored beside
        // the library rather than borrowing from it.
        let symbol: Symbol<AiFn> = unsafe { library.get(b"amadeo_enemy_ai\0") }
            .map_err(|error| format!("missing symbol `amadeo_enemy_ai`: {error}"))?;
        *symbol
    };

    Ok(HotLogic {
        library,
        run,
        generation,
    })
}

/// The system registered with the schedule. Calls whatever version is currently loaded.
///
/// Note what is *not* stored anywhere: no trait object, no closure, and no value whose vtable lives
/// inside the library. Those are the things that turn into dangling pointers the moment the old
/// library is unloaded, and avoiding them is a permanent design constraint on this approach, not a
/// detail of this prototype.
fn hot_enemy_ai(world: &mut World) {
    world.with_service_taken::<HotLogic, ()>(|world, logic| {
        (logic.run)(world);
    });
}

/// Swaps in a freshly built library, keeping the world exactly as it is.
///
/// Returns how long the swap took. The old library is dropped — and therefore unloaded — before the
/// new one is staged, which is what lets cargo overwrite the build output between reloads.
fn reload(world: &mut World) -> Result<std::time::Duration, String> {
    let started = Instant::now();

    let generation = world
        .service::<HotLogic>()
        .map_or(0, |logic| logic.generation)
        + 1;

    // Drop first: unloads the old library and releases the staging file's lock.
    let previous = world.remove_service::<HotLogic>();
    drop(previous);

    match load(generation) {
        Ok(logic) => {
            world.insert_service(logic);
            Ok(started.elapsed())
        }
        Err(error) => Err(error),
    }
}

fn main() {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let flag_value = |name: &str| -> Option<u64> {
        arguments
            .iter()
            .position(|argument| argument == name)
            .and_then(|index| arguments.get(index + 1))
            .and_then(|value| value.parse().ok())
    };

    let ticks = flag_value("--ticks").unwrap_or(scenario::SCENARIO_TICKS);
    let reload_at = flag_value("--reload-at");
    let reload_samples = flag_value("--reload-samples");

    let started = Instant::now();
    let mut app = scenario::build_app(0xA1AD_E000);

    let logic = match load(0) {
        Ok(logic) => logic,
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    };
    app.insert_service(logic);
    scenario::install_ai(&mut app, hot_enemy_ai);
    let built = started.elapsed();

    println!("candidate    : B (hot-reloaded cdylib)");

    // Measure the swap on its own, repeated, with no simulation in between.
    if let Some(samples) = reload_samples {
        let mut timings = Vec::new();
        for _ in 0..samples {
            match reload(&mut app.world) {
                Ok(elapsed) => timings.push(elapsed.as_secs_f64() * 1000.0),
                Err(error) => {
                    eprintln!("reload failed: {error}");
                    std::process::exit(1);
                }
            }
        }
        timings.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let median = timings[timings.len() / 2];
        println!(
            "reload swap  : median {median:.3} ms over {} samples (min {:.3}, max {:.3})",
            timings.len(),
            timings.first().copied().unwrap_or_default(),
            timings.last().copied().unwrap_or_default()
        );
    }

    let simulation_started = Instant::now();
    let mut reload_elapsed = None;

    match reload_at {
        Some(at) if at < ticks => {
            if let Err(error) = app.run_ticks(at) {
                eprintln!("schedule error: {error}");
                std::process::exit(1);
            }
            let before = scenario::report(&app);
            println!("before reload: {before}");

            match reload(&mut app.world) {
                Ok(elapsed) => reload_elapsed = Some(elapsed),
                Err(error) => {
                    eprintln!("reload failed: {error}");
                    std::process::exit(1);
                }
            }

            let after = scenario::report(&app);
            println!("after  reload: {after}");
            if before.state_hash != after.state_hash {
                eprintln!(
                    "STATE LOST: the reload itself changed the world state hash \
                     ({:016x} -> {:016x})",
                    before.state_hash, after.state_hash
                );
                std::process::exit(1);
            }
            println!("state survived the swap (hash unchanged across the reload)");

            if let Err(error) = app.run_ticks(ticks - at) {
                eprintln!("schedule error: {error}");
                std::process::exit(1);
            }
        }
        _ => {
            if let Err(error) = app.run_ticks(ticks) {
                eprintln!("schedule error: {error}");
                std::process::exit(1);
            }
        }
    }

    let simulated = simulation_started.elapsed();

    println!("{}", scenario::report(&app));
    println!("build world  : {:.3} ms", built.as_secs_f64() * 1000.0);
    println!(
        "simulate     : {:.3} ms for {} ticks ({:.1} us/tick)",
        simulated.as_secs_f64() * 1000.0,
        ticks,
        simulated.as_secs_f64() * 1_000_000.0 / ticks as f64
    );
    if let Some(elapsed) = reload_elapsed {
        println!(
            "reload swap  : {:.3} ms (mid-run, world preserved)",
            elapsed.as_secs_f64() * 1000.0
        );
    }
    println!(
        "total in-proc: {:.3} ms",
        started.elapsed().as_secs_f64() * 1000.0
    );
}
