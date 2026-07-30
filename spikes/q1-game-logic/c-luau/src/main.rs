//! Q1 candidate C: gameplay in Luau, embedded via mlua.
//!
//! ```text
//! cargo run -p c-luau                          # straight through
//! cargo run -p c-luau -- --reload-at 900       # prove state survives a script swap
//! cargo run -p c-luau -- --reload-samples 50   # measure the swap on its own
//! ```
//!
//! # What the boundary actually looks like
//!
//! The script cannot touch the `World`. It receives a context table, mutates it in place, and the
//! host writes the results back. The tables are allocated **once** and refilled each tick rather
//! than rebuilt, which is the difference between a naive binding and a merely ordinary one — worth
//! doing, because the point is to give this candidate its best realistic showing.
//!
//! # Two frictions this prototype ran into, both of them findings
//!
//! **The runtime cannot be a `Service`.** `Service: Send + Sync`, and a Lua VM is neither. So the
//! runtime lives in an `Rc<RefCell<..>>` captured by the system closure instead of in the world.
//! That works — `system()` requires only `FnMut(&mut World) + 'static` — but it means the script
//! host is invisible to `world.resources` and to anything else that introspects the world, which is
//! a real cost against `docs/03-ai-native-design.md`.
//!
//! **Luau numbers are `f64`; components are `f32`.** Every arithmetic step in the script runs at
//! double precision and is rounded back to single on the way out. That is not a rounding nicety, it
//! is a different computation, and the state hash says so. See ADR 0011.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, Instant};

use amadeo_app::SimRng;
use amadeo_core::Rng;
use amadeo_ecs::World;
use mlua::{Function, Lua, Table};
use scenario::{AiConfig, Enemy, EnemyState, Velocity, collect_enemies, find_player};

/// Everything needed to run and reload the gameplay script.
struct LuauRuntime {
    /// The VM. Recreated on every reload — cheap, and it guarantees no stale globals survive.
    lua: Lua,
    /// The `update` function the script returns.
    update: Function,
    /// The context table handed to `update` each tick, allocated once and refilled.
    context: Table,
    /// One reusable table per enemy, so a tick allocates nothing on the Lua heap.
    enemy_tables: Vec<Table>,
    /// Where the script lives.
    path: PathBuf,
    /// Shared with the `random` function exposed to Lua.
    ///
    /// The host copies the simulation RNG in before each call and copies the advanced state back
    /// out after, so the script draws from the engine's stream and never from its own. `Rc<RefCell>`
    /// rather than a borrow because an mlua callback must be `'static`.
    rng_bridge: Rc<RefCell<Rng>>,
    /// How many times the script has been loaded.
    generation: u32,
}

impl LuauRuntime {
    /// Compiles the script and builds the reusable tables.
    fn load(path: PathBuf, enemy_count: usize, generation: u32) -> Result<Self, String> {
        let source = std::fs::read_to_string(&path)
            .map_err(|error| format!("could not read {}: {error}", path.display()))?;

        let lua = Lua::new();

        let rng_bridge = Rc::new(RefCell::new(Rng::new(0)));
        let rng_for_lua = Rc::clone(&rng_bridge);

        // The only randomness the script is allowed. `math.random` would keep its own state and
        // silently break replay determinism, so a real engine would remove it from the sandbox.
        let random = lua
            .create_function(move |_, (min, max): (f32, f32)| {
                Ok(rng_for_lua.borrow_mut().range_f32(min, max))
            })
            .map_err(|error| format!("could not create the `random` binding: {error}"))?;

        let update: Function = lua
            .load(&source)
            .set_name(path.to_string_lossy().as_ref())
            .eval()
            .map_err(|error| format!("script error in {}: {error}", path.display()))?;

        let context = lua
            .create_table()
            .map_err(|error| format!("could not create the context table: {error}"))?;
        let enemies = lua
            .create_table()
            .map_err(|error| format!("could not create the enemy table: {error}"))?;

        let mut enemy_tables = Vec::with_capacity(enemy_count);
        for index in 0..enemy_count {
            let table = lua
                .create_table()
                .map_err(|error| format!("could not create enemy table {index}: {error}"))?;
            enemies
                .set(index + 1, table.clone())
                .map_err(|error| format!("could not store enemy table {index}: {error}"))?;
            enemy_tables.push(table);
        }

        context
            .set("enemies", enemies)
            .and_then(|()| context.set("random", random))
            .and_then(|()| context.set("count", enemy_count))
            .map_err(|error| format!("could not populate the context table: {error}"))?;

        Ok(Self {
            lua,
            update,
            context,
            enemy_tables,
            path,
            rng_bridge,
            generation,
        })
    }

    /// Recompiles the script, keeping the world untouched.
    fn reload(&mut self) -> Result<Duration, String> {
        let started = Instant::now();
        let replacement = Self::load(
            self.path.clone(),
            self.enemy_tables.len(),
            self.generation + 1,
        )?;
        *self = replacement;
        Ok(started.elapsed())
    }

    /// Pushes the config table across. Only needed when the config changes, but done per load
    /// rather than per tick because these values are tunables, not state.
    fn publish_config(&self, config: &AiConfig) -> Result<(), String> {
        let table = self
            .lua
            .create_table()
            .map_err(|error| format!("could not create the config table: {error}"))?;
        table
            .set("sight_range", config.sight_range)
            .and_then(|()| table.set("lose_range", config.lose_range))
            .and_then(|()| table.set("patrol_speed", config.patrol_speed))
            .and_then(|()| table.set("pursue_speed", config.pursue_speed))
            .and_then(|()| table.set("search_ticks", config.search_ticks))
            .and_then(|()| table.set("waypoint_radius", config.waypoint_radius))
            .and_then(|()| table.set("search_jitter", config.search_jitter))
            .and_then(|()| self.context.set("config", table))
            .map_err(|error| format!("could not publish the config: {error}"))
    }

