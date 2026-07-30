//! Q1 candidate B, gameplay half: the same enemy AI, built as a reloadable dynamic library.
//!
//! **This is the file the latency measurement perturbs.** Only this crate rebuilds on an edit — the
//! host binary stays running — which is the entire appeal of option B.
//!
//! # What crosses the boundary, and why that is the risk
//!
//! `&mut World` is passed through directly. That is fast (no marshalling at all, and the behaviour
//! is byte-for-byte the same code as candidate A) but it leans on Rust's **unspecified** struct
//! layout: it is only sound while the host and this library were compiled by the same compiler
//! against the same versions of `amadeo-ecs` and `scenario`.
//!
//! Cargo gives us that inside one workspace build, and the host checks it as best it can, but there
//! is no mechanism that *enforces* it. Change a component's fields, rebuild only this crate, and the
//! host will happily reinterpret the old memory as the new layout. See ADR 0011 for why that risk is
//! the deciding factor rather than a footnote.
//!
//! The safe alternative — marshalling every component through a `#[repr(C)]` struct — was rejected
//! for this spike because it would mean hand-writing and hand-maintaining a serialisation shim for
//! every component type in the engine, which is a larger ongoing cost than the problem it solves.

use amadeo_app::SimRng;
use amadeo_ecs::World;
use scenario::{
    AiConfig, Enemy, EnemyState, Velocity, collect_enemies, draw_jitter, find_player, steer_toward,
    waypoint,
};

/// A version stamp the host reads to confirm it loaded the library it expected.
///
/// Bumped by hand if the boundary shape ever changes. It cannot detect a changed *component*
/// layout, which is precisely the hole described in the module docs — it catches the easy mistake,
/// not the dangerous one.
pub const ABI_VERSION: u32 = 1;

/// Reports [`ABI_VERSION`] to the host.
///
/// # Safety
///
/// Takes no arguments and returns a plain integer, so there is nothing for a caller to get wrong.
/// The `unsafe` on the attribute is Rust 2024's requirement for exporting an unmangled symbol at
/// all, not a claim that this function is dangerous.
#[unsafe(no_mangle)]
pub extern "C" fn amadeo_abi_version() -> u32 {
    ABI_VERSION
}

/// Decides one enemy's next state and velocity.
///
/// Identical to candidate A's `decide`. Kept as a separate copy rather than shared through
/// `scenario`, because the point of the measurement is "edit gameplay code, observe the result" —
/// and if the behaviour lived in a shared crate, an edit would rebuild the host too and the
/// comparison would be meaningless.
fn decide(
    enemy: &mut Enemy,
    position: [f32; 2],
    player: [f32; 2],
    config: &AiConfig,
    rng: &mut SimRng,
) -> Velocity {
    let dx = player[0] - position[0];
    let dy = player[1] - position[1];
    let distance_squared = dx * dx + dy * dy;

    let stopped = Velocity { x: 0.0, y: 0.0 };

    match enemy.state {
        EnemyState::Patrol => {
            if distance_squared <= config.sight_range * config.sight_range {
                enemy.state = EnemyState::Pursue;
                return stopped;
            }

            let target = waypoint(enemy.home, enemy.waypoint);
            let velocity = steer_toward(position, target, config.patrol_speed);

            let to_target_x = target[0] - position[0];
            let to_target_y = target[1] - position[1];
            let distance = (to_target_x * to_target_x + to_target_y * to_target_y).sqrt();
            if distance <= config.waypoint_radius {
                enemy.waypoint = (enemy.waypoint + 1) % 4;
            }

            velocity
        }

        EnemyState::Pursue => {
            if distance_squared > config.lose_range * config.lose_range {
                let jitter = draw_jitter(rng, config.search_jitter);
                enemy.state = EnemyState::Search;
                enemy.timer = config.search_ticks;
                enemy.last_seen = [player[0] + jitter[0], player[1] + jitter[1]];
                return stopped;
            }

            enemy.last_seen = player;
            steer_toward(position, player, config.pursue_speed)
        }

        EnemyState::Search => {
            if distance_squared <= config.sight_range * config.sight_range {
                enemy.state = EnemyState::Pursue;
                return stopped;
            }

            let velocity = steer_toward(position, enemy.last_seen, config.patrol_speed);
            enemy.timer = enemy.timer.saturating_sub(1);
            if enemy.timer == 0 {
                enemy.state = EnemyState::Patrol;
            }
            velocity
        }
    }
}

/// The exported entry point the host calls once per tick.
///
/// # Safety
///
/// The caller must pass a `&mut World` produced by a host built against the same versions of
/// `amadeo-ecs` and `scenario` as this library. Nothing verifies that; see the module docs.
///
/// `improper_ctypes_definitions` is allowed because `World` is not `#[repr(C)]` and never will be.
/// That warning is rustc correctly pointing at the exact hazard this candidate is being evaluated
/// for, so it is acknowledged here rather than silenced project-wide.
#[unsafe(no_mangle)]
#[allow(improper_ctypes_definitions)]
pub extern "C" fn amadeo_enemy_ai(world: &mut World) {
    let Some(config) = world.resource::<AiConfig>().copied() else {
        return;
    };
    let Some(player) = find_player(world) else {
        return;
    };
    let enemies = collect_enemies(world);

    world.with_resource_taken::<SimRng, ()>(|world, rng| {
        for (entity, mut enemy, position) in enemies {
            let velocity = decide(&mut enemy, position, player, &config, rng);

            if let Some(slot) = world.get_mut::<Enemy>(entity) {
                *slot = enemy;
            }
            if let Some(slot) = world.get_mut::<Velocity>(entity) {
                *slot = velocity;
            }
        }
    });
}
