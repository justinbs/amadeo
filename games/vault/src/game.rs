//! The Vault: components, resources, and the systems that make it a game.
//!
//! Everything here is *game* knowledge — what a warden is, what collecting something means, what
//! winning is. Invariant I4 keeps all of it out of the engine, which is why this file exists at all
//! rather than the engine growing a `Score`.
//!
//! # The rules
//!
//! You are the amber figure. Six sigils are scattered through a walled vault; collect all of them to
//! win. Two wardens patrol fixed routes and two floor traps sit in the middle corridors; touch
//! either and you lose. Walls stop you, and stop nothing else — the wardens' routes are authored to stay clear of them, which is a deliberate simplicity
//! rather than an oversight (see [`patrol_wardens`]).
//!
//! # Everything here is deterministic
//!
//! No wall-clock, no randomness, no iteration over anything unordered. Movement integrates against
//! `FIXED_DT` rather than a measured frame time, so the same inputs produce the same game on any
//! machine at any frame rate (invariant I3). That is what lets `replays/collect-three.replay` assert
//! this game's exact state four times over.

use amadeo_core::{FIXED_DT, StableHash};
use amadeo_ecs::{Commands, Component, Entity, Resource, World};
use amadeo_input::{ActionId, InputState};
use amadeo_reflect::Reflect;
use amadeo_render::{Quad, Sprite};
use amadeo_transform::Transform;

// --- Tuning. Collected here so the game can be re-balanced without reading any of the code. ---

/// World units per second the player moves at full deflection.
pub const PLAYER_SPEED: f32 = 4.5;

/// World units per second a warden patrols at.
pub const WARDEN_SPEED: f32 = 2.2;

/// Half the player's collision box. Smaller than the sprite, so corners feel generous rather than
/// sticky — the oldest trick in tile-based movement.
const PLAYER_HALF: f32 = 0.34;

/// How close a sigil has to be, centre to centre, to be collected.
const PICKUP_RANGE: f32 = 0.55;

/// How close a warden has to be to catch you. Deliberately smaller than [`PICKUP_RANGE`] plus the
/// two half-widths would suggest: being caught should feel like contact, not like proximity.
const CATCH_RANGE: f32 = 0.62;

/// How close to a waypoint counts as reaching it.
///
/// Must be comfortably larger than one tick of movement (`WARDEN_SPEED * FIXED_DT`, about 0.037
/// units) or a warden can step past a waypoint and never register arriving, then circle forever.
const WAYPOINT_RANGE: f32 = 0.08;

/// Points per sigil. Ten rather than one so the score fills two digits, which is what the sprite
/// sheet draws — and is what every arcade game did for the same reason.
pub const POINTS_PER_SIGIL: u32 = 10;

// --- Components ---

/// Marks the entity the player steers.
#[derive(Debug, Clone, Copy, PartialEq, StableHash, Reflect)]
pub struct Player;
impl Component for Player {}

/// A patrolling warden. Touching one ends the run.
#[derive(Debug, Clone, Copy, PartialEq, StableHash, Reflect)]
pub struct Warden {
    /// World units per second it travels along its route.
    #[reflect(min = 0.0, max = 20.0, unit = "world units/s")]
    pub speed: f32,
}
impl Component for Warden {}

/// A closed route, walked in order and then repeated.
#[derive(Debug, Clone, PartialEq, StableHash, Reflect)]
pub struct Patrol {
    /// The corners to walk between, in world units. Wraps from the last back to the first.
    pub points: Vec<[f32; 2]>,
    /// Which point is being walked towards. Simulation state, so it is hashed and snapshotted.
    pub next: u32,
}
impl Component for Patrol {}

/// One of the things you are here to collect.
#[derive(Debug, Clone, Copy, PartialEq, StableHash, Reflect)]
pub struct Sigil;
impl Component for Sigil {}

/// A solid tile. Blocks the player and nothing else.
#[derive(Debug, Clone, Copy, PartialEq, StableHash, Reflect)]
pub struct Wall;
impl Component for Wall {}

/// One digit of the score readout.
#[derive(Debug, Clone, Copy, PartialEq, StableHash, Reflect)]
pub struct ScoreDigit {
    /// Which column, counting from the left. 0 is the tens digit, 1 the units.
    #[reflect(min = 0.0, max = 8.0)]
    pub place: u32,
}
impl Component for ScoreDigit {}

/// The arena floor, so the outcome can recolour it.
#[derive(Debug, Clone, Copy, PartialEq, StableHash, Reflect)]
pub struct Floor;
impl Component for Floor {}

