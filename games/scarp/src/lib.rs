//! **The Scarp** — M2.5's demo: a generated world, streamed in chunks, that you can walk on and dig
//! into.
//!
//! ```text
//! cargo run -p scarp
//! ```
//!
//! WASD to walk, Q and E to turn, Space to jump, F to dig, Escape to quit.
//!
//! # What this is for
//!
//! Everything under chunked streaming was built across session 12 and **none of it had ever carried
//! a player**: residency, the terrain source, per-chunk meshing, static trimesh colliders, the
//! streamer, the ECS layer, and digging. Each was proved by headless tests, and by nothing else.
//!
//! That is the same position `games/vault` was built to fix in M1 and `games/atrium` in M2, and the
//! bet paid both times — a real game found things about the engine that no amount of reasoning had.
//! This is M2.5's version, and it is exit gate 1.
//!
//! # What it demonstrates that the Atrium could not
//!
//! The Atrium is a room: eleven boxes, all of them present from the first tick to the last. Here
//! **nothing is authored except the player, the camera and the sun.** The ground does not exist in
//! any file. It is a function — [`Highlands`] — sampled into chunks as the player approaches and
//! dropped as they leave, meshed on a job pool, made solid on the tick the player needs it, and
//! changed when they dig.
//!
//! # The seed reaches the world, and that is ADR 0042's claim cashed
//!
//! The terrain is a **pure function of the seed**, so `--seed` gives a different world and the same
//! seed gives the same world on every machine. Nothing is stored. A save file for a world like this
//! is that seed plus the list of samples somebody dug — which is what ADR 0042 was written to make
//! true and what `Highlands` being built on [`amadeo_noise`] makes safe (ADR 0044).

use amadeo_app::{App, Stage, system};
use amadeo_ecs::World;
use amadeo_input::{ActionId, InputDriver, InputState, NullSource};
use amadeo_noise::Fbm;
use amadeo_physics::{Collider, Gravity, Physics, RapierPhysics, RigidBody, Velocity};
use amadeo_render::{
    BoxMesh, Camera, DirectionalLight, Environment, Material, Mesh, PlaneMesh, SortOrder,
    TextureCache,
};
use amadeo_terrain::{STREAM_TERRAIN, Terrain, TerrainEdits, TerrainSettings, TerrainViewer};
use amadeo_transform::{
    GlobalTransform, PROPAGATE_TRANSFORMS, Parent, Transform, propagate_transforms,
};
use amadeo_voxel::{ChunkShape, TerrainSource};
use std::sync::Arc;

/// Where this game's assets live, relative to the project root (ADR 0022).
const ASSET_DIRECTORY: &str = "games/scarp/assets";

/// The player, the camera and the sun. The ground is not in here — it is generated.
const SCENE: &str = include_str!("../scenes/scarp.scene");

/// The seed, when nothing overrides it. `--seed` on the command line changes the world.
const DEFAULT_SEED: u64 = 0x0053_4341_5250;

/// The named input action that digs.
pub const DIG: &str = "dig";

/// The label [`dig_terrain`] is registered under.
pub const DIG_TERRAIN: &str = "dig_terrain";

/// How many cells across a chunk is, and how big a cell is in world units.
///
/// A chunk stays **sixteen metres** across, which is roughly what Minecraft and `godot_voxel` both
/// settle on. Smaller chunks mean more of them and more per-chunk overhead; larger ones mean more
/// work wasted whenever one is re-meshed after a dig.
///
/// **Eight two-metre cells rather than sixteen one-metre ones, and that is the low-poly decision
/// showing up in content** (ADR 0050). Flat shading alone did not deliver the look: at a
/// one-metre triangle a facet is a few pixels at any distance worth looking at, so the faceting was
/// technically present and visually absent. Doubling the cell makes a triangle something you can
/// see the edges of, which is what low-poly means.
///
/// Two consequences, both real. The ground is a **quarter** of the triangles, so chunks mesh faster
/// and use less memory. And it is coarser underfoot: a two-metre cell cannot express a one-metre
/// bump, so the collider a character walks on is blockier — which is the same trade every voxel game
/// makes and is why this is a game's constant rather than an engine default.
const CHUNK: ChunkShape = ChunkShape {
    cells: 8,
    cell_size: 2.0,
};

