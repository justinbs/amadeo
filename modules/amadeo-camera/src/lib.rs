//! A third-person follow camera that keeps itself out of the world — **Q27**.
//!
//! ```text
//! amadeo_camera::install(&mut app)?;
//! ```
//!
//! # What it is for
//!
//! A follow camera is a child entity sitting a fixed distance behind whatever it follows (ADR 0031),
//! and nothing stops that spot being *inside a wall*. Walk into a corner, or into a dip, and the
//! camera ends up in the geometry.
//!
//! What you see then is worse than it sounds. A surface is only drawn where it exists, so a camera
//! inside solid geometry sees the far side of the world through it — and before ADR 0052 made
//! surfaces two-sided, it saw nothing at all and the sky pass filled the frame. That is what
//! *"digging down shows the sky"* turned out to be, and it read as terrain failing to stream rather
//! than as a camera in the wrong place.
//!
//! # Why this is a module rather than engine core
//!
//! Trap 10: the core must not assume a game has a character with a camera behind it. Three of the
//! eight target games are 2D or isometric and one has no character at all. A camera rig is exactly
//! the genre knowledge ADR 0037 created `modules/` to hold.
//!
//! **And it does not depend on `amadeo-character` either.** It follows a [`Parent`], whatever that
//! parent is — a vehicle, a ball, a cursor. Nothing here knows what a character is.
//!
//! # It lived in `games/scarp` first, deliberately
//!
//! The rule this project uses is the one `games/scarp`'s `bin/turf` states: something lives in a game
//! until a *second* game wants it, and that is the moment to promote rather than the moment to guess
//! at an interface. `games/atrium` wanting it is what moved it — and Q27's original wording was
//! about walls, which is the Atrium's case rather than the Scarp's.

use amadeo_app::{App, Stage, system};
use amadeo_ecs::{Component, Entity, World};
use amadeo_input::{ActionId, InputState};
use amadeo_physics::{Physics, Shape, ShapeMove};
use amadeo_reflect::RegistryError;
use amadeo_transform::{Mat4, Parent, Transform};

/// The label [`keep_camera_clear`] is registered under.
pub const KEEP_CAMERA_CLEAR: &str = "keep_camera_clear";

/// The label [`look_with_mouse`] is registered under.
pub const LOOK_WITH_MOUSE: &str = "look_with_mouse";

/// Held to steer the camera. Bound to the right mouse button by a game's window layer.
pub const LOOK: &str = "look";

/// Pointer movement across the screen while [`LOOK`] is held, in pixels this tick.
pub const LOOK_X: &str = "look_x";

/// Pointer movement up and down the screen while [`LOOK`] is held, in pixels this tick.
pub const LOOK_Y: &str = "look_y";

/// A camera that follows its [`Parent`] and pulls in when something gets between them.
///
/// Put it on the camera entity, which must be a child of whatever it follows. Everything is in world
/// units except [`FollowCamera::degrees_per_pixel`].
#[derive(Debug, Clone, Copy, PartialEq, amadeo_core::StableHash, amadeo_reflect::Reflect)]
pub struct FollowCamera {
    /// How far above the parent's origin the camera pivots.
    #[reflect(min = 0.0, max = 100.0, unit = "world units")]
    pub height: f32,
    /// How far behind the parent the camera sits when nothing is in the way.
    #[reflect(min = 0.0, max = 100.0, unit = "world units")]
    pub distance: f32,
    /// How close it may come when something is.
    ///
    /// Never zero: a camera pulled all the way to the pivot sits inside what it is following.
    #[reflect(min = 0.0, max = 100.0, unit = "world units")]
    pub min_distance: f32,
    /// The radius of the sphere swept to find obstructions.
    ///
    /// Bigger than nothing on purpose. A zero-radius ray slips through the crack between two
    /// triangles at a chunk boundary and reports open space where there is rock — and it has to
    /// exceed the near plane's half-diagonal, or geometry enters the frustum before the sweep
    /// notices it. At a 65° field of view and a near plane of 0.1 that is about 0.13.
    #[reflect(min = 0.0, max = 10.0, unit = "world units")]
    pub radius: f32,
    /// How fast the camera eases back out once nothing is in the way, per second.
    ///
    /// **Only ever applies outward.** Coming *in* happens the same tick the obstruction appears,
    /// because easing that direction means spending a frame inside a wall. Going back out is eased,
    /// because a sweep grazing an edge is noisy and a camera that snapped both ways flickers
    /// visibly. Snap in, drift out.
    #[reflect(min = 0.0, max = 100.0, unit = "world units per second")]
    pub return_speed: f32,
    /// How far a pixel of pointer movement turns the view, in degrees.
    #[reflect(min = 0.0, max = 10.0, unit = "degrees per pixel")]
    pub degrees_per_pixel: f32,
    /// How far down the view may be tilted, in degrees. Negative is downward.
    #[reflect(min = -89.0, max = 0.0, unit = "degrees")]
    pub min_pitch: f32,
    /// How far up the view may be tilted, in degrees.
    ///
    /// Both limits stop short of vertical deliberately. At exactly vertical the camera's forward
    /// direction is parallel to the world up its basis is built from, the basis collapses, and the
    /// view rolls — ADR 0018's gimbal problem arriving somewhere concrete.
    #[reflect(min = 0.0, max = 89.0, unit = "degrees")]
    pub max_pitch: f32,
}