/// A floor plate that ends the run when stepped on.
///
/// # This component is M1 exit gate 4
///
/// The gate asks whether `amadeo describe` output is "sufficient to write a new component and system
/// without reading engine source", tested by doing it. `Trap` and [`spring_traps`] are that test, and
/// `docs/09-gate-4-describe-is-not-enough.md` records what it found.
///
/// The short version: `describe` told me exactly what a component's *data* looks like — field names,
/// types, units, ranges, and what each one means — and nothing about how to declare one. The derive
/// list on the line above, the `impl Component` below it, and the registration call in `lib.rs` are
/// all things no `describe` output mentions.
#[derive(Debug, Clone, Copy, PartialEq, StableHash, Reflect)]
pub struct Trap {
    /// Whether it will still fire. A sprung trap stays visible but is spent.
    ///
    /// Authored `true`; there is no way back to armed within a run, which is what makes a trap a
    /// mistake you make once rather than a hazard you learn to time.
    pub armed: bool,
}
impl Component for Trap {}

// --- Resources ---

/// How the run is going.
///
/// An enum rather than two booleans, because "won and lost at the same time" should be
/// unrepresentable rather than merely unlikely.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, StableHash, Reflect)]
pub enum Phase {
    /// Still going.
    #[default]
    Playing,
    /// Every sigil collected.
    Won,
    /// A warden made contact.
    Lost,
}

/// The run's outcome, and the score.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, StableHash, Reflect)]
pub struct Run {
    /// Playing, won, or lost.
    pub phase: Phase,
    /// How many sigils have been collected.
    pub collected: u32,
    /// How many there were to begin with. Counted at startup rather than hard-coded, so editing the
    /// scene changes the win condition with it.
    pub total: u32,
}
impl Resource for Run {}

impl Run {
    /// The score as it is displayed.
    #[must_use]
    pub fn score(&self) -> u32 {
        self.collected * POINTS_PER_SIGIL
    }
}

// --- Action names. Constants rather than literals, so a typo is a compile error. ---

/// Horizontal movement axis.
pub const MOVE_X: &str = "move_x";
/// Vertical movement axis.
pub const MOVE_Y: &str = "move_y";

// --- Systems ---

/// The label each system is registered under. Grouped so the schedule reads in one place.
pub mod labels {
    /// Reads input and moves the player.
    pub const STEER_PLAYER: &str = "steer_player";
    /// Walks wardens along their routes.
    pub const PATROL_WARDENS: &str = "patrol_wardens";
    /// Picks up sigils the player is standing on.
    pub const COLLECT_SIGILS: &str = "collect_sigils";
    /// Springs a trap the player stepped on.
    pub const SPRING_TRAPS: &str = "spring_traps";
    /// Decides whether the run has ended.
    pub const RESOLVE_OUTCOME: &str = "resolve_outcome";
    /// Points the score digits at the right glyphs.
    pub const SHOW_SCORE: &str = "show_score";
}

/// Moves the player by its input, stopping at walls.
///
/// # Why the two axes are resolved separately
///
/// Moving on both axes and then pushing out of a wall gives the classic bug where running diagonally
/// into a corner shoves you sideways through it. Resolving x, then resolving y, means each axis is
/// blocked only by what is actually in the way along it — so sliding along a wall works, and
/// corners hold.
pub fn steer_player(world: &mut World) {
    if !matches!(phase(world), Phase::Playing) {
        return;
    }

    let Some(input) = world.resource::<InputState>() else {
        return;
    };
    let step = [
        input.axis(ActionId::new(MOVE_X)) * PLAYER_SPEED * FIXED_DT,
        input.axis(ActionId::new(MOVE_Y)) * PLAYER_SPEED * FIXED_DT,
    ];
    if step[0] == 0.0 && step[1] == 0.0 {
        return;
    }

    // Collected first, because resolving a move needs to read every wall while writing the player —
    // and one mutable borrow of the world cannot do both.
    let walls = wall_centres(world);
    let Some((player, mut position)) = player_position(world) else {
        return;
    };

    for axis in 0..2 {
        if step[axis] == 0.0 {
            continue;
        }
        let mut moved = position;
        moved[axis] += step[axis];
        if !hits_a_wall(moved, &walls) {
            position = moved;
        }
    }

    if let Some(transform) = world.get_mut::<Transform>(player) {
        transform.translation[0] = position[0];
        transform.translation[1] = position[1];
    }
}

