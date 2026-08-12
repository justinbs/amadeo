//! **The Atrium** — M2's demo: a lit 3D room with shadows and someone to walk around it as.
//!
//! # What this is for
//!
//! Three of M2's exit gate 1 had been built and **none of them had ever been seen together**:
//! dynamic lighting (session 9), the character controller and shadow maps (session 10). Each was
//! proved by headless tests and single-purpose GPU captures, and by nothing else.
//!
//! That is the same position `games/vault` was built to fix in M1, and the bet paid then: a real
//! game found things about the scene format that no amount of reasoning had. So this is the
//! equivalent for 3D — the smallest room that puts a floor, walls, pillars, a sun, a shadow and a
//! character in one place.
//!
//! ```text
//! cargo run -p atrium
//! ```
//!
//! WASD to walk, Q and E to turn, Space to jump, Escape to quit.
//!
//! # Everything in it is a text file
//!
//! `scenes/atrium.scene` is the whole room. The meshes are six-line `.mesh` files carrying a
//! `BoxMesh` (ADR 0035), the materials are `.material` files (ADR 0033), and the look is an
//! `.environment` file (ADR 0034). None of them needs a toolchain, an importer or a binary format,
//! and `amadeo check games/atrium/scenes/atrium.scene` validates the lot.
//!
//! The follow camera is the part worth noticing: it is a **child entity of the player** in the scene
//! file and nothing else. ADR 0031 said a camera parented to a character *is* a follow camera with
//! no special case, and this is that claim being cashed rather than repeated.

use amadeo_app::{App, Stage, system};

use amadeo_audio::{Audio, AudioListener, AudioSource, COLLECT_AUDIO, SoundCache, collect_audio};
use amadeo_input::{InputDriver, NullSource};
use amadeo_physics::{Collider, Gravity, Physics, RapierPhysics, RigidBody, Velocity};
use amadeo_render::{
    BoxMesh, Camera, DirectionalLight, Environment, Material, Mesh, PlaneMesh, PointLight,
    SortOrder, SpotLight, TextureCache,
};
use amadeo_transform::{
    GlobalTransform, PROPAGATE_TRANSFORMS, Parent, Transform, propagate_transforms,
};

/// Where this game's assets live, relative to the project root (ADR 0022).
const ASSET_DIRECTORY: &str = "games/atrium/assets";

/// The room, as text. Compiled in rather than read at runtime, so the binary carries its own level
/// and a capture cannot silently be taken against an edited one — the same choice `games/vault`
/// made. `amadeo check` validates the same text against the real schema.
const SCENE: &str = include_str!("../scenes/atrium.scene");

/// The seed, when nothing overrides it.
const DEFAULT_SEED: u64 = 0x4154_5249_554d;

