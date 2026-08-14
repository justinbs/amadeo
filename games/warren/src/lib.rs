//! **The Warren** — M3's exit gate, in progress: a first-person atmospheric horror slice.
//!
//! ```text
//! cargo run -p warren
//! ```
//!
//! WASD to walk, the mouse to look, F to take what is in front of you. The torch is on the near
//! crate; picking it up lights the beam.
//!
//! # What exists so far, and what does not
//!
//! **One handcrafted room, and the spine that every later version needs.** `docs/05`'s exit gate
//! asks for bounded procedural interiors assembled from handcrafted room pieces, a pursuing entity,
//! a win and a lose state, and a title screen. None of that is here yet. What *is* here is the
//! vertical slice the rest hangs off — the thing `CLAUDE.md` asks for in preference to a complete
//! horizontal layer.
//!
//! # Why this room exists now rather than after the level design
//!
//! Two modules had never been used by a game, which is the "designed against zero users" risk this
//! project keeps naming. `games/atrium` retired that for `amadeo-interaction`. This retires it for
//! **`FirstPersonCamera`**, which has been built since session 17 with no game behind it — the
//! Scarp and the Atrium are both third person.
//!
//! It also cashes the thing session 18 got wrong twice over. An `Interactor` sweeps along its own
//! forward, so aiming it is a matter of where it is parented: here it sits on the **camera**, and
//! the mouse drives the pitch. An authored angle can reach the floor, but only at one angle; this
//! reaches whatever you are looking at, which is what a first-person game means by interaction.
//!
//! # Everything in it is a text file
//!
//! `scenes/warren.scene` is the whole room — walls, ceiling, crates, the torch, the player and the
//! camera. `amadeo check games/warren/scenes/warren.scene` validates the lot.

use amadeo_app::{App, Stage, system};
use amadeo_character::{CharacterController, CharacterMotion};
use amadeo_ecs::{Entity, World};
use amadeo_events::WorldEvents;
use amadeo_input::{InputDriver, NullSource};
use amadeo_interaction::{Interactable, Interacted, Interactor, Looking};
use amadeo_inventory::{Inventory, Item, StoredIn};
use amadeo_physics::{Collider, Gravity, Physics, RapierPhysics, RigidBody, Velocity};
use amadeo_render::{
    BoxMesh, Camera, Environment, Material, Mesh, PlaneMesh, PointLight, SpotLight, TextureCache,
};
use amadeo_transform::{
    GlobalTransform, PROPAGATE_TRANSFORMS, Parent, Transform, propagate_transforms,
};

/// Where this game's assets live, relative to the project root (ADR 0022).
const ASSET_DIRECTORY: &str = "games/warren/assets";

/// The scene the game starts in, compiled in so the binary runs from any directory.
const SCENE: &str = include_str!("../scenes/warren.scene");

/// A fixed seed, so two runs of the same inputs are the same run.
const DEFAULT_SEED: u64 = 0x7761_7272_656e_0001;

/// The item id the torch is authored with. A game-level name: the engine has no idea what a torch is.
pub const TORCH: &str = "torch";

/// How bright the beam burns once it is in your hand.
///
/// **An eyeball number**, and the one that decides whether the room reads as dark-but-navigable or
/// as a black screen with a white circle in it. The scene authors `intensity 0.0` and this replaces
/// it, so "off" and "on" are one number in one place rather than two lights.
pub const BEAM_INTENSITY: f32 = 24.0;

/// The label [`carry_the_torch`] is registered under.
pub const CARRY_THE_TORCH: &str = "carry_the_torch";

/// The label [`take_what_you_used`] is registered under.
pub const TAKE_WHAT_YOU_USED: &str = "take_what_you_used";