/// How far a dig reaches from the digger, **in metres**.
///
/// # It used to be in cells, and that was a bug waiting for the cell size to change
///
/// Which it then did. Coarsening the ground from one-metre cells to two-metre ones for the low-poly
/// look (ADR 0050) doubled every dig without a line of the digging code changing — a two-metre hole
/// became a four-metre one, wide enough to reach the bottom of the streamed region. Below that there
/// is no geometry at all, so the sky pass filled it and digging down showed the sky through the
/// ground.
///
/// Reported by Justin, and the same shape as the sun direction that was written out by hand: a
/// quantity expressed in units of something else that later moved. Stating it in metres and
/// converting is what makes the cell size free to change again.
const DIG_RADIUS_METRES: f32 = 2.0;

/// The label [`keep_camera_out_of_the_ground`] is registered under.
pub const KEEP_CAMERA_OUT_OF_THE_GROUND: &str = "keep_camera_out_of_the_ground";

/// A third-person camera that pulls in when something is between it and what it follows — **Q27**.
///
/// # The bug this exists to stop, which is worse than it sounds
///
/// A follow camera is a child entity sitting a fixed distance behind its parent (ADR 0031), and
/// nothing stopped that spot being *inside the ground*. Walk into a dip, or dig down, and the camera
/// ends up under the terrain.
///
/// What you then see is not "the inside of a hill". Surface nets meshes only the boundary between
/// solid and air, so **solid rock contains no geometry at all** — and the boundary's faces point
/// outward, so from underneath they are backface-culled and vanish. The camera looks straight
/// through the world to the skybox. Reported as "digging down shows the sky", which is exactly what
/// it looks like and not at all where the cause is.
///
/// # Why it lives in this game rather than in `modules/`
///
/// It is the second occupant that layer would want, and `games/atrium` has the same camera and the
/// same problem. But the rule this project uses is the one `bin/turf` states: something lives in a
/// game until a *second* game wants it, and that is the moment to promote rather than the moment to
/// guess at an interface. The Atrium wanting this is what should move it — and moving a component
/// and one system is a file move, where designing a camera-rig crate around one caller is not.
///
/// Trap 10 is the constraint to keep in mind when it does move: a camera rig must not assume a
/// character exists. Nothing here does — it follows a `Parent`, whatever that parent is.
#[derive(Debug, Clone, Copy, PartialEq, amadeo_core::StableHash, amadeo_reflect::Reflect)]
pub struct FollowCamera {
    /// How far above the parent's origin the camera pivots, in world units.
    #[reflect(min = 0.0, max = 100.0, unit = "world units")]
    pub height: f32,
    /// How far behind the parent the camera sits when nothing is in the way.
    #[reflect(min = 0.0, max = 100.0, unit = "world units")]
    pub distance: f32,
    /// How close it is allowed to come when something is.
    ///
    /// Never zero: a camera pulled all the way to the pivot sits inside the thing it is following,
    /// which is its own kind of wrong.
    #[reflect(min = 0.0, max = 100.0, unit = "world units")]
    pub min_distance: f32,
    /// The radius of the sphere swept to find obstructions.
    ///
    /// Larger than nothing on purpose: a zero-radius ray would slip through the gap between two
    /// triangles at a chunk boundary and report open space where there is rock. It also has to
    /// exceed the near plane's half-diagonal, or geometry enters the frustum before the sphere
    /// notices it — at a 65° field of view and a near plane of 0.1 that is about 0.13.
    #[reflect(min = 0.0, max = 10.0, unit = "world units")]
    pub radius: f32,
    /// How fast the camera eases back out once nothing is in the way, in world units per second.
    ///
    /// **Only ever applies outward.** Coming *in* happens the same tick the obstruction appears,
    /// because easing that direction means spending a frame inside a hill; going back out is eased,
    /// because the sweep is noisy near an edge and a camera that snapped both ways flickered
    /// visibly. Snap in, drift out is the standard answer and it is what fixed it here.
    #[reflect(min = 0.0, max = 100.0, unit = "world units per second")]
    pub return_speed: f32,
}

impl Default for FollowCamera {
    fn default() -> Self {
        Self {
            height: 3.0,
            distance: 7.0,
            min_distance: 1.2,
            radius: 0.35,
            return_speed: 6.0,
        }
    }
}

impl amadeo_ecs::Component for FollowCamera {}

/// What a dug sample is set to.
///
/// Positive is outside the surface (air), and a couple of cells' worth of it is solidly outside
/// rather than borderline — a value just above zero would leave the surface hovering inside the hole.
const DUG: f32 = 2.0;

// --- The world's shape ---

