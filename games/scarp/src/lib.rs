//! **The Scarp** — M2.5's demo: a generated world, streamed in chunks, that you can walk on and dig
//! into.
//!
//! ```text
//! cargo run -p scarp
//! ```
//!
//! WASD to walk, Q and E to turn, **hold the right mouse button to steer the view**, Space to jump,
//! F to dig, Escape to quit.
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

/// The seed this world uses when `--seed` does not override it.
///
/// Public so a test can rebuild the same [`Highlands`] the running game has and ask it where the
/// ground is — which is what lets an assertion about the terrain hold for any tuning rather than
/// pinning a coordinate that a change to the camera would invalidate.
#[must_use]
pub fn default_seed() -> u64 {
    DEFAULT_SEED
}

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

    // The follow camera and its mouse control, both of which used to live in this file. Promoted to
    // `modules/` when `games/atrium` wanted them, which is the trigger this project uses rather than
    // guessing at an interface up front (Q27).
    //
    // Its own `install` declares both ordering constraints — a mouse turn before anything reads the
    // rotation, the sweep after `step_physics` — so a game does not have to know them.
    amadeo_camera::install(&mut app)?;

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
