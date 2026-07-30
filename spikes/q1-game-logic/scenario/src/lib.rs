//! The shared benchmark for the Q1 spike.
//!
//! Every candidate — pure Rust, hot-reloaded cdylib, embedded Luau, WASM — runs *this* world,
//! *this* player path, and *this* enemy behaviour. The only thing that varies between them is the
//! mechanism by which the enemy AI system is authored and reloaded. That is the whole point: if the
//! scenario differed even slightly, the latency numbers would not be comparable.
//!
//! # The benchmark task
//!
//! A three-state enemy AI, chosen because it is the smallest thing that is honestly *non-trivial*
//! and because it previews real work — `mod-behaviour` in M3 needs exactly this.
//!
//! It exercises everything a scripting boundary has to get right:
//!
//! | Requirement | Why it is in the benchmark |
//! |---|---|
//! | read+write components | the basic thing gameplay does |
//! | read a resource ([`AiConfig`]) | tunables must be reachable from game logic |
//! | consume the engine RNG | a script must **not** use its own `math.random` (invariant I3) |
//! | branching state machine | measures ergonomics, not just field access |
//! | a tunable constant | the most common edit an agent makes when tuning behaviour |
//!
//! # The specification
//!
//! Every implementation must produce identical results. Stated precisely so a Lua or WASM port has
//! no room to drift:
//!
//! ```text
//! for each entity with (Enemy, Transform2d, Velocity):
//!     delta   = player_position - enemy_position
//!     dist_sq = delta.x^2 + delta.y^2
//!
//!     Patrol:
//!         if dist_sq <= sight_range^2  -> state = Pursue
//!         else:
//!             target = waypoint(home, enemy.waypoint)
//!             steer_toward(target, patrol_speed)
//!             if distance to target <= waypoint_radius:
//!                 enemy.waypoint = (enemy.waypoint + 1) % 4
//!
//!     Pursue:
//!         if dist_sq > lose_range^2:
//!             state     = Search
//!             timer     = search_ticks
//!             last_seen = player_position + rng jitter on each axis
//!         else:
//!             steer_toward(player_position, pursue_speed)
//!             last_seen = player_position
//!
//!     Search:
//!         if dist_sq <= sight_range^2 -> state = Pursue
//!         else:
//!             steer_toward(last_seen, patrol_speed)
//!             timer -= 1
//!             if timer == 0 -> state = Patrol
//! ```
//!
//! `steer_toward(target, speed)` sets velocity to the unit vector toward `target` times `speed`,
//! or to zero if the target is closer than `1e-6`.
//!
//! **RNG order matters.** The jitter draws x then y, and enemies are visited in query order. Any
//! implementation that draws in a different order produces a different state hash — which is the
//! point: the hash is what proves two candidates computed the same thing.
//!
//! # No transcendentals, deliberately
//!
//! The player path is piecewise linear and the AI uses only `sqrt`. `sin`/`cos` route through the
//! platform's libm and are **not** guaranteed identical across platforms, which would confound a
//! cross-candidate hash comparison with a question that belongs to a different investigation.
//! `sqrt` is IEEE-754 correctly rounded, so it is safe.

use amadeo_app::{App, SimRng, Stage, system};
use amadeo_core::{FIXED_DT, StableHash, StableHasher};
use amadeo_ecs::{Component, Entity, Resource, World};
use amadeo_render::Transform2d;

/// How many enemies the benchmark world contains.
///
/// 64 is enough that per-entity boundary crossing costs show up in a per-tick measurement, and
/// small enough to stay a plausible scene rather than a synthetic stress test.
pub const ENEMY_COUNT: usize = 64;

/// How many ticks a measured run simulates.
///
/// 1800 ticks is 30 seconds of simulated time at 60 Hz — long enough for every enemy to cycle
/// through all three states several times as the player completes multiple laps.
pub const SCENARIO_TICKS: u64 = 1800;