/// Picks up anything usable that turns out to be an item.
///
/// The same one-sentence join `games/atrium` writes, and deliberately still a *game's* sentence
/// rather than an engine feature: `amadeo-interaction` decides what you are pointing at and
/// `amadeo-inventory` decides what carrying means, and neither knows the other exists.
///
/// The carrier walk is shorter here than in the Atrium because the interactor is on the camera,
/// which is a direct child of the player — but it is the same walk, and writing it out rather than
/// assuming one level is what will still be right when a hand or a helmet goes in between.
pub fn take_what_you_used(world: &mut World) {
    let used: Vec<Interacted> = world
        .read_events::<Interacted>()
        .iter()
        .map(|record| record.event)
        .collect();

    for event in used {
        if world.get::<Item>(event.target).is_none() {
            continue;
        }
        let Some(carrier) = carrier_of(world, event.interactor) else {
            continue;
        };
        let _ = amadeo_inventory::store(world, event.target, carrier);
    }
}

/// The nearest thing at or above `interactor` that can hold something.
fn carrier_of(world: &World, interactor: Entity) -> Option<Entity> {
    let mut current = interactor;
    // Bounded like `propagate_transforms`, for the same reason: a hierarchy deep enough to loop is
    // indistinguishable from one that does.
    for _ in 0..16 {
        if world.get::<Inventory>(current).is_some() {
            return Some(current);
        }
        current = world.get::<Parent>(current)?.0;
    }
    None
}

/// Lights the beam while the torch is in your inventory, and puts it out when it is not.
///
/// # Why the beam is authored and switched rather than spawned
///
/// The `SpotLight` is a permanent child of the camera in the scene file, starting at
/// `intensity 0.0`. Spawning one on pickup would work and would be worse in two specific ways:
/// spawning is entity-allocator state, so a game that picked a torch up and dropped it repeatedly
/// would churn handles that a snapshot has to reproduce exactly (ADR 0028); and the beam's *place*
/// on the camera is a look decision, which belongs in the scene file where somebody can see it (I1).
///
/// Written every tick from what is in the bag, so there is no "torch state" to get out of step with
/// the inventory — the same reason `games/atrium` projects `Screen` onto `Paused` every tick rather
/// than toggling both.
pub fn carry_the_torch(world: &mut World) {
    let carried: Vec<Entity> = world
        .query::<(&Inventory,)>()
        .map(|(entity, _)| entity)
        .collect();

    let holding = carried
        .iter()
        .any(|carrier| amadeo_inventory::count_of(world, *carrier, TORCH) > 0);

    let beams: Vec<Entity> = world
        .query::<(&SpotLight,)>()
        .map(|(entity, _)| entity)
        .collect();
    for beam in beams {
        if let Some(light) = world.get_mut::<SpotLight>(beam) {
            light.intensity = if holding { BEAM_INTENSITY } else { 0.0 };
        }
    }
}