/// Walks each warden towards its next waypoint, wrapping at the end of the route.
///
/// # Wardens do not collide with walls, on purpose
///
/// Their routes are authored to stay in open corridors, so collision would only ever be a no-op that
/// cost work every tick. Pathfinding is a `mod-behaviour` concern (M3) and putting a shortest-path
/// search in an M1 exit-gate game would be building the wrong thing early.
///
/// The cost is real and worth naming: **edit a route in the scene file so it crosses a pillar, and
/// the warden walks through it.** `a_patrol_route_stays_clear_of_walls` is a test rather than a
/// runtime check, because this is an authoring mistake and the right time to catch one is before the
/// game runs.
pub fn patrol_wardens(world: &mut World) {
    if !matches!(phase(world), Phase::Playing) {
        return;
    }

    world.for_each_triple_mut::<Transform, Patrol, Warden>(|_entity, transform, patrol, warden| {
        if patrol.points.is_empty() {
            return;
        }

        let index = patrol.next as usize % patrol.points.len();
        let target = patrol.points[index];
        let to_target = [
            target[0] - transform.translation[0],
            target[1] - transform.translation[1],
        ];
        let distance = to_target[0].hypot(to_target[1]);

        if distance <= WAYPOINT_RANGE {
            // Snapped rather than left slightly short, so a route cannot accumulate drift over
            // thousands of laps — which would eventually put a warden inside a wall.
            transform.translation[0] = target[0];
            transform.translation[1] = target[1];
            patrol.next = ((index + 1) % patrol.points.len()) as u32;
            return;
        }

        let step = warden.speed * FIXED_DT;
        transform.translation[0] += to_target[0] / distance * step;
        transform.translation[1] += to_target[1] / distance * step;
    });
}

/// Collects any sigil the player is standing on.
pub fn collect_sigils(world: &mut World) {
    if !matches!(phase(world), Phase::Playing) {
        return;
    }
    let Some((_, position)) = player_position(world) else {
        return;
    };

    let taken: Vec<Entity> = world
        .query::<(&Transform, &Sigil)>()
        .filter(|(_, (transform, _))| within(position, transform, PICKUP_RANGE))
        .map(|(entity, _)| entity)
        .collect();

    if taken.is_empty() {
        return;
    }

    // Deferred, because despawning while a query is open would invalidate it. The app flushes after
    // every stage, so the sigil is gone before anything looks again.
    world.with_service_taken::<Commands, ()>(|_world, commands| {
        for entity in &taken {
            commands.despawn(*entity);
        }
    });

    if let Some(run) = world.resource_mut::<Run>() {
        run.collected += taken.len() as u32;
    }
}

/// Springs any armed trap the player is standing on, and ends the run.
///
/// Runs before [`resolve_outcome`], so a trap and the last sigil on the same tick is still a loss —
/// the same reading as walking into a warden, and for the same reason: you did not get away with it.
///
/// A sprung trap is marked rather than despawned, so the arena still shows where it was. That is
/// information a player wants after a loss and costs nothing to keep.
pub fn spring_traps(world: &mut World) {
    if !matches!(phase(world), Phase::Playing) {
        return;
    }
    let Some((_, position)) = player_position(world) else {
        return;
    };

    let sprung: Vec<Entity> = world
        .query::<(&Transform, &Trap)>()
        .filter(|(_, (transform, trap))| trap.armed && within(position, transform, TRAP_RANGE))
        .map(|(entity, _)| entity)
        .collect();

    if sprung.is_empty() {
        return;
    }

    for entity in sprung {
        if let Some(trap) = world.get_mut::<Trap>(entity) {
            trap.armed = false;
        }
    }
    if let Some(run) = world.resource_mut::<Run>() {
        run.phase = Phase::Lost;
    }
}

/// How close the player's centre has to be to a trap's centre to spring it.
///
/// Tighter than a warden's reach: a trap is a tile you stand *on*, so clipping its corner should not
/// count. Wider than nothing, so the plate does not feel like a pixel hunt.
const TRAP_RANGE: f32 = 0.45;