/// The terrain of this particular world: broad hills with finer ground detail on top.
///
/// # This is content, and it lives here for that reason
///
/// ADR 0044 §2 draws the line: `amadeo-noise` ships *noise*, which is a mathematical primitive with
/// no opinion about anything, and **what a world looks like** is a game's own business. Every number
/// below is a taste decision. None of them is engine.
///
/// # Why it is a heightfield and what that costs
///
/// [`TerrainSource::sample`] returns a signed distance, and this one returns *vertical* distance to
/// the surface — `y` minus the height of the ground at that column. That is not a true distance
/// field (a steep slope makes it an overestimate), and surface nets does not mind: it only needs the
/// sign to be right and the zero crossing to be in the right place, and both are.
///
/// What it does cost is **overhangs**. One height per column means nothing can ever be above
/// anything else — no caves, no arches, no cliffs that lean. Fixing that means mixing in
/// [`amadeo_noise::Noise::sample_3d`], which is why that function exists; it is left out here so the
/// first terrain demo is one whose shape can be reasoned about while other things are being debugged.
#[derive(Debug, Clone, Copy)]
pub struct Highlands {
    /// The broad shape of the land.
    hills: Fbm,
    /// Finer bumps, so slopes are not glassy.
    detail: Fbm,
}

/// The height of the ground where nothing has raised or lowered it, in world units.
const BASE_HEIGHT: f32 = 6.0;
/// How far the hills rise above and fall below [`BASE_HEIGHT`].
const HILL_AMPLITUDE: f32 = 11.0;
/// How much the fine detail moves the surface.
const DETAIL_AMPLITUDE: f32 = 1.6;

impl Highlands {
    /// The terrain for a seed.
    ///
    /// The two layers get **different seeds** — `seed` and `seed ^ a constant` — because two `Fbm`s
    /// with the same seed and a frequency ratio of exactly two are correlated: the detail lines up
    /// with the hills and the landscape gets a regular, tiled look that is hard to attribute later.
    #[must_use]
    pub fn new(seed: u64) -> Self {
        Self {
            hills: Fbm {
                frequency: 0.012,
                octaves: 4,
                ..Fbm::new(seed)
            },
            detail: Fbm {
                frequency: 0.09,
                octaves: 3,
                ..Fbm::new(seed ^ 0x9E37_79B9_7F4A_7C15)
            },
        }
    }

    /// The height of the ground at a world column.
    ///
    /// Public because it is what a spawn point needs: putting the player at a fixed `y` and hoping
    /// works on a flat world and drops them inside a hill on this one.
    #[must_use]
    pub fn height(&self, x: f32, z: f32) -> f32 {
        BASE_HEIGHT
            + self.hills.sample_2d(x, z) * HILL_AMPLITUDE
            + self.detail.sample_2d(x, z) * DETAIL_AMPLITUDE
    }
}

impl TerrainSource for Highlands {
    fn sample(&self, x: f32, y: f32, z: f32) -> f32 {
        // Negative is inside. Below the surface is solid ground, above it is air -- getting this
        // backwards meshes the world inside out, which reads as invisible terrain rather than as a
        // sign error.
        y - self.height(x, z)
    }
}

// --- Building the game ---

/// Builds the world: the scene file, physics, the character module and the terrain streamer.
///
/// Shared by the windowed path, the headless path and every test, so an answer the agent gives about
/// this world is an answer about the game that actually runs (invariant I7).
///
/// # Errors
///
/// If the scene file will not parse or will not instantiate against the registered components, if a
/// component name is claimed twice, or if the asset directory cannot be scanned.
pub fn build_simulation() -> anyhow::Result<App> {
    build_with_workers(amadeo_terrain::default_workers())
}

