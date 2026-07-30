//! Candidate A's enemy AI: ordinary Rust, compiled into the binary.
//!
//! **This is the file the latency measurement perturbs.** Changing one number here means rebuilding
//! this crate and relinking the binary, which is the entire cost model of option A.

use amadeo_app::SimRng;
use amadeo_ecs::World;
use scenario::{
    AiConfig, Enemy, EnemyState, Velocity, collect_enemies, draw_jitter, find_player, steer_toward,
    waypoint,
};

/// Decides one enemy's next state and velocity.
///
/// A pure function of its inputs apart from the RNG draw, which makes it directly comparable with
/// the Luau and WASM ports — they express exactly this and nothing else.
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
                // A state transition costs the enemy its movement for this tick. Stated explicitly
                // in the scenario spec so every port agrees; an implicit "carry on moving" would be
                // just as defensible and would silently change the state hash.
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

/// The enemy AI system.
///
/// # Why this collects before it writes
///
/// The behaviour needs three components at once — `Enemy` (write), `Transform2d` (read), and
/// `Velocity` (write) — and the ECS currently offers queries over at most two. So it reads
/// everything it needs into a `Vec`, decides, then writes back by entity handle.
///
/// That is a limitation today (`STATUS.md` lists it), but it happens to make this spike *fairer*:
/// a script or WASM boundary cannot hold a borrow into the world across a call, so it would have to
/// marshal in and out regardless. Every candidate pays the same collection cost, and the numbers
/// compare the reload mechanism rather than the query API.
pub fn enemy_ai(world: &mut World) {
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
