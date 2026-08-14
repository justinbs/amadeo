//! Moving a shape through the world and sliding along what it hits — ADR 0037.
//!
//! # Why this is not part of [`step`](crate::PhysicsBackend::step)
//!
//! `step` advances *every* body by one tick and hands back where they all ended up. It is the right
//! shape for a world full of crates falling over, and the wrong shape for one specific thing that
//! gameplay wants to move deliberately and then be told what happened.
//!
//! A `Kinematic` body handed to `step` goes exactly where gameplay said, through walls included —
//! that is what kinematic *means*. So a character needs a second question the solver can answer:
//! *"I want to move from here by this much; where do I actually end up, and am I standing on
//! anything?"* That question is this module.
//!
//! # What it is deliberately not
//!
//! It has no idea what a character is. There is no walk speed here, no jump, no coyote time — those
//! live in `modules/amadeo-character`, because invariant I4 keeps genre knowledge above the crate
//! layer and `CLAUDE.md` trap 10 says the engine must not assume a game *has* a character.
//!
//! What is here is a geometric mechanism, and it describes a lift, a moving platform, a projectile
//! and a camera that must not clip through a wall just as well as it describes someone walking.

use crate::components::Shape;
use amadeo_ecs::Entity;

/// A request to move a shape through the world, sliding along whatever it hits.
///
/// Build one with [`ShapeMove::new`] and adjust the fields that matter; the defaults are the ones a
/// person-sized character wants, and are documented individually so a game tuning them knows what it
/// is trading.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShapeMove {
    /// The shape being moved. A [`Shape::Capsule`] for anything that walks — it slides over small
    /// bumps rather than catching on them, and cannot tip over on a corner.
    pub shape: Shape,
    /// Where the shape starts, in world units.
    pub translation: [f32; 3],
    /// How the shape starts oriented, in Euler degrees (ADR 0018).
    pub rotation: [f32; 3],
    /// How far to try to move, in world units. **A displacement for this tick, not a velocity** —
    /// the caller has already multiplied by the timestep.
    pub motion: [f32; 3],
    /// Which way is up, used to tell a floor from a wall from a ceiling.
    ///
    /// A field rather than a constant because a game with flipped or rotated gravity is a genre, and
    /// I4 says the engine does not get to rule one out.
    pub up: [f32; 3],
    /// The steepest floor that can be walked up. Anything steeper is treated as a wall.
    ///
    /// 45° is the usual default. Raising it lets a character walk up surfaces that look like they
    /// should be climbed; lowering it makes gentle ramps unwalkable.
    pub max_slope_degrees: f32,
    /// The tallest obstacle to step over automatically, in world units. `0.0` disables stepping.
    ///
    /// This is what makes stairs work. It costs real time — shape-casting upward, forward and back
    /// down at every step — so it is off unless a game asks for it.
    pub step_height: f32,
    /// How far below the shape to look for ground to stay stuck to, in world units. `0.0` disables it.
    ///
    /// Without this, walking off the top of a downward ramp launches the shape into the air on every
    /// small bump, which reads as the character being bouncy rather than as a missing feature.
    pub snap_distance: f32,
    /// A small gap kept between the shape and everything else, in world units.
    ///
    /// Must not be zero. Shape casting against a surface a shape is exactly touching is numerically
    /// unstable, and the symptom is a character that intermittently sinks into or sticks to walls.
    pub skin: f32,
    /// A body to ignore, which is essentially always the moving shape's own.
    ///
    /// Without it the very first thing the cast hits is the character's own collider, and the
    /// character cannot move at all.
    pub ignore: Option<Entity>,
}

impl ShapeMove {
    /// A request to move `shape` from `translation` by `motion`, with person-sized defaults.
    #[must_use]
    pub fn new(shape: Shape, translation: [f32; 3], motion: [f32; 3]) -> Self {
        Self {
            shape,
            translation,
            rotation: [0.0, 0.0, 0.0],
            motion,
            up: [0.0, 1.0, 0.0],
            max_slope_degrees: 45.0,
            // Off by default: it is the expensive one, and a game that wants stairs says so.
            step_height: 0.0,
            snap_distance: 0.1,
            skin: 0.01,
            ignore: None,
        }
    }

