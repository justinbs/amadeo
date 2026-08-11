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
use amadeo_physics::{Physics, Shape, ShapeCast};
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

/// Where the camera's arm actually is right now, as opposed to where [`FollowCamera`] wants it.
///
/// # Why this is a second component rather than another field
///
/// ADR 0037 split `CharacterController` from `CharacterMotion` on exactly this line, and the same
/// line applies here: [`FollowCamera`] is **authored** and this module never writes it, while this is
/// **written every tick** and a person authoring a scene has no business setting it. Folding the two
/// together would produce a component half of whose fields are ignored on load.
///
/// # Why it cannot just be read back out of the `Transform`
///
/// The camera's local position is `pivot + arm × distance`, so recovering the distance means knowing
/// where the pivot was — and the pivot *moves* when a ceiling stops the upward sweep short. Deriving
/// it from this tick's pivot would give a wrong answer on exactly the ticks the smoothing exists to
/// handle, which is the class of bug this rig has already shipped once.
///
/// Hashed, because it genuinely is simulation state: it must survive to the next tick, which is the
/// test [`amadeo_ecs::Component::DERIVED`] states.
#[derive(Debug, Clone, Copy, PartialEq, amadeo_core::StableHash, amadeo_reflect::Reflect)]
pub struct CameraArm {
    /// How far the camera currently is from its pivot, in world units.
    ///
    /// Eased toward [`FollowCamera::distance`] and cut short by whatever the sweep hits. Authored
    /// once as a starting value — usually the same as `distance`, so a scene opens with the camera
    /// already out rather than easing outward through the first second of play.
    #[reflect(min = 0.0, max = 100.0, unit = "world units")]
    pub distance: f32,
}

impl Default for CameraArm {
    fn default() -> Self {
        Self {
            distance: FollowCamera::default().distance,
        }
    }
}

impl Component for CameraArm {}