/// Builds the room: the scene file, physics, and the character module.
///
/// Shared by the windowed path, the headless path, and every test — so an answer the agent gives
/// about this world is an answer about the game that actually runs (invariant I7).
///
/// # Errors
///
/// If the scene file will not parse or will not instantiate against the registered components, if a
/// component name is claimed twice, or if the asset directory cannot be scanned.
pub fn build_simulation() -> anyhow::Result<App> {
    let mut app = App::with_seed(amadeo_app::requested_seed().unwrap_or(DEFAULT_SEED));

    // Registration is what puts a type into `amadeo describe` and lets the scene file name it
    // (invariant I8). Engine components and the module's, together — the schema describes *this
    // game*, and everything the room authors has to be here or the scene will not load.
    app.register_component::<Transform>()?;
    app.register_component::<GlobalTransform>()?;
    app.register_component::<Parent>()?;
    app.register_component::<Camera>()?;
    app.register_component::<Mesh>()?;
    app.register_component::<DirectionalLight>()?;
    // Point and spot lights (ADR 0057). Registered here rather than only where used, so a scene file
    // can name one and `amadeo check` can validate it.
    app.register_component::<PointLight>()?;
    app.register_component::<SpotLight>()?;
    app.register_component::<SortOrder>()?;
    // Sound (ADR 0059). The ears go on the *camera* here rather than on the character, which is a
    // real choice with an audible difference: this is third person, so what the viewer can see is
    // what they should hear. A horror game would put them on the character instead.
    app.register_component::<AudioSource>()?;
    app.register_component::<AudioListener>()?;
    app.register_component::<RigidBody>()?;
    app.register_component::<Collider>()?;
    app.register_component::<Velocity>()?;
    // Registered because this game *ships* the asset files that hold them, even though no entity
    // carries one directly. Session 9's lesson: a game whose own asset fails the validator it ships
    // with is worse than one that has no validator.
    app.register_component::<BoxMesh>()?;
    app.register_component::<PlaneMesh>()?;
    app.register_component::<Material>()?;
    app.register_component::<Environment>()?;

    app.scan_assets(ASSET_DIRECTORY)?;
    app.insert_service(TextureCache::new());
    app.insert_service(SoundCache::new());

    // **`NullAudio` here, and the windowed build swaps in kira.** The same split the renderer has:
    // `build_simulation` is what the agent, the tests and CI run, and none of them has a sound card.
    // `main.rs` replaces this service when it opens a window.
    app.insert_service(Audio::headless());

    // In `Render`, beside the renderer's collection pass and outside the deterministic zone.
    // Nothing it does can move the state hash -- `Audio` is a Service, and ADR 0009 excludes those
    // by trait bound -- so where it sits is about what it can *see*, not about safety: it must run
    // after `propagate_transforms` so a sound attached to a moving thing is where the thing ended
    // up, and `Render` is after `PostSimulation`.
    app.add_system(Stage::Render, system(COLLECT_AUDIO, collect_audio));

    // Rapier rather than `NullPhysics`. Against the null backend the character walks through the
    // walls — which is a deliberate and useful control case in a test, and a broken demo here.
    app.insert_service(Physics::new(Box::new(RapierPhysics::new())));
    app.insert_resource(Gravity::earth());

    // **Before `load_scene`, and that matters.** `install` registers `CharacterController` and
    // `CharacterMotion` as well as wiring the systems, and the scene file names both — so calling it
    // after the scene loads fails with "no component named `CharacterController` is registered".
    // Written the wrong way round first; the error said exactly what was wrong, including "if it
    // belongs to a module, that module may not be loaded", which is the message earning its keep.
    //
    // It also registers `step_physics` and the character system **after** it, which is the ordering
    // ADR 0037 calls load-bearing: the move-and-slide query reads a spatial index the step builds,
    // so asking first would query an empty one on tick 1 and walk through the level exactly once.
    amadeo_character::install(&mut app)?;

    // **Q27's original case, which was about walls rather than terrain.** A follow camera six units
    // behind a character in a room this size ends up inside a wall in most corners, and a camera
    // inside geometry sees the far side of the world through it. The module was written for
    // `games/scarp` and moved here the moment a second game wanted it.
    amadeo_camera::install(&mut app)?;

    // Input is sampled before anything reads it, which is what `PreSimulation` is for. The character
    // module reads named actions rather than keys, so this is the only place in the game that knows
    // input exists at all.
    app.add_system(
        Stage::PreSimulation,
        system(amadeo_input::SAMPLE_INPUT, amadeo_input::sample_input),
    );

    let document = amadeo_scene::parse(SCENE)
        .map_err(|error| anyhow::anyhow!("games/atrium/scenes/atrium.scene: {error}"))?;
    app.load_scene(&document)?;

    // Last in the tick, so the composed transforms reflect where everything finally ended up — and
    // so the camera, which is a *child* of the player, follows this tick's movement rather than
    // last tick's.
    //
    // No `.after(DRIVE_CHARACTERS)` here, and that is not an omission: ordering constraints resolve
    // **within a stage**, and the character runs in `Simulation` while this runs in
    // `PostSimulation`, which already comes after. Written with the constraint first, and the
    // schedule refused it by name — `UnknownLabel { missing: "drive_characters" }`.
    app.add_system(
        Stage::PostSimulation,
        system(PROPAGATE_TRANSFORMS, propagate_transforms),
    );

    Ok(app)
}

/// The same room with no window, no GPU, and no keyboard — what the agent inspects.
///
/// # Errors
///
/// Whatever [`build_simulation`] returns.
pub fn build_headless() -> anyhow::Result<App> {
    let mut app = build_simulation()?;
    amadeo_input::install(&mut app.world, InputDriver::new(Box::new(NullSource)));
    Ok(app)
}