/// The label the AI system must register under.
///
/// Fixed so ordering constraints (`after(DRIVE_PLAYER)`, `before(INTEGRATE)`) are identical in
/// every host, whatever mechanism actually executes the behaviour.
pub const ENEMY_AI: &str = "enemy_ai";

/// The label of the system that moves the player along its scripted path.
pub const DRIVE_PLAYER: &str = "drive_player";

/// The label of the system that applies velocity to position.
pub const INTEGRATE: &str = "integrate";

// --- Components ---

/// Which behaviour an enemy is currently running.
///
/// A plain enum with an explicit `u32` mapping, because a script or a WASM module has to be able to
/// round-trip this value across a boundary that has no concept of a Rust enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnemyState {
    /// Walking a fixed circuit around home.
    Patrol,
    /// Moving directly at the player.
    Pursue,
    /// Heading for where the player was last seen, then giving up.
    Search,
}

impl EnemyState {
    /// The wire representation, for boundaries that only speak numbers.
    #[must_use]
    pub fn as_u32(self) -> u32 {
        match self {
            EnemyState::Patrol => 0,
            EnemyState::Pursue => 1,
            EnemyState::Search => 2,
        }
    }

    /// Rebuilds a state from its wire representation.
    ///
    /// An unrecognised value becomes `Patrol` rather than an error: a script that writes garbage
    /// should produce visibly wrong behaviour, not take the process down (`docs/03` Pillar 5).
    #[must_use]
    pub fn from_u32(value: u32) -> Self {
        match value {
            1 => EnemyState::Pursue,
            2 => EnemyState::Search,
            _ => EnemyState::Patrol,
        }
    }
}

/// An enemy's behaviour state.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Enemy {
    /// Which behaviour is running.
    pub state: EnemyState,
    /// Ticks remaining before `Search` gives up and returns to `Patrol`.
    pub timer: u32,
    /// The centre of this enemy's patrol circuit.
    pub home: [f32; 2],
    /// Where the player was last seen, plus jitter. Only meaningful in `Search`.
    pub last_seen: [f32; 2],
    /// Which of the four patrol waypoints is being walked toward.
    pub waypoint: u32,
}

impl StableHash for Enemy {
    fn stable_hash(&self, hasher: &mut StableHasher) {
        hasher.write_u32(self.state.as_u32());
        hasher.write_u32(self.timer);
        hasher.write_f32(self.home[0]);
        hasher.write_f32(self.home[1]);
        hasher.write_f32(self.last_seen[0]);
        hasher.write_f32(self.last_seen[1]);
        hasher.write_u32(self.waypoint);
    }
}

impl Component for Enemy {}

/// How fast something is moving, in world units per second.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Velocity {
    /// Horizontal component.
    pub x: f32,
    /// Vertical component.
    pub y: f32,
}

impl StableHash for Velocity {
    fn stable_hash(&self, hasher: &mut StableHasher) {
        hasher.write_f32(self.x);
        hasher.write_f32(self.y);
    }
}

impl Component for Velocity {}

/// Marks the entity the enemies react to.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Player;

impl StableHash for Player {
    fn stable_hash(&self, _hasher: &mut StableHasher) {}
}

impl Component for Player {}

// --- Tunables ---

/// The numbers a designer or an agent actually tweaks.
///
/// A [`Resource`] rather than constants, so every candidate reads them the same way and the
/// "change one number" edit is expressible in a script as well as in Rust.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AiConfig {
    /// Distance at which an enemy notices the player.
    pub sight_range: f32,
    /// Distance at which a pursuing enemy loses the player. Larger than `sight_range`, so the
    /// state does not chatter at the boundary.
    pub lose_range: f32,
    /// Movement speed while patrolling or searching, in units per second.
    pub patrol_speed: f32,
    /// Movement speed while pursuing, in units per second.
    pub pursue_speed: f32,
    /// How many ticks an enemy searches before giving up.
    pub search_ticks: u32,
    /// How close counts as having reached a patrol waypoint.
    pub waypoint_radius: f32,
    /// Half-width of the random offset applied to a last-seen position.
    pub search_jitter: f32,
}