/// The same world with a chosen number of meshing threads.
///
/// # This exists for exactly one test, and that test is an exit gate
///
/// M2.5's exit gate 2 is that a replay of this world reproduces across runs, processes **and thread
/// counts** — the last being the one that proves ADR 0041 rather than assuming it. Proving it needs
/// the count to be settable, and [`build_simulation`] deliberately uses whatever the machine has.
///
/// Nothing else should call this. A game choosing its own worker count is choosing how *fast* chunks
/// are meshed, and if that choice can change *what* comes out then ADR 0041 has already failed.
///
/// # Errors
///
/// Whatever [`build_simulation`] returns.
pub fn build_with_workers(workers: usize) -> anyhow::Result<App> {
    let seed = amadeo_app::requested_seed().unwrap_or(DEFAULT_SEED);
    let mut app = App::with_seed(seed);

    app.register_component::<Transform>()?;
    app.register_component::<GlobalTransform>()?;
    app.register_component::<Parent>()?;
    app.register_component::<Camera>()?;
    app.register_component::<Mesh>()?;
    app.register_component::<DirectionalLight>()?;
    app.register_component::<SortOrder>()?;
    app.register_component::<RigidBody>()?;
    app.register_component::<Collider>()?;
    app.register_component::<Velocity>()?;
    // Registered because this game ships the asset files that hold them, even though no entity
    // carries one directly. Session 9's lesson: a game whose own asset fails the validator it ships
    // with is worse than one that has no validator.
    app.register_component::<BoxMesh>()?;
    app.register_component::<PlaneMesh>()?;
    app.register_component::<Material>()?;
    app.register_component::<Environment>()?;
    app.register_component::<FollowCamera>()?;

    app.scan_assets(ASSET_DIRECTORY)?;
    app.insert_service(TextureCache::new());
    app.insert_service(Physics::new(Box::new(RapierPhysics::new())));
    app.insert_resource(Gravity::earth());

    // **Both installs, and each registers `step_physics` only if the other has not.** A game with a
    // character walking on streamed terrain is the ordinary open-world case and is the first thing
    // to call both -- which failed at startup with `DuplicateLabel { label: "step_physics" }` until
    // `App::has_system` existed. Found by building this.
    //
    // Before `load_scene` in both cases: `install` is what registers the components the scene names.
    amadeo_character::install(&mut app)?;
    amadeo_terrain::install(
        &mut app,
        Terrain::new(
            Arc::new(Highlands::new(seed)),
            TerrainSettings {
                shape: CHUNK,
                // Ground people walk up, so not ice and not glue. Matched to the Atrium's floor.
                friction: 0.8,
            },
            // Chunk meshing is what ADR 0041's job pool was built for, and it is a pure speedup:
            // the same chunks come out in the same order however many of these there are, which is
            // what `a_walk_reproduces_at_every_thread_count` requires rather than assumes.
            workers,
        )
        .with_material("turf")
        // ADR 0050: Amadeo's own content is low-poly. Surface nets gives smooth normals from the
        // field's gradient, which is the opposite of what a faceted look wants — this is what turns
        // rolling hills into folded planes. Drawn geometry only; the collider is untouched, so the
        // ground is exactly as solid as it was.
        .flat_shaded(),
    )?;

    app.add_system(
        Stage::PreSimulation,
        system(amadeo_input::SAMPLE_INPUT, amadeo_input::sample_input),
    );

    // **Before streaming, explicitly.** Systems with no declared constraint run in *alphabetical*
    // order, and `dig_terrain` happening to sort before `stream_terrain` would be a correct schedule
    // by accident -- one rename away from a dig taking a tick to appear.
    app.add_system(
        Stage::Simulation,
        system(DIG_TERRAIN, dig_terrain).before(STREAM_TERRAIN),
    );

    // **After the physics step, explicitly** (Q27). `move_shape` answers from an index `step_physics`
    // builds, so asking before it has run queries an empty world and finds open space everywhere —
    // which is this system doing nothing at all, silently. The same ordering `amadeo-character`
    // needs, for the same reason.
    app.add_system(
        Stage::Simulation,
        system(KEEP_CAMERA_OUT_OF_THE_GROUND, keep_camera_out_of_the_ground)
            .after(amadeo_physics::STEP_PHYSICS),
    );

    let document = amadeo_scene::parse(SCENE)
        .map_err(|error| anyhow::anyhow!("games/scarp/scenes/scarp.scene: {error}"))?;
    app.load_scene(&document)?;

    // Last in the tick, so the camera -- a *child* of the player -- follows this tick's movement
    // rather than last tick's.
    app.add_system(
        Stage::PostSimulation,
        system(PROPAGATE_TRANSFORMS, propagate_transforms),
    );

    Ok(app)
}

/// The same world with no window, no GPU and no keyboard — what the agent inspects.
///
/// # Errors
///
/// Whatever [`build_simulation`] returns.
pub fn build_headless() -> anyhow::Result<App> {
    let mut app = build_simulation()?;
    amadeo_input::install(&mut app.world, InputDriver::new(Box::new(NullSource)));
    Ok(app)
}

