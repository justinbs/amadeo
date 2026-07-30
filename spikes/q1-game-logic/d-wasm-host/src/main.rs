//! Q1 candidate D: gameplay as a WebAssembly module, hosted by wasmtime.
//!
//! ```text
//! cd d-wasm-logic && cargo build --release --target wasm32-unknown-unknown
//! cargo run -p d-wasm-host
//! cargo run -p d-wasm-host -- --reload-at 900
//! cargo run -p d-wasm-host -- --reload-samples 20
//! ```
//!
//! # The claim this candidate is here to test
//!
//! That it can be **both** hot-reloadable *and* bit-identical to native Rust. Luau achieves the
//! first and fails the second; the cdylib achieves the second and pays for it with `unsafe` and a
//! layout contract nothing enforces. If WebAssembly gets both, it dominates.
//!
//! The test is the same as everywhere else in this spike: the final state hash must equal
//! candidate A's, exactly.
//!
//! # The boundary
//!
//! A flat array of 48-byte records in the module's linear memory, written and read through
//! wasmtime's bounds-checked API. No pointers are dereferenced on either side — compare the cdylib
//! host, which dereferences a `&mut World` whose layout is verified by nothing at all.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use amadeo_app::SimRng;
use amadeo_core::Rng;
use amadeo_ecs::World;
use scenario::{AiConfig, Enemy, EnemyState, Velocity, collect_enemies, find_player};
use wasmtime::{Caller, Engine, Linker, Memory, Module, Store, TypedFunc};

/// The signature of the module's per-tick entry point.
type EnemyAiFunc = TypedFunc<(u32, f32, f32, f32, f32, f32, f32, u32, f32, f32), ()>;

/// How many bytes one enemy record occupies. Read from the module rather than hardcoded, so a
/// layout change on the guest side is detected instead of silently misread.
const EXPECTED_RECORD_SIZE: u32 = 48;

/// State the host exposes to the module.
struct HostState {
    /// A copy of the simulation RNG, refreshed before every call and read back after.
    rng: Rng,
}

/// Everything needed to run and reload the gameplay module.
struct WasmRuntime {
    /// Reused across reloads: it owns the JIT configuration and its compilation caches.
    engine: Engine,
    /// Rebuilt on every reload. Holds the module's linear memory and the host state.
    store: Store<HostState>,
    /// The module's linear memory.
    memory: Memory,
    /// The per-tick entry point.
    enemy_ai: EnemyAiFunc,
    /// Where the enemy buffer starts in linear memory.
    buffer_offset: usize,
    /// Bytes per record, as reported by the module.
    record_size: usize,
    /// Where the module lives on disk.
    path: PathBuf,
    /// Reused marshalling buffer, so a tick allocates nothing.
    scratch: Vec<u8>,
    /// How many times the module has been loaded.
    generation: u32,
    /// How long the last JIT compile took, separated from instantiation.
    last_compile: Duration,
}

impl WasmRuntime {
    /// Compiles and instantiates the module.
    fn load(
        engine: Engine,
        path: PathBuf,
        enemy_count: usize,
        generation: u32,
    ) -> Result<Self, String> {
        let compile_started = Instant::now();
        let module = Module::from_file(&engine, &path).map_err(|error| {
            format!(
                "could not compile {}: {error}\n\
                 hint: build it first with\n  \
                 cd d-wasm-logic && cargo build --release --target wasm32-unknown-unknown",
                path.display()
            )
        })?;
        let last_compile = compile_started.elapsed();

        let mut linker = Linker::new(&engine);
        linker
            .func_wrap(
                "env",
                "host_random",
                |mut caller: Caller<'_, HostState>, min: f32, max: f32| -> f32 {
                    caller.data_mut().rng.range_f32(min, max)
                },
            )
            .map_err(|error| format!("could not link `host_random`: {error}"))?;

        let mut store = Store::new(&engine, HostState { rng: Rng::new(0) });
        let instance = linker
            .instantiate(&mut store, &module)
            .map_err(|error| format!("could not instantiate the module: {error}"))?;

        let memory = instance
            .get_memory(&mut store, "memory")
            .ok_or_else(|| "the module exports no `memory`".to_string())?;