/// Ends the run when every sigil is collected, or when a warden makes contact.
///
/// **Losing is checked first.** Walking into the last sigil and a warden on the same tick is a loss,
/// which is the reading a player expects — you did not get away with it.
pub fn resolve_outcome(world: &mut World) {
    if !matches!(phase(world), Phase::Playing) {
        return;
    }
    let Some((_, position)) = player_position(world) else {
        return;
    };

    let caught = world
        .query::<(&Transform, &Warden)>()
        .any(|(_, (transform, _))| within(position, transform, CATCH_RANGE));

    let Some(run) = world.resource_mut::<Run>() else {
        return;
    };
    if caught {
        run.phase = Phase::Lost;
    } else if run.total > 0 && run.collected >= run.total {
        run.phase = Phase::Won;
    }

    // The floor carries the outcome, because M1 has no text and no UI. Reading the result off the
    // colour of the room is crude and completely unambiguous, which is the right trade here.
    let colour = match phase(world) {
        Phase::Playing => FLOOR_PLAYING,
        Phase::Won => FLOOR_WON,
        Phase::Lost => FLOOR_LOST,
    };
    world.for_each_pair_mut::<Quad, Floor>(|_entity, quad, _floor| {
        quad.color = colour;
    });
}

/// The arena floor while the run is going: a cold near-black.
const FLOOR_PLAYING: [f32; 4] = [0.055, 0.062, 0.098, 1.0];
/// On a win, a muted green.
const FLOOR_WON: [f32; 4] = [0.075, 0.145, 0.106, 1.0];
/// On a loss, a muted red.
const FLOOR_LOST: [f32; 4] = [0.165, 0.055, 0.070, 1.0];

/// Points each score digit at the glyph it should show.
///
/// The digit sheet is ten 8x8 cells stacked vertically, so digit `d` is the region
/// `[0, d/10, 1, 1/10]`. There is no text rendering until M3, and this is how every game drew a
/// number before there was.
pub fn show_score(world: &mut World) {
    let score = world.resource::<Run>().map_or(0, Run::score);

    world.for_each_pair_mut::<Sprite, ScoreDigit>(|_entity, sprite, digit| {
        // Place 0 is the leftmost column, so it is the most significant digit.
        let divisor = 10u32.pow(digit.place.min(8));
        let value = (score / divisor.max(1)) % 10;
        sprite.region = [0.0, value as f32 / 10.0, 1.0, 0.1];
    });
}

// --- Shared helpers ---

/// The run's current phase, or `Playing` when there is no `Run` resource at all.
fn phase(world: &World) -> Phase {
    world
        .resource::<Run>()
        .map_or(Phase::Playing, |run| run.phase)
}

/// The player's entity and world position.
fn player_position(world: &World) -> Option<(Entity, [f32; 2])> {
    world
        .query::<(&Transform, &Player)>()
        .next()
        .map(|(entity, (transform, _))| {
            (entity, [transform.translation[0], transform.translation[1]])
        })
}

/// Every wall's centre.
pub fn wall_centres(world: &World) -> Vec<[f32; 2]> {
    world
        .query::<(&Transform, &Wall)>()
        .map(|(_, (transform, _))| [transform.translation[0], transform.translation[1]])
        .collect()
}

/// Whether a player box centred here overlaps any wall tile.
///
/// Walls are exactly one world unit square, so their half-extent is a constant rather than a
/// component — a `Solid` component would be the general answer and this game has one shape.
fn hits_a_wall(position: [f32; 2], walls: &[[f32; 2]]) -> bool {
    const WALL_HALF: f32 = 0.5;
    let reach = WALL_HALF + PLAYER_HALF;
    walls
        .iter()
        .any(|wall| (position[0] - wall[0]).abs() < reach && (position[1] - wall[1]).abs() < reach)
}

/// Whether a point is within `range` of a transform's position.
fn within(position: [f32; 2], transform: &Transform, range: f32) -> bool {
    let dx = position[0] - transform.translation[0];
    let dy = position[1] - transform.translation[1];
    dx.hypot(dy) <= range
}

/// The tuning constants above have to hold two relationships, and both are checked here at **compile
/// time** rather than in a test.
///
/// A test would be the obvious place and is the worse one: these are constants, so the answer cannot
/// depend on anything a test could set up, and a compile error arrives the moment someone edits the
/// number rather than the next time the suite happens to run.
const _: () = {
    // A player box smaller than a tile is what makes corners generous rather than sticky.
    assert!(PLAYER_HALF < 0.5);

    // A warden must not be able to step over a waypoint without registering arrival, or it sails
    // past and circles its route forever -- a hang rather than a glitch.
    assert!(WAYPOINT_RANGE > WARDEN_SPEED * FIXED_DT * 2.0);

    // Being caught should feel like contact rather than proximity, so it must be the tighter of the
    // two ranges. Swapping them would make the wardens feel unfair without anything looking wrong.
    assert!(CATCH_RANGE < PICKUP_RANGE + PLAYER_HALF);
};