/// The room, with no window and no GPU — what the agent and the tests drive.
///
/// # Errors
///
/// If a component fails to register, the assets will not scan, or the scene will not load.
pub fn build_simulation() -> anyhow::Result<App> {
    let mut app = App::with_seed(amadeo_app::requested_seed().unwrap_or(DEFAULT_SEED));

    // Everything the scene file names has to be registered before it loads, or the load fails with
    // "no component named ... is registered" (invariant I8).
    app.register_component::<Transform>()?;
    app.register_component::<GlobalTransform>()?;
    app.register_component::<Parent>()?;
    app.register_component::<Camera>()?;
    app.register_component::<Mesh>()?;
    app.register_component::<amadeo_render::DirectionalLight>()?;
    app.register_component::<PointLight>()?;
    app.register_component::<SpotLight>()?;
    app.register_component::<RigidBody>()?;
    app.register_component::<Collider>()?;
    app.register_component::<Velocity>()?;
    // Registered because this game *ships* the asset files holding them, even though no entity
    // carries one directly — a game whose own assets fail the validator it ships with is worse than
    // one with no validator.
    app.register_component::<BoxMesh>()?;
    app.register_component::<PlaneMesh>()?;
    app.register_component::<Material>()?;
    app.register_component::<Environment>()?;

    app.scan_assets(ASSET_DIRECTORY)?;
    app.insert_service(TextureCache::new());

    app.insert_service(Physics::new(Box::new(RapierPhysics::new())));
    app.insert_resource(Gravity::earth());

    // **Before `load_scene`**, all three: each registers the components its own scene lines name.
    amadeo_character::install(&mut app)?;
    amadeo_camera::install(&mut app)?;
    amadeo_interaction::install(&mut app)?;
    amadeo_inventory::install(&mut app)?;

    // Input is sampled before anything reads it, which is what `PreSimulation` is for. Both the
    // character and the camera read *named actions*, so this is the only place in the game that
    // knows a keyboard or a mouse exists.
    app.add_system(
        Stage::PreSimulation,
        system(amadeo_input::SAMPLE_INPUT, amadeo_input::sample_input),
    );

    let document = amadeo_scene::parse(SCENE)
        .map_err(|error| anyhow::anyhow!("games/warren/scenes/warren.scene: {error}"))?;
    app.load_scene(&document)?;

    // Last in the simulation, so composed transforms reflect where everything finally ended up —
    // and so the camera, a *child* of the player, follows this tick's movement rather than last
    // tick's. Everything below reads a `GlobalTransform`, so all of it comes after.
    app.add_system(
        Stage::PostSimulation,
        system(PROPAGATE_TRANSFORMS, propagate_transforms),
    );

    // After the interaction system, which is what raises the event this reads. Same stage, one step
    // later, so a key pressed this tick is acted on this tick.
    app.add_system(
        Stage::PostSimulation,
        system(TAKE_WHAT_YOU_USED, take_what_you_used)
            .after(amadeo_interaction::UPDATE_INTERACTIONS),
    );
    // And after that, so the beam reflects what is in the bag on the tick it arrived rather than a
    // tick later — a torch that lights one frame after you take it reads as a bug.
    app.add_system(
        Stage::PostSimulation,
        system(CARRY_THE_TORCH, carry_the_torch).after(TAKE_WHAT_YOU_USED),
    );

    Ok(app)
}

/// The same room with no keyboard either.
///
/// # Errors
///
/// Whatever [`build_simulation`] returns.
pub fn build_headless() -> anyhow::Result<App> {
    let mut app = build_simulation()?;
    amadeo_input::install(&mut app.world, InputDriver::new(Box::new(NullSource)));
    Ok(app)
}

/// The player entity, for tests and for the window layer.
#[must_use]
pub fn player(world: &World) -> Option<Entity> {
    world
        .query::<(&CharacterController,)>()
        .map(|(entity, _)| entity)
        .next()
}

/// The camera entity, which is also what does the looking.
#[must_use]
pub fn eyes(world: &World) -> Option<Entity> {
    world
        .query::<(&Interactor,)>()
        .map(|(entity, _)| entity)
        .next()
}

/// What the player is currently able to use, and what it would say.
///
/// The prompt a HUD would draw. Returned rather than drawn because there is no HUD yet, and because
/// it is the one thing a test can assert about "does interaction feel connected".
#[must_use]
pub fn prompt(world: &World) -> Option<String> {
    let looking = world.get::<Looking>(eyes(world)?)?;
    let target = looking.at?;
    world
        .get::<Interactable>(target)
        .map(|interactable| interactable.prompt.clone())
}

/// Whether the torch is in the player's hands.
#[must_use]
pub fn holding_torch(world: &World) -> bool {
    player(world).is_some_and(|carrier| amadeo_inventory::count_of(world, carrier, TORCH) > 0)
}

/// Whether an entity is out of the world — stored rather than lying about (ADR 0070).
#[must_use]
pub fn is_stored(world: &World, entity: Entity) -> bool {
    world.get::<StoredIn>(entity).is_some()
}

/// Everything registered as a character motion, so a test can assert the player is standing.
#[must_use]
pub fn grounded(world: &World) -> bool {
    player(world)
        .and_then(|entity| world.get::<CharacterMotion>(entity))
        .is_some_and(|motion| motion.grounded)
}

/// The label the propagation system runs under, re-exported so `main.rs` need not import transform.
pub const PROPAGATE: &str = PROPAGATE_TRANSFORMS;