    /// One tick: marshal out, call the script, marshal back.
    fn run_tick(&mut self, world: &mut World) -> Result<(), String> {
        let Some(player) = find_player(world) else {
            return Ok(());
        };
        let enemies = collect_enemies(world);

        self.context
            .set("player_x", player[0])
            .and_then(|()| self.context.set("player_y", player[1]))
            .map_err(|error| format!("could not set the player position: {error}"))?;

        for (slot, (_entity, enemy, position)) in enemies.iter().enumerate() {
            let Some(table) = self.enemy_tables.get(slot) else {
                break;
            };
            table
                .set("x", position[0])
                .and_then(|()| table.set("y", position[1]))
                .and_then(|()| table.set("state", enemy.state.as_u32()))
                .and_then(|()| table.set("timer", enemy.timer))
                .and_then(|()| table.set("home_x", enemy.home[0]))
                .and_then(|()| table.set("home_y", enemy.home[1]))
                .and_then(|()| table.set("last_x", enemy.last_seen[0]))
                .and_then(|()| table.set("last_y", enemy.last_seen[1]))
                .and_then(|()| table.set("waypoint", enemy.waypoint))
                .map_err(|error| format!("could not marshal enemy {slot}: {error}"))?;
        }

        // The world is not borrowed while the script runs -- it cannot be, since nothing about the
        // world crosses the boundary. Taking the RNG out and putting it back is the whole of the
        // engine-side state the script can influence.
        let mut sim_rng = world
            .remove_resource::<SimRng>()
            .ok_or_else(|| "SimRng resource is missing".to_string())?;
        *self.rng_bridge.borrow_mut() = sim_rng.0.clone();

        let call_result = self.update.call::<()>(&self.context);

        sim_rng.0 = self.rng_bridge.borrow().clone();
        world.insert_resource(sim_rng);

        call_result.map_err(|error| format!("script error in `update`: {error}"))?;

        for (slot, (entity, _enemy, _position)) in enemies.iter().enumerate() {
            let Some(table) = self.enemy_tables.get(slot) else {
                break;
            };

            let read = |key: &str| -> Result<f32, String> {
                table
                    .get::<f32>(key)
                    .map_err(|error| format!("enemy {slot} field `{key}`: {error}"))
            };
            let read_u32 = |key: &str| -> Result<u32, String> {
                table
                    .get::<u32>(key)
                    .map_err(|error| format!("enemy {slot} field `{key}`: {error}"))
            };

            let updated = Enemy {
                state: EnemyState::from_u32(read_u32("state")?),
                timer: read_u32("timer")?,
                home: [read("home_x")?, read("home_y")?],
                last_seen: [read("last_x")?, read("last_y")?],
                waypoint: read_u32("waypoint")?,
            };
            let velocity = Velocity {
                x: read("vx")?,
                y: read("vy")?,
            };

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

/// Locates a script by bare name inside this crate's `scripts/` directory.
///
/// Relative to the crate, so `cargo run` works from anywhere in the workspace.
fn script_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("scripts")
        .join(format!("{name}.luau"))
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

    // `--script null` runs a do-nothing script, which isolates the cost of the marshalling boundary
    // from the cost of actually executing Luau. The difference between the two is the answer to
    // "is the scripting language slow, or is the binding slow".
    let script = arguments
        .iter()
        .position(|argument| argument == "--script")
        .and_then(|index| arguments.get(index + 1))
        .cloned()
        .unwrap_or_else(|| "enemy".to_string());

    let started = Instant::now();
    let mut app = scenario::build_app(0xA1AD_E000);

    let runtime = match LuauRuntime::load(script_path(&script), scenario::ENEMY_COUNT, 0) {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    };
    let config = app
        .world
        .resource::<AiConfig>()
        .copied()
        .unwrap_or_default();
    if let Err(error) = runtime.publish_config(&config) {
        eprintln!("{error}");
        std::process::exit(1);
    }

    // The runtime lives here rather than in the world: `Service` requires `Send + Sync` and a Lua
    // VM is neither. See the module docs -- this is a finding, not a workaround.
    let runtime = Rc::new(RefCell::new(runtime));
    let runtime_for_system = Rc::clone(&runtime);

    scenario::install_ai(&mut app, move |world| {
        if let Err(error) = runtime_for_system.borrow_mut().run_tick(world) {
            // A script error degrades to "this system did nothing" rather than taking the process
            // down, so the broken state stays inspectable (docs/03 Pillar 5).
            eprintln!("enemy_ai: {error}");
        }
    });
    let built = started.elapsed();

    println!("candidate    : C (embedded Luau)");

    if let Some(samples) = reload_samples {
        let mut timings = Vec::new();
        for _ in 0..samples {
            match runtime.borrow_mut().reload() {
                Ok(elapsed) => timings.push(elapsed.as_secs_f64() * 1000.0),
                Err(error) => {
                    eprintln!("reload failed: {error}");
                    std::process::exit(1);
                }
            }
        }
        if let Err(error) = runtime.borrow().publish_config(&config) {
            eprintln!("{error}");
            std::process::exit(1);
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

            match runtime.borrow_mut().reload() {
                Ok(elapsed) => reload_elapsed = Some(elapsed),
                Err(error) => {
                    eprintln!("reload failed: {error}");
                    std::process::exit(1);
                }
            }
            if let Err(error) = runtime.borrow().publish_config(&config) {
                eprintln!("{error}");
                std::process::exit(1);
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