impl Default for FollowCamera {
    fn default() -> Self {
        Self {
            height: 3.0,
            distance: 7.0,
            min_distance: 1.2,
            radius: 0.35,
            return_speed: 6.0,
            degrees_per_pixel: 0.18,
            min_pitch: -75.0,
            max_pitch: 30.0,
        }
    }
}

impl Component for FollowCamera {}

/// Registers [`FollowCamera`] and both of its systems.
///
/// # Ordering, and both constraints are load-bearing
///
/// [`look_with_mouse`] runs **before** anything that reads the parent's rotation, so a turn takes
/// effect the same tick rather than the next one. [`keep_camera_clear`] runs **after**
/// [`amadeo_physics::STEP_PHYSICS`], because `move_shape` answers from an index that step builds —
/// asking earlier queries an empty world and finds open space everywhere, which is the system doing
/// nothing at all while looking like it works.
///
/// Systems with no declared constraint run in *alphabetical* order, so neither of these can be left
/// to chance: `keep_camera_clear` sorts before `look_with_mouse`, which is the wrong way round.
///
/// # Errors
///
/// [`RegistryError`] if [`FollowCamera`] is already registered under a different type.
pub fn install(app: &mut App) -> Result<(), RegistryError> {
    app.register_component::<FollowCamera>()?;

    app.add_system(
        Stage::Simulation,
        system(LOOK_WITH_MOUSE, look_with_mouse).before(KEEP_CAMERA_CLEAR),
    );
    app.add_system(
        Stage::Simulation,
        system(KEEP_CAMERA_CLEAR, keep_camera_clear).after(amadeo_physics::STEP_PHYSICS),
    );
    Ok(())
}

/// Turns the view with the pointer while [`LOOK`] is held.
///
/// # Yaw goes on the parent, pitch goes on the camera
///
/// That split is what makes a third-person view feel right rather than a trick. Turning rotates
/// **what is being followed**, so moving forward goes where you are looking and the camera comes
/// round for free — it is a child entity, so its offset is already in the parent's space. Tilting
/// rotates **only the camera**, because a character that pitched would lean over and walk into the
/// ground.
///
/// # Why this writes rotation directly rather than feeding a turn action
///
/// A turn *action* is a rate — a controller multiplies it by a turn speed and the timestep, so full
/// deflection is a fixed degrees per second. A pointer is a *displacement*: a fast flick should turn
/// further than a slow drag covering the same time. Squeezing one into the other caps how fast the
/// view can move at whatever the turn speed happens to be.
///
/// Writing the parent's rotation is safe where writing its *translation* would not be (**Q30**):
/// `move_shape` is handed a rotation and returns only a position, so nothing reads this back and
/// overwrites it.
pub fn look_with_mouse(world: &mut World) {
    let Some(input) = world.resource::<InputState>() else {
        return;
    };
    if !input.pressed(ActionId::new(LOOK)) {
        return;
    }
    let dx = input.axis(ActionId::new(LOOK_X));
    let dy = input.axis(ActionId::new(LOOK_Y));
    if dx == 0.0 && dy == 0.0 {
        return;
    }

    let cameras: Vec<(Entity, FollowCamera, Entity)> = world
        .query::<(&FollowCamera, &Parent)>()
        .map(|(entity, (follow, parent))| (entity, *follow, parent.0))
        .collect();

    for (entity, follow, parent) in cameras {
        // Yaw onto the parent. Subtracted, because a turn action is conventionally positive for
        // *left* and moving the pointer right should look right.
        if let Some(mut transform) = world.get::<Transform>(parent).copied() {
            transform.rotation[1] -= dx * follow.degrees_per_pixel;
            world.insert(parent, transform);
        }

        // Pitch onto the camera. Also subtracted: a window's y grows *downward*, so pushing the
        // pointer away from you is negative and should raise the view.
        if let Some(mut transform) = world.get::<Transform>(entity).copied() {
            transform.rotation[0] = (transform.rotation[0] - dy * follow.degrees_per_pixel)
                .clamp(follow.min_pitch, follow.max_pitch);
            world.insert(entity, transform);
        }
    }
}

