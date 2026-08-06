//! A physics-driven character controller — ADR 0037.
//!
//! # Why this is a module and not a crate
//!
//! `CLAUDE.md` trap 10 is blunt about it: of the eight target games, one has no character at all and
//! three are 2D or isometric. An engine that assumes a game has someone to walk around as is an
//! engine that has quietly picked a genre, which invariant I4 forbids.
//!
//! So the split runs down the middle of the problem. **`amadeo-physics` owns the geometry** — a
//! [`ShapeMove`] asks "move this shape by this much and slide along what it hits", and knows nothing
//! about walking. **This module owns the character** — walk speed, jump, turning, and the input
//! actions that drive them. The same line Godot draws between `move_and_collide` and
//! `CharacterBody3D`.
//!
//! This is the first occupant of `modules/`, which `CLAUDE.md` section 4 has reserved since session
//! one. The rule that comes with the layer is invariant I6 one level up: a module may depend on
//! engine crates and on other modules, and **no engine crate may ever depend on a module**.
//!
//! # It is deterministic, like everything else in the simulation
//!
//! Movement integrates against [`FIXED_DT`] rather than a measured frame time, and both components
//! here are reflected and **hashed** — so a character-driven game is snapshot-able and replayable
//! with nothing extra built, exactly as ADR 0036 arranged for rigid bodies.
//!
//! # Worked example
//!
//! ```
//! use amadeo_app::App;
//! use amadeo_character::{CharacterController, CharacterMotion};
//! use amadeo_ecs::World;
//! use amadeo_physics::{Collider, Gravity, NullPhysics, Physics, RigidBody};
//! use amadeo_transform::Transform;
//!
//! let mut app = App::new();
//! app.world.insert_service(Physics::new(Box::new(NullPhysics::new())));
//! app.world.insert_resource(Gravity::earth());
//!
//! // Everything a character needs: where it is, that physics knows about it, what shape it is,
//! // how it moves, and where it is up to.
//! let player = app.world.spawn();
//! app.world.insert(player, Transform::at_xyz(0.0, 1.0, 0.0));
//! app.world.insert(player, RigidBody::kinematic());
//! app.world.insert(player, Collider::capsule(0.4, 1.2));
//! app.world.insert(player, CharacterController::default());
//! app.world.insert(player, CharacterMotion::default());
//!
//! // Registers the component types, physics, and this module's system, in the order ADR 0037
//! // requires.
//! amadeo_character::install(&mut app).expect("nothing else claims these names");
//! ```

use amadeo_app::{App, Stage, system};
use amadeo_core::{FIXED_DT, StableHash};
use amadeo_ecs::{Component, Entity, World};
use amadeo_input::{ActionId, InputState};
use amadeo_physics::{Collider, Gravity, Physics, STEP_PHYSICS, Shape, ShapeMove, step_physics};
use amadeo_reflect::{Reflect, RegistryError};
use amadeo_transform::Transform;

/// The label [`drive_characters`] is registered under.
pub const DRIVE_CHARACTERS: &str = "drive_characters";

// --- The actions a character reads ---
//
// Named strings rather than a field on the component, deliberately. `ActionId` is the hash of a
// name (and Q18 notes that a reflected one is a number nobody can read), so putting them in a
// component would make a scene file carry unreadable integers to no benefit. A game that wants
// different bindings changes what its input map points these names at, which is the layer where
// rebinding belongs.

/// Axis action: forward is `+1.0`, backward is `-1.0`, relative to where the character faces.
pub const MOVE_FORWARD: &str = "move_forward";
/// Axis action: right is `+1.0`, left is `-1.0`, relative to where the character faces.
pub const MOVE_RIGHT: &str = "move_right";
/// Axis action: turns the character about its up axis. `+1.0` turns left.
pub const TURN: &str = "turn";
/// Button action: jumps, if the character is standing on something.
pub const JUMP: &str = "jump";