/// Carves a hollow out of the terrain under whoever is holding the dig action.
///
/// # Why this is a game system rather than an engine one
///
/// Invariant I4. *That a world can be changed* is engine — `TerrainStreamer::edit` and ADR 0042.
/// *That pressing a key removes a two-cell sphere of rock under your feet* is a game's rules: a
/// different game would want a pickaxe with a reach, a bomb with a radius, or a terraforming brush
/// with a falloff. None of that belongs below `games/`.
///
/// # It writes the resource, never the streamer
///
/// [`TerrainEdits`] is the authored truth and the streamer is a cache of it (ADR 0046). Writing the
/// streamer directly would put the hole somewhere the state hash and the save file cannot see, which
/// is exactly the defect **Q29** closed — a dug world used to reload undug.
///
/// # Determinism
///
/// An edit is a gameplay action and happens at a definite tick on every machine: the action comes
/// from [`InputState`], which is recorded and replayed, and the position comes from a `Transform`,
/// which is hashed. Turning that into sample coordinates is `floor` and integer arithmetic. Nothing
/// here consults the job pool, so a replay of a dig reproduces — and now so does a save of one.
pub fn dig_terrain(world: &mut World) {
    let digging = world
        .resource::<InputState>()
        .is_some_and(|input| input.just_pressed(ActionId::new(DIG)));
    if !digging {
        return;
    }

    // Where the diggers are. Collected before touching the service, because the query borrows the
    // world and the edit needs it mutably -- the ordinary shape in this engine.
    let feet: Vec<[f32; 3]> = world
        .query::<(&TerrainViewer, &Transform)>()
        .map(|(_, (_, transform))| transform.translation)
        .collect();

    let cell = match world.service::<Terrain>() {
        Some(terrain) => terrain.streamer.settings().shape.cell_size,
        None => return,
    };
    let Some(edits) = world.resource_mut::<TerrainEdits>() else {
        return;
    };

    for position in feet {
        // The sample the player is standing on. `floor` before the cast so a negative coordinate
        // rounds down rather than towards zero, which would put the hole one cell off on one side of
        // the origin only -- a bug that is invisible until somebody walks west.
        let centre = [
            (position[0] / cell).floor() as i32,
            (position[1] / cell).floor() as i32,
            (position[2] / cell).floor() as i32,
        ];

        // The radius in cells, from the radius in metres. At least one, so a cell coarser than the
        // dig still removes something rather than silently doing nothing.
        let radius = ((DIG_RADIUS_METRES / cell).round() as i32).max(1);

        for dx in -radius..=radius {
            for dy in -radius..=radius {
                for dz in -radius..=radius {
                    // A sphere rather than a cube, so the hole looks dug rather than cut.
                    if dx * dx + dy * dy + dz * dz > radius * radius {
                        continue;
                    }
                    // Set rather than added to, so digging the same spot twice is a no-op instead of
                    // burrowing steadily further into a value nothing reads.
                    edits.set([centre[0] + dx, centre[1] + dy, centre[2] + dz], DUG);
                }
            }
        }
    }
}