/// Pulls each [`FollowCamera`] in until nothing is between it and what it follows.
///
/// # Two sweeps, not one
///
/// The obvious one goes backwards from the pivot to where the camera wants to be. The second goes
/// **upward from the parent to the pivot**, and skipping it is a bug that only shows up indoors or
/// underground: the pivot is a point some distance above the parent, and in a tunnel or a low
/// corridor that point is inside the ceiling. A shape cast that *starts* embedded in geometry has no
/// good answer — solvers differ on whether they report an immediate hit, no hit, or push out — so
/// what comes back is arbitrary, and arbitrary per tick is a flicker.
///
/// # Why the result is projected rather than measured
///
/// [`ShapeMove`] is a *character* move: it slides along whatever it hits, because that is what a body
/// walking into a wall should do. A camera wants the other question — how far along this one axis
/// before something is in the way — and the engine has no pure shape cast to ask (**Q34**). Measuring
/// the straight-line distance travelled counts a slide as progress, so a camera brushing a wall got a
/// distance with little to do with where it was pointed. A dot product keeps only the component that
/// was actually asked for.
pub fn keep_camera_clear(world: &mut World) {
    // Collected before touching the physics service, because the query borrows the world and the
    // sweeps need it mutably. The current distance comes along because the ease-out below needs it.
    let cameras: Vec<(Entity, FollowCamera, Transform, f32)> = world
        .query::<(&FollowCamera, &Parent, &Transform)>()
        .filter_map(|(entity, (follow, parent, transform))| {
            // The parent's own transform, which *is* its world transform: a follow camera's parent
            // is a root entity. Reading `GlobalTransform` would be a tick behind, because
            // propagation runs at the end of the tick.
            let parent_transform = world.get::<Transform>(parent.0)?;
            Some((entity, *follow, *parent_transform, transform.translation[2]))
        })
        .collect();

    if cameras.is_empty() {
        return;
    }
    let Some(physics) = world.service_mut::<Physics>() else {
        return;
    };

    let mut results: Vec<(Entity, [f32; 3])> = Vec::with_capacity(cameras.len());
    for (entity, follow, parent, current) in cameras {
        let sphere = Shape::Sphere {
            radius: follow.radius,
        };
        // Stepping and ground snapping are off throughout: a camera does not walk, and either would
        // pull it somewhere the geometry did not ask for.
        let sweep = |from: [f32; 3], motion: [f32; 3]| ShapeMove {
            step_height: 0.0,
            snap_distance: 0.0,
            ..ShapeMove::new(sphere, from, motion)
        };

        // Upward first, so the pivot is in open air — see this function's docs.
        let pivot = physics
            .move_shape(&sweep(parent.translation, [0.0, follow.height, 0.0]))
            .translation;

        // The parent's axes in world space. Column two is its local +Z, and a camera looks along its
        // own negative Z — so +Z is *behind* the thing being followed.
        let basis = Mat4::from_transform(parent.translation, parent.rotation, [1.0, 1.0, 1.0]);
        let back = [
            basis.columns[2][0],
            basis.columns[2][1],
            basis.columns[2][2],
        ];
        let wanted = [
            back[0] * follow.distance,
            back[1] * follow.distance,
            back[2] * follow.distance,
        ];

        let landed = physics.move_shape(&sweep(pivot, wanted)).translation;
        let delta = [
            landed[0] - pivot[0],
            landed[1] - pivot[1],
            landed[2] - pivot[2],
        ];
        let along = delta[0] * back[0] + delta[1] * back[1] + delta[2] * back[2];
        let target = along.clamp(follow.min_distance, follow.distance);

        // Snap in, drift out — see `FollowCamera::return_speed`.
        let distance = if target < current {
            target
        } else {
            (current + follow.return_speed * amadeo_core::FIXED_DT).min(target)
        };

        // The height the pivot *reached*, not the one it asked for. A ceiling stops it short, and
        // using the requested height would put the camera back inside what the sweep just avoided.
        results.push((entity, [0.0, pivot[1] - parent.translation[1], distance]));
    }

    for (entity, translation) in results {
        // Rotation and scale are the scene's and `look_with_mouse`'s to own — this only ever moves
        // the camera along the axes it is allowed to move along.
        if let Some(mut transform) = world.get::<Transform>(entity).copied() {
            transform.translation = translation;
            world.insert(entity, transform);
        }
    }
}