impl Default for AiConfig {
    fn default() -> Self {
        Self {
            sight_range: 3.5,
            lose_range: 5.0,
            patrol_speed: 1.5,
            pursue_speed: 3.0,
            search_ticks: 90,
            waypoint_radius: 0.25,
            search_jitter: 0.5,
        }
    }
}

impl StableHash for AiConfig {
    fn stable_hash(&self, hasher: &mut StableHasher) {
        hasher.write_f32(self.sight_range);
        hasher.write_f32(self.lose_range);
        hasher.write_f32(self.patrol_speed);
        hasher.write_f32(self.pursue_speed);
        hasher.write_u32(self.search_ticks);
        hasher.write_f32(self.waypoint_radius);
        hasher.write_f32(self.search_jitter);
    }
}

impl Resource for AiConfig {}

// --- Shared behaviour helpers ---
//
// These live here rather than in each candidate so that the parts which are *not* under test are
// provably identical. Only the enemy AI itself differs between candidates.

/// The four patrol waypoints around a home position, in visit order.
///
/// Fixed offsets rather than random placement: patrol routes must be reproducible, and a random
/// route would consume RNG draws that the state hash comparison depends on being identical.
#[must_use]
pub fn waypoint(home: [f32; 2], index: u32) -> [f32; 2] {
    const RADIUS: f32 = 2.0;
    match index % 4 {
        0 => [home[0] + RADIUS, home[1]],
        1 => [home[0], home[1] + RADIUS],
        2 => [home[0] - RADIUS, home[1]],
        _ => [home[0], home[1] - RADIUS],
    }
}

/// A unit vector from `from` toward `to`, scaled by `speed`.
///
/// Returns zero when the two points are within `1e-6`, which avoids dividing by a near-zero length
/// and producing an infinity that would poison the state hash.
#[must_use]
pub fn steer_toward(from: [f32; 2], to: [f32; 2], speed: f32) -> Velocity {
    let dx = to[0] - from[0];
    let dy = to[1] - from[1];
    let length = (dx * dx + dy * dy).sqrt();
    if length < 1e-6 {
        return Velocity { x: 0.0, y: 0.0 };
    }
    Velocity {
        x: dx / length * speed,
        y: dy / length * speed,
    }
}

/// Where the player is on tick `tick`.
///
/// A rectangular circuit walked at constant speed, 480 ticks (8 seconds) per lap. Piecewise linear
/// on purpose — see the module docs on why there are no transcendentals in this benchmark.
#[must_use]
pub fn player_position(tick: u64) -> [f32; 2] {
    const LAP_TICKS: u64 = 480;
    const LEG_TICKS: u64 = LAP_TICKS / 4;
    const CORNERS: [[f32; 2]; 4] = [[-6.0, -4.0], [6.0, -4.0], [6.0, 4.0], [-6.0, 4.0]];

    let position_in_lap = tick % LAP_TICKS;
    let leg = (position_in_lap / LEG_TICKS) as usize;
    let fraction = (position_in_lap % LEG_TICKS) as f32 / LEG_TICKS as f32;

    let start = CORNERS[leg];
    let end = CORNERS[(leg + 1) % 4];
    [
        start[0] + (end[0] - start[0]) * fraction,
        start[1] + (end[1] - start[1]) * fraction,
    ]
}

/// Moves the player along its scripted path. Identical in every candidate.
pub fn drive_player(world: &mut World) {
    let target = player_position(world.tick().0);
    world.for_each_pair_mut::<Transform2d, Player>(|_entity, transform, _player| {
        transform.position = target;
    });
}