/// How a character moves. Authored, and never written by this module.
///
/// The runtime half — where it is up to — is [`CharacterMotion`]. Splitting them means a scene file
/// carries only the tuning, and a diff of a saved game shows only what actually changed.
#[derive(Debug, Clone, Copy, PartialEq, StableHash, Reflect)]
pub struct CharacterController {
    /// Top speed on the ground, in world units per second.
    #[reflect(min = 0.0, max = 1000.0, unit = "units/s")]
    pub speed: f32,
    /// How quickly top speed is reached, in world units per second squared.
    ///
    /// Very large values give instant, twitchy, arcade movement; small ones feel like ice. This is
    /// the single field with the most effect on how a game feels to play.
    #[reflect(min = 0.0, max = 10000.0, unit = "units/s^2")]
    pub acceleration: f32,
    /// Turn rate, in degrees per second, at full deflection of [`TURN`].
    #[reflect(min = 0.0, max = 3600.0, unit = "deg/s")]
    pub turn_speed: f32,
    /// Upward speed applied the instant a jump starts, in world units per second.
    ///
    /// Height follows from this and gravity rather than being set directly, because a jump that
    /// specified its height would have to fight the solver to guarantee it.
    #[reflect(min = 0.0, max = 1000.0, unit = "units/s")]
    pub jump_speed: f32,
    /// The steepest floor that can be walked up, in degrees. Steeper counts as a wall.
    #[reflect(min = 0.0, max = 90.0, unit = "deg")]
    pub max_slope_degrees: f32,
    /// The tallest obstacle to step over automatically, in world units. `0.0` disables it.
    ///
    /// What makes stairs work, and the expensive one — it costs extra shape casts every tick, so it
    /// is off unless a level has steps.
    #[reflect(min = 0.0, max = 100.0, unit = "world units")]
    pub step_height: f32,
    /// How far below the feet to look for ground to stay stuck to, in world units.
    ///
    /// Without it, walking down a ramp launches the character off every small bump — which reads as
    /// the character being bouncy rather than as a missing feature.
    #[reflect(min = 0.0, max = 100.0, unit = "world units")]
    pub snap_distance: f32,
    /// A small gap kept between the character and the world, in world units.
    ///
    /// Must not be zero: shape casting against a surface being exactly touched is numerically
    /// unstable, and the symptom is a character that intermittently sticks to or sinks into walls.
    #[reflect(min = 0.0001, max = 1.0, unit = "world units")]
    pub skin: f32,
}

impl Default for CharacterController {
    fn default() -> Self {
        Self {
            // A brisk walk. Real human walking is about 1.4 units/s and reads as painfully slow in a
            // game, which is the same reason `Gravity::earth` warns that nobody uses real gravity.
            speed: 5.0,
            acceleration: 40.0,
            turn_speed: 180.0,
            jump_speed: 5.0,
            max_slope_degrees: 45.0,
            step_height: 0.0,
            snap_distance: 0.1,
            skin: 0.01,
        }
    }
}

impl Component for CharacterController {}

/// Where a character is up to. Computed every tick, and **in the state hash**.
///
/// Hashed rather than derived — unlike `GlobalTransform`, which ADR 0019 keeps out of the hash — for
/// a concrete reason: two worlds whose characters are falling at different speeds have genuinely
/// diverged, and a snapshot that restored position without velocity would resume mid-air at a
/// standstill.
#[derive(Debug, Clone, Copy, PartialEq, Default, StableHash, Reflect)]
pub struct CharacterMotion {
    /// Current velocity in world units per second.
    #[reflect(unit = "units/s")]
    pub velocity: [f32; 3],
    /// Whether the character is standing on something walkable.
    ///
    /// What a jump is allowed by and a fall is detected with. Recomputed after every move, so a
    /// character that walked off a ledge this tick reports `false` this tick.
    pub grounded: bool,
}

impl Component for CharacterMotion {}