    /// The same request, ignoring a body — in practice the moving shape's own.
    #[must_use]
    pub fn ignoring(mut self, entity: Entity) -> Self {
        self.ignore = Some(entity);
        self
    }
}

/// What actually happened to a shape that tried to move.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShapeMotion {
    /// Where the shape ended up, in world units.
    ///
    /// **An absolute position, not the distance travelled.** The caller writes this straight into a
    /// `Transform`, and handing back a delta would mean every call site repeated the same addition —
    /// with the start position it already passed in.
    pub translation: [f32; 3],
    /// Whether the shape is resting on something walkable after the move.
    ///
    /// The flag a jump is allowed by and a fall is detected with. It is computed *after* the motion
    /// is applied, so a shape that walked off a ledge this tick reports `false` this tick.
    pub grounded: bool,
    /// Whether the shape is on a surface too steep to stand on and is sliding down it.
    ///
    /// Distinct from `!grounded`: the shape is touching a floor, it just cannot hold position on it.
    pub sliding_down_slope: bool,
}

impl ShapeMotion {
    /// The result of a move that hit nothing: the motion applied in full, in mid-air.
    ///
    /// What a backend with no collision detection returns, and what a real backend returns in open
    /// space.
    #[must_use]
    pub fn unobstructed(request: &ShapeMove) -> Self {
        Self {
            translation: [
                request.translation[0] + request.motion[0],
                request.translation[1] + request.motion[1],
                request.translation[2] + request.motion[2],
            ],
            grounded: false,
            sliding_down_slope: false,
        }
    }
}

/// A request to sweep a shape along a straight line and find the first thing in the way — ADR 0054.
///
/// # How this differs from [`ShapeMove`], which is the whole point
///
/// `ShapeMove` asks *"I am a body that wants to go there; where do I end up?"* and answers by
/// **sliding** along whatever it hits, because that is what a character walking into a wall should
/// do. This asks *"how far along this line before something is in the way?"* and answers with a
/// distance. Nothing slides, nothing steps, nothing snaps to the ground.
///
/// Using the first to answer the second is what **Q34** recorded and what session 15 watched fail
/// twice. A camera arm pointing down and back hits the ground, slides *backward* along it, and the
/// backward part of that slide is most of the direction the arm was pointing — so projecting the
/// travel onto the arm reported six metres of progress for a shape that had gone almost nowhere in
/// the direction asked for, and put the camera underground.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShapeCast {
    /// The shape being swept.
    pub shape: Shape,
    /// Where the sweep starts, in world units.
    pub translation: [f32; 3],
    /// How the shape is oriented throughout, in Euler degrees (ADR 0018). A cast does not rotate.
    pub rotation: [f32; 3],
    /// How far to sweep, in world units. The hit's [`ShapeHit::fraction`] is a fraction *of this*.
    pub motion: [f32; 3],
    /// A gap kept between the shape and what it hits, in world units.
    ///
    /// The same idea as [`ShapeMove::skin`] and for the same reason: a shape left exactly touching a
    /// surface is the degenerate case for the next query against it. Unlike `ShapeMove`'s, this one
    /// **may** be zero, because a cast does not have to leave the shape anywhere.
    pub skin: f32,
    /// A body to ignore, which is usually whatever the sweep starts on or inside.
    pub ignore: Option<Entity>,
}

impl ShapeCast {
    /// A sweep of `shape` from `translation` along `motion`, unrotated, with a small skin.
    #[must_use]
    pub fn new(shape: Shape, translation: [f32; 3], motion: [f32; 3]) -> Self {
        Self {
            shape,
            translation,
            rotation: [0.0, 0.0, 0.0],
            motion,
            skin: 0.01,
            ignore: None,
        }
    }