/// Applies velocity to position. Identical in every candidate.
pub fn integrate(world: &mut World) {
    world.for_each_pair_mut::<Transform2d, Velocity>(|_entity, transform, velocity| {
        transform.position[0] += velocity.x * FIXED_DT;
        transform.position[1] += velocity.y * FIXED_DT;
    });
}

/// Finds the player's current position, if a player exists.
///
/// Every AI implementation needs this before it can do anything, so it is shared rather than
/// reimplemented three times.
#[must_use]
pub fn find_player(world: &World) -> Option<[f32; 2]> {
    world
        .iter_pair::<Transform2d, Player>()
        .map(|(_entity, transform, _player)| transform.position)
        .next()
}

/// Collects every enemy's entity handle, state, and position.
///
/// Returned as a plain `Vec` because a scripting or WASM boundary cannot hold a borrow of the
/// world across a call. The Rust candidates do not need this, and the fact that the others do is
/// itself one of the spike's findings.
#[must_use]
pub fn collect_enemies(world: &World) -> Vec<(Entity, Enemy, [f32; 2])> {
    world
        .iter_pair::<Enemy, Transform2d>()
        .map(|(entity, enemy, transform)| (entity, *enemy, transform.position))
        .collect()
}

// --- World construction ---

/// Builds the benchmark app: one player, [`ENEMY_COUNT`] enemies, and everything except the AI.
///
/// The caller adds the enemy AI system under the [`ENEMY_AI`] label. Systems are ordered
/// `drive_player -> enemy_ai -> integrate`, so the AI always sees the player's position for the
/// current tick and its velocity decision is applied within the same tick.
#[must_use]
pub fn build_app(seed: u64) -> App {
    let mut app = App::with_seed(seed);
    app.insert_resource(AiConfig::default());

    app.add_system(Stage::Simulation, system(DRIVE_PLAYER, drive_player));
    app.add_system(
        Stage::Simulation,
        system(INTEGRATE, integrate).after(ENEMY_AI),
    );

    let player = app.world.spawn();
    app.world.insert(player, Transform2d::at(-6.0, -4.0));
    app.world.insert(player, Player);

    // An 8x8 grid of enemies spread across the player's circuit, so a useful fraction of them
    // transition between states as it passes. Positions are computed, never random: the RNG budget
    // belongs to the behaviour under test, and spending draws here would shift every later value.
    for index in 0..ENEMY_COUNT {
        let column = (index % 8) as f32;
        let row = (index / 8) as f32;
        let home = [-7.0 + column * 2.0, -5.25 + row * 1.5];

        let enemy = app.world.spawn();
        app.world.insert(enemy, Transform2d::at(home[0], home[1]));
        app.world.insert(enemy, Velocity { x: 0.0, y: 0.0 });
        app.world.insert(
            enemy,
            Enemy {
                state: EnemyState::Patrol,
                timer: 0,
                home,
                last_seen: home,
                waypoint: 0,
            },
        );
    }

    app
}

/// Registers the AI system that other candidates supply as a boxed closure.
///
/// Wraps the ordering constraint in one place so no candidate can accidentally schedule itself
/// differently and change the result for a reason unrelated to what is being measured.
pub fn install_ai(app: &mut App, run: impl FnMut(&mut World) + 'static) {
    app.add_system(
        Stage::Simulation,
        system(ENEMY_AI, run).after(DRIVE_PLAYER),
    );
}

/// Draws the search jitter for one enemy, in the order the specification requires.
///
/// Separated out because every candidate must consume exactly two `f32` draws, in x-then-y order,
/// at exactly the moment an enemy enters `Search`. Getting this wrong is the single easiest way to
/// make two candidates disagree, and the failure looks like a behaviour bug rather than an
/// ordering bug.
pub fn draw_jitter(rng: &mut SimRng, jitter: f32) -> [f32; 2] {
    let x = rng.0.range_f32(-jitter, jitter);
    let y = rng.0.range_f32(-jitter, jitter);
    [x, y]
}

// --- Reporting ---