/// Registers this module's components and systems, in the order ADR 0037 requires.
///
/// # The ordering is the reason this helper exists
///
/// [`drive_characters`] must run **after** [`step_physics`], because it queries a spatial index the
/// step builds. Registered the other way round it would query an *empty* index on tick 1 and the
/// character would pass through the level exactly once — a bug that appears for one tick, at
/// startup, and never again.
///
/// A rule that can be forgotten silently is a rule that will be, so the ordering lives here rather
/// than in each game's setup. Registering [`step_physics`] too is part of the same argument: a
/// character module that let a game forget physics entirely would not be doing its job.
///
/// # And it registers the component types
///
/// `CLAUDE.md` trap 5 is "skipping reflection registration: ships fine, then the editor and the
/// agent cannot see the type, and you find out three milestones later." A module that ships
/// components registers them, for the same reason session 9 concluded that a game shipping an asset
/// registers the type it holds — otherwise `describe` and `amadeo check` disagree with what actually
/// loads.
///
/// **Call this before loading a scene that authors either component.** Registration is what lets a
/// scene file name a type, so `install` after `load_scene` fails with *no component named
/// `CharacterController` is registered*. `games/atrium` was written the wrong way round the first
/// time; the error names the module, which is what made it a two-minute problem rather than a
/// mystery.
///
/// # Errors
///
/// [`RegistryError`] if a game has already registered a different type under one of these names.
pub fn install(app: &mut App) -> Result<(), RegistryError> {
    app.register_component::<CharacterController>()?;
    app.register_component::<CharacterMotion>()?;

    app.add_system(Stage::Simulation, system(STEP_PHYSICS, step_physics));
    app.add_system(
        Stage::Simulation,
        system(DRIVE_CHARACTERS, drive_characters).after(STEP_PHYSICS),
    );
    Ok(())
}

// --- Small vector helpers ---
//
// Written out rather than pulled from a math crate because `amadeo-math` does not exist yet and
// three-element arrays are what every component here already speaks. Each is one line and named
// after what it does, which is cheaper to read than a generic abstraction would be.