        let record_size: TypedFunc<(), u32> = instance
            .get_typed_func(&mut store, "record_size")
            .map_err(|error| format!("missing export `record_size`: {error}"))?;
        let record_size = record_size
            .call(&mut store, ())
            .map_err(|error| format!("`record_size` trapped: {error}"))?;
        if record_size != EXPECTED_RECORD_SIZE {
            return Err(format!(
                "module reports a {record_size}-byte enemy record, host packs \
                 {EXPECTED_RECORD_SIZE}; the two sides disagree about layout"
            ));
        }

        let reserve: TypedFunc<u32, u32> = instance
            .get_typed_func(&mut store, "reserve")
            .map_err(|error| format!("missing export `reserve`: {error}"))?;
        let buffer_offset = reserve
            .call(&mut store, enemy_count as u32)
            .map_err(|error| format!("`reserve` trapped: {error}"))?;

        let enemy_ai: EnemyAiFunc = instance
            .get_typed_func(&mut store, "enemy_ai")
            .map_err(|error| format!("missing export `enemy_ai`: {error}"))?;

        Ok(Self {
            engine,
            store,
            memory,
            enemy_ai,
            buffer_offset: buffer_offset as usize,
            record_size: record_size as usize,
            path,
            scratch: vec![0u8; enemy_count * record_size as usize],
            generation,
            last_compile,
        })
    }

    /// Recompiles and re-instantiates, keeping the world untouched.
    ///
    /// The whole simulation lives host-side, so "reload" really is just "throw the module away and
    /// make a new one". Nothing has to be migrated, which is the structural advantage this shares
    /// with the scripting candidate and does not share with a cdylib holding statics.
    fn reload(&mut self, enemy_count: usize) -> Result<Duration, String> {
        let started = Instant::now();
        let replacement = Self::load(
            self.engine.clone(),
            self.path.clone(),
            enemy_count,
            self.generation + 1,
        )?;
        *self = replacement;
        Ok(started.elapsed())
    }

    /// One tick: pack the records in, call the module, unpack them back out.
    fn run_tick(&mut self, world: &mut World) -> Result<(), String> {
        let Some(config) = world.resource::<AiConfig>().copied() else {
            return Ok(());
        };
        let Some(player) = find_player(world) else {
            return Ok(());
        };
        let enemies = collect_enemies(world);

        for (slot, (_entity, enemy, position)) in enemies.iter().enumerate() {
            let base = slot * self.record_size;
            let Some(record) = self.scratch.get_mut(base..base + self.record_size) else {
                break;
            };
            pack_record(record, enemy, *position);
        }

        self.memory
            .write(&mut self.store, self.buffer_offset, &self.scratch)
            .map_err(|error| format!("could not write the enemy buffer: {error}"))?;

        let mut sim_rng = world
            .remove_resource::<SimRng>()
            .ok_or_else(|| "SimRng resource is missing".to_string())?;
        self.store.data_mut().rng = sim_rng.0.clone();

        let call_result = self.enemy_ai.call(
            &mut self.store,
            (
                enemies.len() as u32,
                player[0],
                player[1],
                config.sight_range,
                config.lose_range,
                config.patrol_speed,
                config.pursue_speed,
                config.search_ticks,
                config.waypoint_radius,
                config.search_jitter,
            ),
        );

        sim_rng.0 = self.store.data().rng.clone();
        world.insert_resource(sim_rng);

        call_result.map_err(|error| format!("`enemy_ai` trapped: {error}"))?;

        self.memory
            .read(&self.store, self.buffer_offset, &mut self.scratch)
            .map_err(|error| format!("could not read the enemy buffer: {error}"))?;

        for (slot, (entity, _enemy, _position)) in enemies.iter().enumerate() {
            let base = slot * self.record_size;
            let Some(record) = self.scratch.get(base..base + self.record_size) else {
                break;
            };
            let (updated, velocity) = unpack_record(record);

            if let Some(existing) = world.get_mut::<Enemy>(*entity) {
                *existing = updated;
            }
            if let Some(existing) = world.get_mut::<Velocity>(*entity) {
                *existing = velocity;
            }
        }

        Ok(())
    }
}