    /// The same sweep, ignoring a body — in practice whatever it starts inside.
    #[must_use]
    pub fn ignoring(mut self, entity: Entity) -> Self {
        self.ignore = Some(entity);
        self
    }
}

/// What a [`ShapeCast`] ran into.
///
/// Absent — a `None` from [`PhysicsBackend::cast_shape`](crate::PhysicsBackend::cast_shape) — means
/// the whole motion is clear. That is a different statement from a hit at `fraction == 1.0`, which
/// means something is exactly at the far end, and callers that care about the difference get to see
/// it rather than having it flattened into a number.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShapeHit {
    /// How much of [`ShapeCast::motion`] was travelled before the hit, from `0.0` to `1.0`.
    ///
    /// A fraction rather than a distance because it is the unit-free answer: multiplying by the
    /// motion's length gives a distance, and a caller that wants a position gets one below without
    /// having to reconstruct it.
    pub fraction: f32,
    /// Where the shape's origin ends up, in world units.
    ///
    /// **The start plus `fraction` of the motion**, so it is always on the line asked about. That is
    /// the property [`ShapeMotion::translation`] cannot offer, since a slide leaves the line.
    pub translation: [f32; 3],
    /// The surface normal at the point of contact, pointing out of the thing that was hit.
    ///
    /// Nothing in the engine reads this yet. It is here because every other caller Q34 named — a
    /// bullet that should ricochet, a placement check that wants to know if it landed on a floor or
    /// a wall — needs it, and adding it later would change a returned type rather than extend one.
    pub normal: [f32; 3],
    /// **What** was hit, when it is an entity.
    ///
    /// `None` means static geometry — a `StaticMesh`, which belongs to a level rather than to a body
    /// and has no entity to name. That is a real answer rather than a failure: "you are looking at
    /// the ground" is what a level is for.
    ///
    /// # Why this was missing and why it matters
    ///
    /// The original `ShapeHit` said *where* a cast stopped and not *what* it stopped against, which
    /// is enough for the two callers it was written for — both camera sweeps only need a distance.
    /// Everything else needs the entity: an interaction prompt asks what is under the crosshair, an
    /// AI's line of sight asks whether the thing in the way is the player or a wall, and a projectile
    /// asks what it should damage. `docs/05` names two modules that cannot be built without it.
    ///
    /// It travels on the **collider** rather than through a lookup table beside it, so it cannot
    /// drift out of step with the body it describes — the failure two parallel maps always
    /// eventually have.
    pub entity: Option<Entity>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unobstructed_move_travels_the_whole_way() {
        let request = ShapeMove::new(
            Shape::Capsule {
                radius: 0.4,
                height: 1.2,
            },
            [1.0, 2.0, 3.0],
            [0.5, 0.0, -0.25],
        );
        let motion = ShapeMotion::unobstructed(&request);
        assert_eq!(motion.translation, [1.5, 2.0, 2.75]);
        // Nothing was detected, so nothing can be claimed about the ground.
        assert!(!motion.grounded);
    }

    #[test]
    fn the_defaults_are_the_ones_a_walking_character_wants() {
        // Pinned because they are the difference between "the controller feels wrong" and "the
        // controller is wrong", and a silent change to one reads as the former.
        let request = ShapeMove::new(Shape::Sphere { radius: 1.0 }, [0.0; 3], [0.0; 3]);
        assert_eq!(request.up, [0.0, 1.0, 0.0]);
        assert_eq!(request.max_slope_degrees, 45.0);
        // Stepping is expensive, so a game that wants stairs asks for them.
        assert_eq!(request.step_height, 0.0);
        // And the skin must never be zero, or shape casting goes numerically unstable.
        assert!(request.skin > 0.0);
    }

    #[test]
    fn ignoring_names_the_body_to_skip() {
        let mut world = amadeo_ecs::World::new();
        let entity = world.spawn();
        let request = ShapeMove::new(Shape::Sphere { radius: 1.0 }, [0.0; 3], [1.0, 0.0, 0.0])
            .ignoring(entity);
        assert_eq!(request.ignore, Some(entity));
    }
}