/// Pulls each [`FollowCamera`] in until nothing is between it and what it follows — **Q27**.
///
/// # Must run after `step_physics`, in the same tick
///
/// `move_shape` answers from an index that `step_physics` builds, so asking before it has run
/// queries an empty world and reports open space everywhere — which is exactly the bug this system
/// exists to fix, silently reintroduced. The same ordering `amadeo-character` documents and for the
/// same reason.
///
/// # Why it writes a local translation rather than a world position
///
/// The camera is a **child** of what it follows (ADR 0031), so its `Transform` is already in the
/// parent's space: pulling it in is one number, and the parent's own rotation carries it round as
/// the player turns. Writing a world position would fight the propagation that runs at the end of
/// the tick.
///
/// It is also safe to write, where writing a *physics body's* `Transform` is not (**Q30**) — nothing
/// steps a camera, so nothing reads it back stale and overwrites it.
pub fn keep_camera_out_of_the_ground(world: &mut World) {
    // Collected before touching the physics service, because the query borrows the world and the
    // sweep needs it mutably. The ordinary shape in this engine.
    // The current distance is collected here too, because the ease-out below needs it and the
    // physics service is borrowed mutably for the whole of the loop that does the sweeping.
    let cameras: Vec<(amadeo_ecs::Entity, FollowCamera, Transform, f32)> = world
        .query::<(&FollowCamera, &Parent, &Transform)>()
        .filter_map(|(entity, (follow, parent, transform))| {
            // The parent's own transform, which *is* its world transform: a follow camera's parent
            // is a root entity. Reading `GlobalTransform` instead would be a tick behind, because
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

    let mut results: Vec<(amadeo_ecs::Entity, Transform)> = Vec::with_capacity(cameras.len());
    for (entity, follow, parent, current) in cameras {
        // The parent's axes in world space. Column two is its local +Z, and a camera looks along its
        // own negative Z — so +Z is *behind* the thing being followed.
        let basis = amadeo_transform::Mat4::from_transform(
            parent.translation,
            parent.rotation,
            [1.0, 1.0, 1.0],
        );
        let back = [
            basis.columns[2][0],
            basis.columns[2][1],
            basis.columns[2][2],
        ];

        let pivot = [
            parent.translation[0],
            parent.translation[1] + follow.height,
            parent.translation[2],
        ];
        let wanted = [
            back[0] * follow.distance,
            back[1] * follow.distance,
            back[2] * follow.distance,
        ];

        // A sphere swept from the pivot towards where the camera wants to be. Stepping and ground
        // snapping are off: a camera does not walk, and either would pull it somewhere the geometry
        // did not ask for.
        let request = amadeo_physics::ShapeMove {
            step_height: 0.0,
            snap_distance: 0.0,
            ..amadeo_physics::ShapeMove::new(
                amadeo_physics::Shape::Sphere {
                    radius: follow.radius,
                },
                pivot,
                wanted,
            )
        };
        let landed = physics.move_shape(&request).translation;

        // **Projected onto the direction asked for, rather than measured as a straight-line
        // distance**, and that difference is what stopped the camera flickering.
        //
        // `move_shape` is a *character* move: it slides along whatever it hits, because that is what
        // a body walking into a wall should do. A camera wants the other thing — how far along this
        // one axis before something is in the way — and the engine has no pure shape cast to ask
        // (**Q34**). Measuring `|landed - pivot|` treats a slide as progress, so a camera brushing a
        // slope got a distance that had little to do with where it was pointed, and small movements
        // swung it wildly. A dot product keeps only the component that was actually asked for.
        let delta = [
            landed[0] - pivot[0],
            landed[1] - pivot[1],
            landed[2] - pivot[2],
        ];
        let along = delta[0] * back[0] + delta[1] * back[1] + delta[2] * back[2];
        let target = along.clamp(follow.min_distance, follow.distance);

        // **In at once, out slowly**, which is the other half of the flicker and the standard answer
        // to it. Geometry appearing between the camera and the player must be reacted to *this*
        // tick, or the camera spends a frame inside a hill. Geometry going away must not yank the
        // camera backwards, because the sweep result is noisy near an edge and easing turns a
        // twitch into a drift nobody notices.
        let distance = if target < current {
            target
        } else {
            (current + follow.return_speed * amadeo_core::FIXED_DT).min(target)
        };

        results.push((
            entity,
            Transform {
                translation: [0.0, follow.height, distance],
                // Rotation and scale are the scene's to author — this only ever moves the camera
                // along the one axis it is allowed to move along.
                ..Transform::default()
            },
        ));
    }

    for (entity, transform) in results {
        // The authored rotation has to survive: the Scarp's camera is pitched down 16°, and
        // replacing the whole transform would level it every tick.
        if let Some(existing) = world.get::<Transform>(entity) {
            let kept = Transform {
                translation: transform.translation,
                rotation: existing.rotation,
                scale: existing.scale,
            };
            world.insert(entity, kept);
        }
    }
}

/// The height of the ground at the origin, which is the same for **every** seed.
///
/// # Why a generated world can still author its spawn point in text
///
/// This looked like a genuine conflict with invariant I1 while the scene was being written. The
/// ground is a function of the seed, so where it *is* at the spawn column is not knowable when the
/// file is written — and computing the player's `y` in code afterwards would make the scene file
/// lie about the world it describes, which is the one thing I1 does not allow.
///
/// It resolves rather than needing a compromise, because of a property of gradient noise:
/// **it is exactly zero at every lattice point**, whatever the seed, since all eight corner offsets
/// are zero there and so is every dot product. The origin is a lattice point for both of
/// [`Highlands`]'s octaves, so `height(0, 0)` is the base height on the nose for every seed there is.
///
/// So `scarp.scene` can author a spawn height honestly, and
/// `the_spawn_column_is_at_base_height_for_every_seed` is what stops that quietly ceasing to be true
/// if the frequencies or the spawn column ever change.
#[must_use]
pub fn origin_ground_height() -> f32 {
    BASE_HEIGHT
}