/// Writes one enemy into its 48-byte slot.
///
/// Field order must match `EnemyRecord` in `d-wasm-logic` exactly. Written by hand rather than with
/// a serialisation crate so the layout contract is visible in one place on each side — which is
/// also the honest cost of this approach, and it grows with every component the boundary carries.
fn pack_record(into: &mut [u8], enemy: &Enemy, position: [f32; 2]) {
    let floats = [
        position[0],
        position[1],
        enemy.home[0],
        enemy.home[1],
        enemy.last_seen[0],
        enemy.last_seen[1],
        0.0, // vx, an output
        0.0, // vy, an output
    ];
    for (index, value) in floats.iter().enumerate() {
        into[index * 4..index * 4 + 4].copy_from_slice(&value.to_le_bytes());
    }

    let integers = [enemy.state.as_u32(), enemy.timer, enemy.waypoint, 0];
    for (index, value) in integers.iter().enumerate() {
        let base = 32 + index * 4;
        into[base..base + 4].copy_from_slice(&value.to_le_bytes());
    }
}

/// Reads one enemy back out of its 48-byte slot.
fn unpack_record(from: &[u8]) -> (Enemy, Velocity) {
    let float_at = |index: usize| -> f32 {
        let mut bytes = [0u8; 4];
        bytes.copy_from_slice(&from[index * 4..index * 4 + 4]);
        f32::from_le_bytes(bytes)
    };
    let integer_at = |index: usize| -> u32 {
        let base = 32 + index * 4;
        let mut bytes = [0u8; 4];
        bytes.copy_from_slice(&from[base..base + 4]);
        u32::from_le_bytes(bytes)
    };

    let enemy = Enemy {
        state: EnemyState::from_u32(integer_at(0)),
        timer: integer_at(1),
        home: [float_at(2), float_at(3)],
        last_seen: [float_at(4), float_at(5)],
        waypoint: integer_at(2),
    };
    let velocity = Velocity {
        x: float_at(6),
        y: float_at(7),
    };
    (enemy, velocity)
}

fn module_path() -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.pop();
    path.join("d-wasm-logic")
        .join("target")
        .join("wasm32-unknown-unknown")
        .join("release")
        .join("d_wasm_logic.wasm")
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

    let runtime = match WasmRuntime::load(
        Engine::default(),
        module_path(),
        scenario::ENEMY_COUNT,
        0,
    ) {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    };
    let first_compile = runtime.last_compile;

    // Same story as the Luau candidate: `Service` requires `Send + Sync`, and a `Store` is not
    // `Sync`, so the runtime lives in the system closure rather than in the world.
    let runtime = std::rc::Rc::new(std::cell::RefCell::new(runtime));
    let runtime_for_system = std::rc::Rc::clone(&runtime);

    scenario::install_ai(&mut app, move |world| {
        if let Err(error) = runtime_for_system.borrow_mut().run_tick(world) {
            eprintln!("enemy_ai: {error}");
        }
    });
    let built = started.elapsed();

    println!("candidate    : D (WASM module via wasmtime)");
    println!(
        "first compile: {:.3} ms (JIT, 18 KB module)",
        first_compile.as_secs_f64() * 1000.0
    );

    if let Some(samples) = reload_samples {
        let mut timings = Vec::new();
        let mut compiles = Vec::new();
        for _ in 0..samples {
            // Both numbers are read inside one borrow. Reading `last_compile` through a second
            // `runtime.borrow()` while the `borrow_mut()` temporary is still alive panics at
            // runtime -- a match scrutinee's temporaries live for the whole match.
            let outcome = {
                let mut runtime = runtime.borrow_mut();
                runtime
                    .reload(scenario::ENEMY_COUNT)
                    .map(|elapsed| (elapsed, runtime.last_compile))
            };

            match outcome {
                Ok((elapsed, compile)) => {
                    timings.push(elapsed.as_secs_f64() * 1000.0);
                    compiles.push(compile.as_secs_f64() * 1000.0);
                }
                Err(error) => {
                    eprintln!("reload failed: {error}");
                    std::process::exit(1);
                }
            }
        }
        timings.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        compiles.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        println!(
            "reload swap  : median {:.3} ms over {} samples (min {:.3}, max {:.3})",
            timings[timings.len() / 2],
            timings.len(),
            timings.first().copied().unwrap_or_default(),
            timings.last().copied().unwrap_or_default()
        );
        println!(
            "  of which JIT: median {:.3} ms",
            compiles[compiles.len() / 2]
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

            match runtime.borrow_mut().reload(scenario::ENEMY_COUNT) {
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
                    "STATE LOST: the reload itself changed the world state hash ({:016x} -> {:016x})",
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