/// What a completed run produced.
///
/// The state hash is the headline: two candidates that agree on it computed the same simulation,
/// which is what makes their latency numbers comparable rather than merely adjacent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Report {
    /// [`World::state_hash`] at the end of the run.
    pub state_hash: u64,
    /// The tick the run finished on.
    pub tick: u64,
    /// How many enemies ended in `Patrol`.
    pub patrolling: usize,
    /// How many enemies ended in `Pursue`.
    pub pursuing: usize,
    /// How many enemies ended in `Search`.
    pub searching: usize,
}

/// Summarises a finished app.
#[must_use]
pub fn report(app: &App) -> Report {
    let mut patrolling = 0;
    let mut pursuing = 0;
    let mut searching = 0;
    for (_entity, enemy) in app.world.iter::<Enemy>() {
        match enemy.state {
            EnemyState::Patrol => patrolling += 1,
            EnemyState::Pursue => pursuing += 1,
            EnemyState::Search => searching += 1,
        }
    }

    Report {
        state_hash: app.state_hash(),
        tick: app.tick().0,
        patrolling,
        pursuing,
        searching,
    }
}

impl std::fmt::Display for Report {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "tick {} | state_hash {:016x} | patrol {} pursue {} search {}",
            self.tick, self.state_hash, self.patrolling, self.pursuing, self.searching
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn player_path_is_a_closed_loop() {
        // Tick 0 and one full lap later must agree, or the scenario drifts over a long run.
        assert_eq!(player_position(0), player_position(480));
        assert_eq!(player_position(0), [-6.0, -4.0]);
        assert_eq!(player_position(120), [6.0, -4.0]);
        assert_eq!(player_position(240), [6.0, 4.0]);
        assert_eq!(player_position(360), [-6.0, 4.0]);
    }

    #[test]
    fn steer_toward_produces_a_vector_of_the_requested_speed() {
        let velocity = steer_toward([0.0, 0.0], [3.0, 4.0], 10.0);
        let length = (velocity.x * velocity.x + velocity.y * velocity.y).sqrt();
        assert!((length - 10.0).abs() < 1e-5, "length was {length}");
        // 3-4-5 triangle, so the direction is exactly checkable.
        assert!((velocity.x - 6.0).abs() < 1e-5);
        assert!((velocity.y - 8.0).abs() < 1e-5);
    }

    #[test]
    fn steer_toward_at_the_target_is_zero_not_infinity() {
        let velocity = steer_toward([1.0, 1.0], [1.0, 1.0], 5.0);
        assert_eq!(velocity, Velocity { x: 0.0, y: 0.0 });
    }

    #[test]
    fn waypoints_cycle_through_four_positions() {
        let home = [0.0, 0.0];
        assert_eq!(waypoint(home, 0), [2.0, 0.0]);
        assert_eq!(waypoint(home, 1), [0.0, 2.0]);
        assert_eq!(waypoint(home, 2), [-2.0, 0.0]);
        assert_eq!(waypoint(home, 3), [0.0, -2.0]);
        // Wraps rather than panicking, so a script writing 7 is not a crash.
        assert_eq!(waypoint(home, 4), waypoint(home, 0));
    }

    #[test]
    fn enemy_state_round_trips_through_its_wire_form() {
        for state in [EnemyState::Patrol, EnemyState::Pursue, EnemyState::Search] {
            assert_eq!(EnemyState::from_u32(state.as_u32()), state);
        }
        // Garbage degrades to Patrol rather than panicking.
        assert_eq!(EnemyState::from_u32(99), EnemyState::Patrol);
    }

    #[test]
    fn the_world_is_built_with_the_expected_population() {
        let app = build_app(1234);
        assert_eq!(app.world.iter::<Enemy>().count(), ENEMY_COUNT);
        assert_eq!(app.world.iter::<Player>().count(), 1);
        assert_eq!(find_player(&app.world), Some([-6.0, -4.0]));
    }
}