fn add(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn scale(v: [f32; 3], k: f32) -> [f32; 3] {
    [v[0] * k, v[1] * k, v[2] * k]
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn length(v: [f32; 3]) -> f32 {
    dot(v, v).sqrt()
}

/// Normalises, or returns `fallback` if the vector is too short to have a direction.
///
/// The guard matters: normalising a zero vector gives NaN, and one NaN in a transform spreads to
/// every value it touches and survives every comparison, which makes it very hard to trace back.
fn normalise_or(v: [f32; 3], fallback: [f32; 3]) -> [f32; 3] {
    let len = length(v);
    if len > 1e-6 {
        scale(v, 1.0 / len)
    } else {
        fallback
    }
}

/// Everything one character needs, read out of the world before anything is written back.
///
/// Collected first for the borrow checker's sake: the move needs the `Physics` service *and* the
/// world, and taking both at once is what `with_service_taken` exists for.
struct Character {
    entity: Entity,
    controller: CharacterController,
    motion: CharacterMotion,
    transform: Transform,
    shape: Shape,
}

/// Reads input, moves every character, and writes back where each ended up.
///
/// Registered by [`install`] in `Stage::Simulation`, after [`step_physics`]. Does nothing if there
/// is no [`Physics`] service, so a world without physics is untouched rather than half-driven.
pub fn drive_characters(world: &mut World) {
    if !world.has_service::<Physics>() {
        return;
    }

    // Read the actions once rather than per character. Every character reads the same input today;
    // splitting input per player is a multiplayer question (ADR 0006) and answering it now would be
    // guessing at the shape of something M6 has not started.
    let (forward_input, right_input, turn_input, jumped) = match world.resource::<InputState>() {
        Some(input) => (
            input.axis(ActionId::new(MOVE_FORWARD)),
            input.axis(ActionId::new(MOVE_RIGHT)),
            input.axis(ActionId::new(TURN)),
            input.just_pressed(ActionId::new(JUMP)),
        ),
        None => (0.0, 0.0, 0.0, false),
    };

    let gravity = world
        .resource::<Gravity>()
        .copied()
        .unwrap_or_else(Gravity::none)
        .acceleration;

    // Up is derived from gravity rather than being another field to keep in step. For a walker they
    // are the same fact stated twice, and a game with rotated or flipped gravity gets a correct
    // controller for nothing. A world with no gravity falls back to +Y, which is harmless: nothing
    // is pulled anywhere, so "which way is up" only decides what counts as a floor.
    let up = normalise_or(scale(gravity, -1.0), [0.0, 1.0, 0.0]);

    let characters: Vec<Character> = world
        .query::<(
            &CharacterController,
            &CharacterMotion,
            &Transform,
            &Collider,
        )>()
        .map(
            |(entity, (controller, motion, transform, collider))| Character {
                entity,
                controller: *controller,
                motion: *motion,
                transform: *transform,
                // The collision shape comes from the `Collider` the entity already has rather than
                // from a size on the controller. Two places describing one capsule would drift, and
                // the one that physics reads should win.
                shape: collider.shape,
            },
        )
        .collect();

    if characters.is_empty() {
        return;
    }

    world.with_service_taken::<Physics, ()>(|world, physics| {
        for character in &characters {
            let controller = character.controller;
            let mut transform = character.transform;

            // Turning first, so this tick's movement uses this tick's facing. The other order lags
            // the character's direction one tick behind its body, which feels like input delay.
            transform.rotation[1] += turn_input * controller.turn_speed * FIXED_DT;
            let yaw = transform.rotation[1].to_radians();

            // At yaw zero the character faces -Z and its right hand points +X, which is the
            // right-handed Y-up convention the projection maths in `amadeo-transform` already uses.
            let facing = [-yaw.sin(), 0.0, -yaw.cos()];
            let rightward = [yaw.cos(), 0.0, -yaw.sin()];

            // Clamped to unit length so holding forward and right is not faster than forward alone
            // -- the oldest bug in twin-stick movement.
            let wish = add(scale(facing, forward_input), scale(rightward, right_input));
            let wish = if length(wish) > 1.0 {
                normalise_or(wish, [0.0; 3])
            } else {
                wish
            };
            let desired = scale(wish, controller.speed);

            // Split the current velocity into the part along up and the part across it, so the same
            // code works whichever way gravity points.
            let vertical_speed = dot(character.motion.velocity, up);
            let horizontal = sub(character.motion.velocity, scale(up, vertical_speed));

            // Accelerate toward the desired velocity rather than snapping to it. Moving at most
            // `acceleration * dt` per tick is what gives the movement weight.
            let difference = sub(desired, horizontal);
            let step = controller.acceleration * FIXED_DT;
            let horizontal = if length(difference) <= step {
                desired
            } else {
                add(horizontal, scale(normalise_or(difference, [0.0; 3]), step))
            };

            let vertical_speed = if character.motion.grounded {
                if jumped {
                    controller.jump_speed
                } else {
                    // Exactly zero while standing, and this is load-bearing rather than lazy.
                    //
                    // The obvious alternative -- press gently downward so the character stays
                    // attached -- ratchets it into the floor. Ground detection holds the character
                    // a skin-width above the surface, so any downward motion larger than that skin
                    // moves it *through* the gap; the cast then starts from a touching position,
                    // which is degenerate, and it sinks again next tick. Measured: a 1 unit/s bias
                    // against a 0.01 skin sank a resting character 0.07 units per second, forever.
                    //
                    // Staying attached is `snap_distance`'s job, and it does it by pulling the
                    // character down to the surface *after* the move rather than by aiming below it.
                    0.0
                }
            } else {
                // Only the component along up accelerates: sideways gravity would already be in the
                // horizontal part, and applying it twice would make a character on a rotated-gravity
                // world drift.
                vertical_speed + dot(gravity, up) * FIXED_DT
            };

            let velocity = add(horizontal, scale(up, vertical_speed));

            let request = ShapeMove {
                rotation: transform.rotation,
                up,
                max_slope_degrees: controller.max_slope_degrees,
                step_height: controller.step_height,
                snap_distance: controller.snap_distance,
                skin: controller.skin,
                ..ShapeMove::new(
                    character.shape,
                    transform.translation,
                    scale(velocity, FIXED_DT),
                )
            }
            .ignoring(character.entity);

            let moved = physics.move_shape(&request);

            // Velocity comes back from what actually happened, not from what was asked for. Walking
            // into a wall must not keep building speed into it, and landing must stop the fall --
            // both fall out of this one line rather than needing to be special-cased.
            let travelled = sub(moved.translation, transform.translation);
            let mut velocity = scale(travelled, 1.0 / FIXED_DT);

            // Except while grounded, where ground snapping can pull the character down much further
            // than it moved under its own power. Keeping that as velocity would read as a lurch the
            // next time it left the ground.
            if moved.grounded {
                let vertical = dot(velocity, up);
                if vertical < 0.0 {
                    velocity = sub(velocity, scale(up, vertical));
                }
            }

            transform.translation = moved.translation;
            world.insert(character.entity, transform);
            world.insert(
                character.entity,
                CharacterMotion {
                    velocity,
                    grounded: moved.grounded,
                },
            );
        }
    });
}