/// Registers [`FollowCamera`], [`CameraArm`], and both systems.
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
/// # And one ordering that is correct by luck rather than by declaration
///
/// Whatever moves the parent this tick must run *before* [`keep_camera_clear`], or the camera
/// follows where the parent was last tick and lags visibly. In practice that is
/// `amadeo_character::DRIVE_CHARACTERS`, which is also `.after(STEP_PHYSICS)` and which sorts
/// alphabetically before this — so the schedule is right today and nothing states it.
///
/// It is **not** declared here, because this module deliberately does not depend on
/// `amadeo-character` (trap 10: a camera rig must not assume a character exists). Naming the label as
/// a bare string would couple them just as tightly while also compiling when it is wrong. So
/// `the_camera_reads_the_parent_after_it_has_moved` in the Scarp's tests pins the resolved order
/// instead, and a rename turns CI red rather than producing a camera that trails by one tick.
///
/// # Errors
///
/// [`RegistryError`] if either component is already registered under a different type.
pub fn install(app: &mut App) -> Result<(), RegistryError> {
    app.register_component::<FollowCamera>()?;
    app.register_component::<CameraArm>()?;

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

/// Swings each [`FollowCamera`] round its pivot to match its pitch, and pulls it in when something
/// gets in the way.
///
/// # The camera orbits its pivot; it does not sit at a fixed offset and tilt
///
/// This is the difference between a third-person camera and a camera that happens to be behind
/// something, and getting it wrong is what **Justin reported in session 15**: tilting down looked at
/// the ground immediately below the camera and tilting up looked at the sky, with the player leaving
/// the frame in both directions.
///
/// The cause was that the camera's position was the constant `[0, height, distance]` — pitch went
/// into its *rotation* and reached its position nowhere. So the arm never moved. What a spring arm
/// does instead (Unreal's `USpringArmComponent` and Cinemachine's orbital rigs both work this way) is
/// treat pitch as an angle **around the pivot**: tilting down lifts the camera up and over so it
/// looks down *at* the thing it follows, and tilting up drops it so it looks up past it. The camera
/// stays framed at every angle, for free, because its forward direction is exactly the arm reversed.
///
/// # Two sweeps, not one
///
/// The obvious one goes backwards along the arm from the pivot to where the camera wants to be. The
/// second goes **upward from the parent to the pivot**, and skipping it is a bug that only shows up
/// indoors or underground: the pivot is a point some distance above the parent, and in a tunnel or a
/// low corridor that point is inside the ceiling.
///
/// # Both sweeps ignore the thing being followed, and that was the flicker
///
/// A shape cast that *starts* embedded in geometry has no good answer, and the pivot sweep starts at
/// the parent's own origin — which is the middle of the parent's own collider. Rapier reads that
/// penetration as a surface too steep to stand on, reports `sliding_down_slope`, and cancels the
/// motion entirely.
///
/// **It does so intermittently**, because whether the contact resolves that way depends on the exact
/// penetration normal, which shifts as the parent moves. So the pivot collapsed to the parent's feet
/// on roughly one tick in ten, the camera snapped to its minimum distance, eased back out at
/// `return_speed`, and was knocked down again long before it arrived. That is
/// *"any movement in any direction makes the camera flicker close or far"*, and it is one missing
/// [`ShapeCast::ignoring`] — the same call `modules/amadeo-character` has always made, for the same
/// reason, one crate away.
///
/// # Both sweeps are casts, not moves, and that took three attempts to get right
///
/// [`ShapeCast`] asks *"how far along this line before something is in the way?"*.
/// [`ShapeMove`](amadeo_physics::ShapeMove) asks
/// *"where does this body end up?"* and **slides** along whatever it hits to answer, because that is
/// what a character walking into a wall should do. Until ADR 0054 the engine only had the second, and
/// this function borrowed it — which failed twice, in ways that looked unrelated:
///
/// 1. The slide counted as progress. Measuring the straight-line distance travelled made a camera
///    brushing a slope report a distance with little to do with the axis it was pointed along.
///    Projecting onto the arm fixed *that*, and was a workaround.
/// 2. **The projection then failed on its own terms.** With the view tilted up, the arm points down
///    and back; it hits the ground, slides *backward* along it, and backward is most of the arm's
///    direction. So the projection reported nearly a full arm's progress for a shape that had gone
///    almost nowhere in the direction asked for, and the camera ended up **under the terrain looking
///    up at its underside** — which reads as the sky, because the ground's underside is unlit
///    (ADR 0052). Reported by Justin, and it is Q34 arriving with a picture.
///
/// A cast has no slide to misinterpret, so neither correction is needed and neither can come back.
pub fn keep_camera_clear(world: &mut World) {
    // Collected before the physics service is read, because the query borrows the world and the
    // writes at the end need it mutably. Everything the loop reads comes along, including the
    // parent's entity — the sweeps have to name it to exclude it.
    let cameras: Vec<Rig> = world
        .query::<(&FollowCamera, &CameraArm, &Parent, &Transform)>()
        .filter_map(|(entity, (follow, arm, parent, transform))| {
            // The parent's own transform, which *is* its world transform: a follow camera's parent
            // is a root entity. Reading `GlobalTransform` would be a tick behind, because
            // propagation runs at the end of the tick.
            let parent_transform = world.get::<Transform>(parent.0)?;
            Some(Rig {
                entity,
                follow: *follow,
                parent: *parent_transform,
                parent_entity: parent.0,
                distance: arm.distance,
                pitch: transform.rotation[0],
            })
        })
        .collect();

    if cameras.is_empty() {
        return;
    }
    let Some(physics) = world.service::<Physics>() else {
        return;
    };

    let mut results: Vec<(Entity, [f32; 3], f32)> = Vec::with_capacity(cameras.len());
    for rig in cameras {
        let follow = rig.follow;
        let sphere = Shape::Sphere {
            radius: follow.radius,
        };
        // Ignoring the parent throughout is what stops a sweep starting inside the followed body's
        // own collider — see this function's docs. Returns how far along `motion` is clear, as a
        // fraction, with `None` meaning all of it.
        let clear_fraction = |from: [f32; 3], motion: [f32; 3]| {
            physics
                .cast_shape(&ShapeCast::new(sphere, from, motion).ignoring(rig.parent_entity))
                .map_or(1.0, |hit| hit.fraction)
        };

        // Upward first, so the pivot is in open air.
        let rise =
            follow.height * clear_fraction(rig.parent.translation, [0.0, follow.height, 0.0]);
        let pivot = [
            rig.parent.translation[0],
            rig.parent.translation[1] + rise,
            rig.parent.translation[2],
        ];

        // The parent's axes in world space, as a pure rotation — scale is forced to one so the
        // columns stay unit vectors and the dot products below are projections rather than
        // projections-times-something.
        let basis = Mat4::from_transform(rig.parent.translation, rig.parent.rotation, [1.0; 3]);
        let up = column(&basis, 1);
        let back = column(&basis, 2);

        // **The arm, and this is the orbit.** Pitch rotates the arm about the parent's local +X, so
        // it leans out of the horizontal plane instead of the camera merely tilting in place.
        // Negative pitch is downward, and a downward look must raise the camera — hence `-sin`.
        //
        // Deterministic trigonometry (ADR 0053): this reaches the camera's `Transform`, which is
        // hashed, and `f32::sin_cos` is not specified to give the same answer on two machines.
        let (sin_pitch, cos_pitch) = amadeo_core::sin_cos_degrees(rig.pitch);
        let arm = [
            up[0] * -sin_pitch + back[0] * cos_pitch,
            up[1] * -sin_pitch + back[1] * cos_pitch,
            up[2] * -sin_pitch + back[2] * cos_pitch,
        ];

        let wanted = [
            arm[0] * follow.distance,
            arm[1] * follow.distance,
            arm[2] * follow.distance,
        ];
        // A fraction of a known length is a distance along the arm directly — no projection, and
        // nothing to reinterpret, because a cast never leaves the line it was given.
        // A fraction of a known length is a distance along the arm directly — no projection, and
        // nothing to reinterpret, because a cast never leaves the line it was given.
        let target = (follow.distance * clear_fraction(pivot, wanted))
            .clamp(follow.min_distance, follow.distance);

        // Snap in, drift out — see `FollowCamera::return_speed`.
        let distance = if target < rig.distance {
            target
        } else {
            (rig.distance + follow.return_speed * amadeo_core::FIXED_DT).min(target)
        };

        // Back into the parent's frame, because the camera is a child and its `Transform` is a local
        // one. The basis is a pure rotation, so its inverse is its transpose — which is what
        // dotting the offset against each column comes to. Done properly rather than assuming the
        // parent is upright: it *is* upright today, since only yaw is ever written to it, and an
        // assumption that holds by coincidence is the kind this rig has already been bitten by.
        let world_position = [
            pivot[0] + arm[0] * distance,
            pivot[1] + arm[1] * distance,
            pivot[2] + arm[2] * distance,
        ];
        let offset = [
            world_position[0] - rig.parent.translation[0],
            world_position[1] - rig.parent.translation[1],
            world_position[2] - rig.parent.translation[2],
        ];
        let local = [
            dot(offset, column(&basis, 0)),
            dot(offset, up),
            dot(offset, back),
        ];

        results.push((rig.entity, local, distance));
    }

    for (entity, translation, distance) in results {
        // Rotation and scale are the scene's and `look_with_mouse`'s to own — this only ever moves
        // the camera, never aims it. The camera's forward is its own local −Z, which after pitching
        // is exactly the arm reversed, so it already points at the pivot with nothing else to do.
        if let Some(mut transform) = world.get::<Transform>(entity).copied() {
            transform.translation = translation;
            world.insert(entity, transform);
        }
        world.insert(entity, CameraArm { distance });
    }
}

/// One camera's inputs, gathered before the physics service is borrowed.
///
/// A named struct rather than a tuple because six positional fields at a `for` loop is where
/// "the third one is the pitch, I think" starts costing debugging time.
struct Rig {
    /// The camera entity.
    entity: Entity,
    /// What it is asking for.
    follow: FollowCamera,
    /// The world transform of whatever it follows.
    parent: Transform,
    /// That parent's entity, so the sweeps can exclude its collider.
    parent_entity: Entity,
    /// The arm length carried over from last tick.
    distance: f32,
    /// The camera's tilt in degrees, negative downward.
    pitch: f32,
}

/// One column of a matrix as a 3-vector, dropping the homogeneous row.
fn column(matrix: &Mat4, index: usize) -> [f32; 3] {
    [
        matrix.columns[index][0],
        matrix.columns[index][1],
        matrix.columns[index][2],
    ]
}

/// The dot product of two 3-vectors.
fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
