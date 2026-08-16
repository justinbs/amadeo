//! **The Warren** — M3's exit gate, in progress: a first-person atmospheric horror slice.
//!
//! ```text
//! cargo run -p warren
//! ```
//!
//! WASD to walk, the mouse to look, F to use what is in front of you. The torch is one room away;
//! picking it up lights the beam.
//!
//! # What exists so far, and what does not
//!
//! **A generated interior with a loop you can win and lose.** Wake up, find the torch next door,
//! find the key, reach the door — and the warden is looking for you. That is `docs/05`'s exit gate
//! items 1 (a playable loop with a win and a lose state), 2 (bounded procedural interiors) and 3 (a
//! pursuing entity with distinct AI states, driven by `mod-behaviour`).
//!
//! A HUD says what is in reach and how the run ended, authored in the scene like everything else.
//!
//! **Still missing**: a title screen, audio, and the atmosphere pass. Save and resume are not wired
//! up either; `games/atrium` proves that mechanism and this game has not needed it.
//!
//! # A level is a graph, a set of landmarks, and a pile of pieces
//!
//! `lay_out` picks the rooms and the doors between them, then chooses five [`Landmarks`] out of the
//! graph — where you start, where the way out is, and where the key, the torch and the warden go.
//! `to_scene` renders all of that as text. Every entity it emits is an instance of a **piece** in
//! `assets/pieces/`, so moving a lamp is an edit to `room_lamp.scene` and reaches every level ever
//! generated.
//!
//! The generator writes a file and stops (ADR 0071 §1), so the level the game plays is
//! `scenes/generated.scene` — committed, diffable, and editable by hand. `--bin layout` rewrites it.
//!
//! # Why the handcrafted room still exists
//!
//! `scenes/warren.scene` is where every content piece was cut from, and it is now a *second user*
//! for each of them: a change to `way_out.scene` that breaks a level shows up in two scenes rather
//! than one. It is also the room whose coordinates are written down, which is why the tests about
//! the game's **rules** play it and only `the_level_is_a_level.rs` plays the generated one.
//!
//! # Why this game exists at all
//!
//! Two modules had never been used by a game, which is the "designed against zero users" risk this
//! project keeps naming. `games/atrium` retired that for `amadeo-interaction`. This retires it for
//! **`FirstPersonCamera`**, which had been built since session 17 with no game behind it — the
//! Scarp and the Atrium are both third person.
//!
//! It also cashes the thing session 18 got wrong twice over. An `Interactor` sweeps along its own
//! forward, so aiming it is a matter of where it is parented: here it sits on the **camera**, and
//! the mouse drives the pitch. An authored angle can reach the floor, but only at one angle; this
//! reaches whatever you are looking at, which is what a first-person game means by interaction.
//!
//! # Everything in it is a text file
//!
//! `amadeo check -p warren games/warren/scenes/generated.scene` validates the generated level, and
//! `amadeo capture -p warren --ticks 5` draws it. **`check` is not a load** — it says nothing about
//! whether the floor is under the player, which is why `the_level_is_a_level.rs` stands on it.

use amadeo_app::{App, Stage, system};
use amadeo_behaviour::{Behaviour, Facts};
use amadeo_character::{CharacterController, CharacterMotion};
use amadeo_core::StableHash;
use amadeo_ecs::{Component, Entity, Resource, World};
use amadeo_events::WorldEvents;
use amadeo_input::{InputDriver, NullSource};
use amadeo_interaction::{Interactable, Interacted, Interactor, Looking};
use amadeo_inventory::{Inventory, Item, StoredIn};
use amadeo_physics::{Collider, Gravity, Physics, RapierPhysics, RigidBody, Velocity};
use amadeo_reflect::Reflect;
use amadeo_render::{
    BoxMesh, Camera, Environment, Material, Mesh, PlaneMesh, PointLight, SpotLight, TextureCache,
};
use amadeo_transform::{
    GlobalTransform, PROPAGATE_TRANSFORMS, Parent, Transform, propagate_transforms,
};
use amadeo_ui::{
    COLLECT_UI, ComputedRect, FontCache, LAYOUT_UI, Text, UiNode, collect_ui, layout_ui_system,
};

/// Where this game's assets live, relative to the project root (ADR 0022).
const ASSET_DIRECTORY: &str = "games/warren/assets";

/// The level the game plays: a generated interior, committed as text (ADR 0071).
///
/// Compiled in so the binary runs from any directory. Rewrite it with
/// `cargo run -p warren --bin layout` and the change arrives in a reviewable diff, which is the
/// whole reason a generated level is a file.
pub const GENERATED_SCENE: &str = include_str!("../scenes/generated.scene");

/// The seed [`GENERATED_SCENE`] was made from.
///
/// Recorded here as well as in the scene's own name so that a test can regenerate the shipped level
/// and compare — which is what stops the committed file drifting away from the generator that is
/// supposed to produce it.
pub const GENERATED_SEED: u64 = 20_250_815;

/// How many rooms [`GENERATED_SCENE`] was asked for. The generator may add one to close a loop.
pub const GENERATED_ROOMS: usize = 14;

/// The one handcrafted room, which is no longer what the game boots into.
///
/// Kept, and worth keeping. It is where every content piece was cut from, it is the only place the
/// lighting and the spacing were tuned by eye, and it is a second user for each piece — so a change
/// to `way_out.scene` that breaks a level shows up in two scenes rather than one. `build_handcrafted`
/// loads it.
pub const HANDCRAFTED_SCENE: &str = include_str!("../scenes/warren.scene");

/// A fixed seed, so two runs of the same inputs are the same run.
const DEFAULT_SEED: u64 = 0x7761_7272_656e_0001;

/// The item id the torch is authored with. A game-level name: the engine has no idea what a torch is.
pub const TORCH: &str = "torch";

/// How bright the beam burns once it is in your hand.
///
/// **An eyeball number**, and the one that decides whether the room reads as dark-but-navigable or
/// as a black screen with a white circle in it. The scene authors `intensity 0.0` and this replaces
/// it, so "off" and "on" are one number in one place rather than two lights.
pub const BEAM_INTENSITY: f32 = 30.0;

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

/// The generated Warren, with no window and no GPU — what the agent and the tests drive.
///
/// # Errors
///
/// If a component fails to register, the assets will not scan, or the scene will not load.
pub fn build_simulation() -> anyhow::Result<App> {
    build_from_scene(GENERATED_SCENE)
}

/// The one handcrafted room instead, for tests about the *rules* rather than about the level.
///
/// # Errors
///
/// Whatever [`build_from_scene`] returns.
pub fn build_handcrafted() -> anyhow::Result<App> {
    build_from_scene(HANDCRAFTED_SCENE)
}

/// Builds the game around whichever scene text it is handed.
///
/// Both levels need exactly the same registrations, systems and services — they differ only in
/// their geometry — so there is one of this and two callers rather than two of everything. A second
/// copy is how a generated level ends up missing a system nobody noticed the handcrafted one had.
///
/// # Errors
///
/// If a component fails to register, the assets will not scan, or the scene will not load.
pub fn build_from_scene(scene: &str) -> anyhow::Result<App> {
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

    // **Before `load_scene`**, all of them: each registers the components its own scene lines name,
    // and a component the registry has not heard of stops the scene loading.
    amadeo_character::install(&mut app)?;
    amadeo_camera::install(&mut app)?;
    amadeo_interaction::install(&mut app)?;
    amadeo_inventory::install(&mut app)?;
    amadeo_behaviour::install(&mut app)?;

    // This game's own marks, and the one thing it can end as.
    app.register_component::<WayOut>()?;
    app.register_component::<Warden>()?;
    app.register_component::<Socket>()?;
    app.register_component::<PromptLine>()?;
    app.register_component::<EndingLine>()?;
    app.insert_resource(Outcome::default());

    // The interface (ADR 0062). `ComputedRect` is registered although nothing authors one, because
    // it is a component an agent should be able to *see* — "where did that line end up" is the
    // question `world.entity` exists to answer.
    app.register_component::<UiNode>()?;
    app.register_component::<ComputedRect>()?;
    app.register_component::<Text>()?;

    // **Layout before collection, and the ordering is load-bearing**: `collect_ui` reads the
    // rectangles `layout_ui_system` writes, so the other way round draws an empty interface on the
    // first frame and a one-frame-stale one forever after.
    //
    // No `Theme` asset — this game ships none, so the built-in Signage look draws it. That is
    // `TextureCache`'s argument again: a last resort that is itself a file cannot cover the case
    // where files are the problem.
    app.insert_service(FontCache::new());
    app.insert_service(amadeo_render::Overlay::default());

    // Input is sampled before anything reads it, which is what `PreSimulation` is for. Both the
    // character and the camera read *named actions*, so this is the only place in the game that
    // knows a keyboard or a mouse exists.
    app.add_system(
        Stage::PreSimulation,
        system(amadeo_input::SAMPLE_INPUT, amadeo_input::sample_input),
    );

    let document = amadeo_scene::parse(scene)
        .map_err(|error| anyhow::anyhow!("games/warren/scenes/: {error}"))?;
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
    // Same tick as the pickup, for the same reason: using the door on the tick the key reached your
    // pocket has to work, or the game looks like it ignored you.
    app.add_system(
        Stage::PostSimulation,
        system(TRY_THE_DOOR, try_the_door).after(TAKE_WHAT_YOU_USED),
    );
    app.add_system(
        Stage::PostSimulation,
        system(LABEL_THE_DOOR, label_the_door).after(TAKE_WHAT_YOU_USED),
    );

    // The warden. Perception **before** the machine and action **after** it, which is ADR 0068's
    // boundary: the module sequences states and this game supplies both sides of it.
    app.add_system(
        Stage::Simulation,
        system(WATCH_FOR_YOU, watch_for_you).before(amadeo_behaviour::RUN_BEHAVIOURS),
    );
    app.add_system(
        Stage::Simulation,
        system(MOVE_THE_WARDEN, move_the_warden).after(amadeo_behaviour::RUN_BEHAVIOURS),
    );
    // Last, so it judges where everything actually ended up this tick rather than where it was.
    app.add_system(
        Stage::PostSimulation,
        system(SETTLE_THE_RUN, settle_the_run).after(PROPAGATE_TRANSFORMS),
    );
    // After the run is settled, so the ending appears on the tick it happens rather than the next.
    app.add_system(
        Stage::PostSimulation,
        system(WRITE_THE_HUD, write_the_hud).after(SETTLE_THE_RUN),
    );

    // Drawn in `Render`, where nothing it does can reach the state hash. What the lines *say* was
    // decided above, in the deterministic zone.
    app.add_system(Stage::Render, system(LAYOUT_UI, layout_ui_system));
    app.add_system(
        Stage::Render,
        system(COLLECT_UI, collect_ui).after(LAYOUT_UI),
    );

    Ok(app)
}

/// The same level with no keyboard either.
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

// --- The loop: a key, a door, and something that wants you not to reach it -----------------------

/// The item id the key is authored with.
pub const KEY: &str = "key";

/// How the run ended, or that it has not.
///
/// # Why this is the game's and not the engine's
///
/// M3's exit gate asks for a win state and a lose state, and **what those mean is genre knowledge** —
/// invariant I4 one level up, the same split `games/atrium` draws with its `Screen`. The engine has
/// no notion of winning, and this room's answer would be wrong for the next one.
///
/// An ordinary reflected resource, so it is hashed, a snapshot restores it, and `amadeo query` can
/// read it. None of that had to be built.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, StableHash, Reflect)]
pub enum Outcome {
    /// Still going.
    #[default]
    Playing,
    /// Out through the door with the key. The win.
    Escaped,
    /// The warden reached you. The lose.
    Caught,
}

impl Resource for Outcome {}

/// Marks the door as the way out. A game-level fact: the engine sees an `Interactable` like any
/// other.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, StableHash, Reflect)]
pub struct WayOut;

impl Component for WayOut {}

/// Marks the thing that hunts you.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, StableHash, Reflect)]
pub struct Warden;

impl Component for Warden {}

/// How far the warden can see you, in metres. An eyeball number.
pub const WARDEN_SIGHT: f32 = 9.0;

/// How fast it moves, in metres per second.
///
/// **Slower than you**, deliberately — you walk at 2.6. A chase you cannot win is a cutscene.
pub const WARDEN_SPEED: f32 = 1.9;

/// How close it has to get to catch you, in metres.
pub const WARDEN_REACH: f32 = 0.9;

/// The label [`watch_for_you`] is registered under.
pub const WATCH_FOR_YOU: &str = "watch_for_you";

/// The label [`move_the_warden`] is registered under.
pub const MOVE_THE_WARDEN: &str = "move_the_warden";

/// The label [`settle_the_run`] is registered under.
pub const SETTLE_THE_RUN: &str = "settle_the_run";

/// The label [`try_the_door`] is registered under.
pub const TRY_THE_DOOR: &str = "try_the_door";

/// The label [`label_the_door`] is registered under.
pub const LABEL_THE_DOOR: &str = "label_the_door";

/// Where the player is, if there is one.
fn player_at(world: &World) -> Option<[f32; 3]> {
    let player = player(world)?;
    Some(world.get::<Transform>(player)?.translation)
}

/// Straight-line distance between two places.
fn distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    let d = [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
}

/// Writes the one fact the warden's machine reads.
///
/// **This is the whole of what the game owes `modules/amadeo-behaviour`.** The module sequences
/// named states and knows nothing about sight, distance or players; the machine in `warren.scene`
/// names `"sees_you"` and this decides what that means.
///
/// Runs *before* the machine, so a decision is made on this tick's facts rather than last tick's —
/// which the module cannot declare for itself, because it cannot see this game's system names.
pub fn watch_for_you(world: &mut World) {
    let Some(you) = player_at(world) else {
        return;
    };
    let seen: Vec<(Entity, bool)> = world
        .query::<(&Warden, &Transform)>()
        .map(|(entity, (_, at))| (entity, distance(at.translation, you) <= WARDEN_SIGHT))
        .collect();

    for (entity, sees) in seen {
        if let Some(facts) = world.get_mut::<Facts>(entity) {
            facts.set("sees_you", sees);
        }
    }
}

/// Acts on whatever state the machine settled into.
///
/// The other half of that boundary: the module says *which* state, and this says what the state
/// does. Swapping the sequencer for a behaviour tree later would replace neither this nor
/// [`watch_for_you`], which is ADR 0068's whole argument.
///
/// # It has no collider, and that is stated rather than hidden
///
/// The warden walks through crates. Giving it one means building a second character controller to
/// prove a point about pursuit, which `games/atrium`'s watcher declined for the same reason. The
/// room is open enough that it does not read as broken, and it is the first thing to fix if the
/// pursuit ever needs to feel fair.
pub fn move_the_warden(world: &mut World) {
    let Some(you) = player_at(world) else {
        return;
    };

    let moving: Vec<(Entity, [f32; 3])> = world
        .query::<(&Warden, &Behaviour, &Transform)>()
        .filter(|(_, (_, mind, _))| mind.state == "pursue")
        .map(|(entity, (_, _, at))| (entity, at.translation))
        .collect();

    for (entity, at) in moving {
        let gap = distance(at, you);
        if gap <= f32::EPSILON {
            continue;
        }
        let step = WARDEN_SPEED * amadeo_core::FIXED_DT;
        // Horizontal only, so it walks the floor rather than swimming towards your eyes.
        let toward = [(you[0] - at[0]) / gap, (you[2] - at[2]) / gap];
        if let Some(transform) = world.get_mut::<Transform>(entity) {
            transform.translation[0] += toward[0] * step;
            transform.translation[2] += toward[1] * step;
        }
    }
}

/// Decides whether the run is over, and how.
///
/// Both endings are settled here rather than where each is caused, so "how can this run end" is one
/// list in one place. **A run that has already ended is left alone** — being caught after escaping
/// would be a memorable bug, and it is one tick away without this.
pub fn settle_the_run(world: &mut World) {
    if outcome(world) != Outcome::Playing {
        return;
    }
    let Some(you) = player_at(world) else {
        return;
    };

    let caught = world
        .query::<(&Warden, &Transform)>()
        .any(|(_, (_, at))| distance(at.translation, you) <= WARDEN_REACH);

    if caught && let Some(settled) = world.resource_mut::<Outcome>() {
        *settled = Outcome::Caught;
    }
}

/// Opens the way out when somebody uses it while carrying the key.
///
/// Reads the same `Interacted` event [`take_what_you_used`] does and runs after it, so using the
/// door on the very tick you picked the key up works. A one-tick edge there reads as the game
/// ignoring you.
pub fn try_the_door(world: &mut World) {
    if outcome(world) != Outcome::Playing {
        return;
    }
    let used: Vec<Interacted> = world
        .read_events::<Interacted>()
        .iter()
        .map(|record| record.event)
        .collect();

    for event in used {
        if world.get::<WayOut>(event.target).is_none() {
            continue;
        }
        let Some(carrier) = carrier_of(world, event.interactor) else {
            continue;
        };
        if amadeo_inventory::count_of(world, carrier, KEY) == 0 {
            continue;
        }
        if let Some(settled) = world.resource_mut::<Outcome>() {
            *settled = Outcome::Escaped;
        }
    }
}

/// Keeps the door's prompt honest about whether it will open.
///
/// A **field on the component rather than two doors**: `Interactable::enabled` exists for exactly
/// this reason, and a locked door that becomes unlocked must not move between archetypes to say so.
///
/// The locked wording lives in `warren.scene` where content belongs; the unlocked one is here, which
/// is the single place this game puts player-facing words in code. Worth noticing as a small wart —
/// it wants a second `Interactable` field, or a game-side lookup, once there is a second door.
pub fn label_the_door(world: &mut World) {
    let Some(carrier) = player(world) else {
        return;
    };
    let has_key = amadeo_inventory::count_of(world, carrier, KEY) > 0;
    let wanted = if has_key {
        "Unlock the door and leave"
    } else {
        "The door is locked"
    };

    let doors: Vec<Entity> = world
        .query::<(&WayOut,)>()
        .map(|(entity, _)| entity)
        .collect();
    for door in doors {
        if let Some(interactable) = world.get_mut::<Interactable>(door)
            && interactable.prompt != wanted
        {
            interactable.prompt = wanted.to_string();
        }
    }
}

/// How this run ended, or that it has not.
#[must_use]
pub fn outcome(world: &World) -> Outcome {
    world.resource::<Outcome>().copied().unwrap_or_default()
}

// --- Room pieces (ADR 0071) ---------------------------------------------------------------------

/// A place on a room piece where another piece may join.
///
/// # Authored, never inferred
///
/// ADR 0071 §2. A generator could guess at doorways from bounding boxes, and that would be deriving
/// *authored intent* from a mesh — ADR 0044 §2's objection to treating the shape of a thing as
/// anything but content. It would also be unfixable by hand, which is the property the whole
/// decision exists to protect: a generated level is a file precisely so somebody can open it and
/// move a door.
///
/// # A socket is a place and a facing, and the facing is the whole mechanism
///
/// Two sockets join when they **face each other**. That makes stitching a *placement* rather than a
/// search: given a piece, a socket on it, and a socket on the piece being added, the second piece's
/// transform is fully determined — rotate until the facings oppose, then translate until the places
/// coincide. Nothing has to be solved or backtracked.
///
/// The place and the facing both come from the entity's own `Transform`, which is why this component
/// carries neither: a socket is a child entity of a piece, so it is positioned and aimed exactly the
/// way a camera or a light is, and **-Z is forward** as it is everywhere else (ADR 0018). Repeating
/// the position here would be two numbers meaning one thing, which is the mistake
/// `FirstPersonCamera::height` documents.
#[derive(Debug, Clone, PartialEq, Eq, Default, StableHash, Reflect)]
pub struct Socket {
    /// What may connect here.
    ///
    /// Two sockets join only when their kinds match, so a corridor mouth does not open onto a
    /// cupboard. A `String` rather than an enum because **what kinds exist is content**: a game with
    /// vents and one with airlocks should not need an engine change, and this is the same call ADR
    /// 0068 makes for the names of facts.
    pub kind: String,
    /// Whether the generator may still attach something here.
    ///
    /// A field rather than removing the component, for `Interactable::enabled`'s reason: a socket
    /// that has been used must not move between archetypes to say so, and a *used* socket is still
    /// worth being able to see in `amadeo query` when a layout comes out wrong.
    pub open: bool,
}

impl Component for Socket {}

impl Socket {
    /// A socket of one kind, open.
    #[must_use]
    pub fn new(kind: &str) -> Self {
        Self {
            kind: kind.to_string(),
            open: true,
        }
    }
}

/// Every open socket in the world, with the piece it belongs to.
///
/// Sorted by entity, so a generator walking this makes the same choices in the same order on every
/// machine — the seeded-RNG half of determinism is worthless if the *sequence* it feeds is not
/// reproducible (I3).
#[must_use]
pub fn open_sockets(world: &World) -> Vec<(Entity, Socket)> {
    let mut found: Vec<(Entity, Socket)> = world
        .query::<(&Socket,)>()
        .filter(|(_, (socket,))| socket.open)
        .map(|(entity, (socket,))| (entity, socket.clone()))
        .collect();
    found.sort_by_key(|(entity, _)| (entity.index(), entity.generation()));
    found
}

// --- Laying out an interior (ADR 0071) ----------------------------------------------------------

/// Which way a socket faces, on the grid an interior is laid out over.
///
/// Cardinal only, and rooms are placed **without rotation**. That is the simplification that makes
/// stitching arithmetic rather than geometry: a socket facing east joins the west socket of the cell
/// next door, and the second piece's position follows from the grid rather than from a matrix.
///
/// Rotation is the obvious extension and is deliberately not here yet — it buys varied *pieces*, and
/// what the exit gate asks for is a varied *layout*.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Side {
    /// Towards -Z, which is forward everywhere else in this engine (ADR 0018).
    North,
    /// Towards +X.
    East,
    /// Towards +Z.
    South,
    /// Towards -X.
    West,
}

impl Side {
    /// Every side, in a fixed order.
    ///
    /// **Fixed, not arbitrary**: the generator walks this to choose a direction, so its order is
    /// part of what a seed means. Shuffling it would change every layout ever generated.
    pub const ALL: [Side; 4] = [Side::North, Side::East, Side::South, Side::West];

    /// The side a socket on this side must meet.
    #[must_use]
    pub fn opposite(self) -> Side {
        match self {
            Side::North => Side::South,
            Side::East => Side::West,
            Side::South => Side::North,
            Side::West => Side::East,
        }
    }

    /// The cell one step this way.
    #[must_use]
    pub fn step(self, cell: (i32, i32)) -> (i32, i32) {
        match self {
            Side::North => (cell.0, cell.1 - 1),
            Side::East => (cell.0 + 1, cell.1),
            Side::South => (cell.0, cell.1 + 1),
            Side::West => (cell.0 - 1, cell.1),
        }
    }
}

/// One placed room, and which of its sides are open onto a neighbour.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlacedRoom {
    /// Where on the grid. Multiply by the cell size to get world units.
    pub cell: (i32, i32),
    /// The sides that lead somewhere, sorted.
    pub doors: Vec<Side>,
    /// Whether this room's ceiling lamp still works.
    ///
    /// **Part of the layout rather than of the writer**, so that [`to_scene`] stays a pure function
    /// of a [`Layout`] — everything random happens in [`lay_out`], where one seeded `Rng` decides
    /// the whole level. A writer that rolled its own dice would make "the same layout writes the
    /// same bytes" a property of nothing.
    pub lit: bool,
}

/// The handful of places in an interior that mean something.
///
/// # Why these are chosen here rather than by the writer
///
/// A room graph is a shape; a *level* is a shape with somewhere to start, somewhere to end, and a
/// reason to walk between them. Choosing those is graph work — it is all shortest paths — so it
/// belongs beside the graph and can be tested without producing a single line of text.
///
/// Every rule below is a deterministic function of the graph, with ties broken by cell order. No
/// dice: where the key is has to be *good*, and a die roll cannot tell a detour from a dead end.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Landmarks {
    /// Where the player wakes up. Always the cell the walk began at.
    pub start: (i32, i32),
    /// Where the way out is: the room **furthest from the start** by doors walked.
    pub exit: (i32, i32),
    /// Where the key is: the room with the largest *detour*, meaning the greatest total distance
    /// from the start and from the exit at once.
    ///
    /// On the shortest route between the two that total is constant, so anything off that route
    /// scores higher — which is exactly "the key is not on your way to the door".
    pub key: (i32, i32),
    /// Where the torch is: one door from the start, so the first thing you do is walk through a
    /// doorway in the dark and pick up a light.
    pub torch: (i32, i32),
    /// Where the warden begins: the room closest to **half way** to the exit, so you meet it on the
    /// journey rather than at the start or standing on the finish line.
    pub warden: (i32, i32),
}

/// A whole interior, as cells and the doors between them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layout {
    /// The seed that produced it, so a file can say how to make it again (ADR 0071 §4).
    pub seed: u64,
    /// Every room, **sorted by cell** so the output is byte-stable (I2).
    pub rooms: Vec<PlacedRoom>,
    /// Where the things that make it a level go.
    pub landmarks: Landmarks,
}

impl Layout {
    /// The room at a cell, if one was placed there.
    #[must_use]
    pub fn at(&self, cell: (i32, i32)) -> Option<&PlacedRoom> {
        room_at(&self.rooms, cell)
    }

    /// How many doors away each room is from `origin`, as `(cell, doors walked)` sorted by cell.
    #[must_use]
    pub fn distances_from(&self, origin: (i32, i32)) -> Vec<((i32, i32), u32)> {
        distances(&self.rooms, origin)
    }

    /// Whether every room can be reached from the first one.
    ///
    /// Worth having as a *query* rather than only as an invariant: a generator that produced an
    /// island would produce a level with an unreachable key, and that is the failure a player
    /// experiences as the game being broken rather than as the game being hard.
    #[must_use]
    pub fn is_connected(&self) -> bool {
        let Some(first) = self.rooms.first() else {
            return true;
        };
        let mut seen: Vec<(i32, i32)> = vec![first.cell];
        let mut frontier = vec![first.cell];

        while let Some(cell) = frontier.pop() {
            let Some(room) = self.at(cell) else {
                continue;
            };
            for side in &room.doors {
                let next = side.step(cell);
                if self.at(next).is_some() && !seen.contains(&next) {
                    seen.push(next);
                    frontier.push(next);
                }
            }
        }
        seen.len() == self.rooms.len()
    }

    /// How many doors there are, counting each once rather than once per side.
    #[must_use]
    pub fn door_count(&self) -> usize {
        self.rooms
            .iter()
            .map(|room| room.doors.len())
            .sum::<usize>()
            / 2
    }

    /// Whether the layout contains a cycle — a route out and back that repeats no door.
    ///
    /// **The property ADR 0071 asks for by name.** For a connected graph this is just "more doors
    /// than a tree would have": a tree over `n` rooms has exactly `n - 1`, so anything more closes a
    /// loop. Cheaper and far more legible than walking for one.
    #[must_use]
    pub fn has_loop(&self) -> bool {
        !self.rooms.is_empty() && self.door_count() >= self.rooms.len()
    }
}

/// Lays out `count` rooms from a seed, and closes a loop.
///
/// Returns `count` rooms, **or `count + 1`** when closing the loop needed a new one — see the second
/// pass below. A level of dead ends is a gameplay failure and one extra room is not, so that is the
/// trade taken deliberately.
///
/// # How it works, in one paragraph
///
/// Walk. Start at the origin, and repeatedly step to a neighbouring cell that is empty, opening a
/// door behind you. When the walk paints itself into a corner, back up to a room that still has an
/// empty neighbour. That gives a connected, tree-shaped run of rooms — and then **one more door is
/// added between two rooms that are adjacent but not yet joined**, which is what turns the tree into
/// a loop.
///
/// The loop is added deliberately rather than hoped for, which is ADR 0071 §3: a tree of rooms forces
/// backtracking, and being chased down a dead end you have already cleared is the failure a horror
/// slice cannot afford.
///
/// # Determinism
///
/// Every choice comes from `rng` and every collection walked is ordered — `Side::ALL` is a fixed
/// array and `rooms` is sorted before it is returned. Two machines given one seed produce one
/// layout, byte for byte.
#[must_use]
pub fn lay_out(seed: u64, count: usize) -> Layout {
    let mut rng = amadeo_core::Rng::new(seed);
    let mut cells: Vec<(i32, i32)> = vec![START];
    let mut doors: Vec<((i32, i32), Side)> = Vec::new();

    // The walk. `path` is where it can back up to, which is what stops it dead-ending early.
    let mut path: Vec<(i32, i32)> = vec![START];
    while cells.len() < count.max(1) {
        let Some(&here) = path.last() else {
            break;
        };
        let free: Vec<Side> = Side::ALL
            .into_iter()
            .filter(|side| !cells.contains(&side.step(here)))
            .collect();

        let Some(index) = rng.pick_index(free.len()) else {
            // Boxed in. Step back and try somewhere earlier; if there is nowhere, the grid around
            // the walk is full and the layout is as large as it is going to get.
            path.pop();
            if path.is_empty() {
                break;
            }
            continue;
        };
        let side = free[index];
        let next = side.step(here);
        cells.push(next);
        doors.push((here, side));
        path.push(next);
    }

    // **The loop, added rather than hoped for.** Two ways, tried in order, and the second is the one
    // that makes it reliable.
    cells.sort_unstable();
    let joined = |doors: &[((i32, i32), Side)], a: (i32, i32), side: Side| {
        doors.contains(&(a, side)) || doors.contains(&(side.step(a), side.opposite()))
    };

    // 1. Two rooms that already touch without a door between them. A walk that doubles back leaves
    //    these lying around, and one door closes the cycle for free.
    let mut closed = false;
    'touching: for &cell in &cells {
        for side in Side::ALL {
            if cells.contains(&side.step(cell)) && !joined(&doors, cell, side) {
                doors.push((cell, side));
                closed = true;
                break 'touching;
            }
        }
    }

    // 2. Otherwise **add a room** in an empty cell that touches two placed ones, and open it to
    //    both. A walk that never doubles back — a spiral, or a straight run — leaves no pair for the
    //    first pass to find, and seed 0 was exactly that. Without this the generator quietly hands
    //    back a tree, which is a level of dead ends.
    if !closed {
        let mut candidates: Vec<((i32, i32), Vec<Side>)> = Vec::new();
        for &cell in &cells {
            for side in Side::ALL {
                let empty = side.step(cell);
                if cells.contains(&empty) {
                    continue;
                }
                let touching: Vec<Side> = Side::ALL
                    .into_iter()
                    .filter(|reach| cells.contains(&reach.step(empty)))
                    .collect();
                if touching.len() >= 2 {
                    candidates.push((empty, touching));
                }
            }
        }
        // Sorted and deduplicated before choosing, so the pick is reproducible rather than
        // dependent on the order the scan happened to reach cells in (I3).
        candidates.sort();
        candidates.dedup();
        if let Some((empty, touching)) = candidates.first() {
            cells.push(*empty);
            cells.sort_unstable();
            for &side in touching {
                doors.push((*empty, side));
            }
        }
    }

    let rooms: Vec<PlacedRoom> = cells
        .iter()
        .map(|&cell| {
            let mut open: Vec<Side> = Side::ALL
                .into_iter()
                .filter(|&side| joined(&doors, cell, side))
                .collect();
            open.sort_unstable();
            PlacedRoom {
                cell,
                doors: open,
                // **The room you wake up in always works; after that it is luck.** Waking in the
                // pitch dark before you have found the torch is not atmosphere, it is a player who
                // cannot tell the game has started. `cells` is sorted before this runs, so the
                // sequence of draws is the same on every machine (I3).
                lit: cell == START || rng.chance(LAMPS_WORKING),
            }
        })
        .collect();

    let landmarks = choose_landmarks(&rooms);
    Layout {
        seed,
        rooms,
        landmarks,
    }
}

/// The cell every walk begins at, which is also where the player wakes up.
pub const START: (i32, i32) = (0, 0);

/// The chance that a room other than the start has a working lamp.
///
/// **An eyeball number, and the one that decides how dark the Warren is.** Too high and the torch is
/// pointless; too low and the level is a black maze before you have found it. A room is lit or it is
/// not, rather than everywhere being dimly lit, because a dark room next to a working one reads as
/// lighting that has failed in patches — which is a *place*. Uniform gloom reads as a setting.
pub const LAMPS_WORKING: f32 = 0.45;

/// The room at a cell, in a list that may not be a finished [`Layout`] yet.
fn room_at(rooms: &[PlacedRoom], cell: (i32, i32)) -> Option<&PlacedRoom> {
    rooms.iter().find(|room| room.cell == cell)
}

/// How many doors away each room is from `origin`, as `(cell, doors walked)` sorted by cell.
///
/// A plain breadth-first walk, returned as a sorted `Vec` rather than a map. That is deliberate
/// twice over: a `HashMap` would put iteration order into a generator (trap 2), and over the twenty
/// rooms this deals in a linear scan beats hashing anyway.
///
/// A room the walk cannot reach is **absent** rather than present with a large number, so a caller
/// that forgets to handle an island gets nothing rather than a wrong answer.
fn distances(rooms: &[PlacedRoom], origin: (i32, i32)) -> Vec<((i32, i32), u32)> {
    let mut found: Vec<((i32, i32), u32)> = Vec::new();
    if room_at(rooms, origin).is_none() {
        return found;
    }
    found.push((origin, 0));

    // A `Vec` plus a read cursor, which is a breadth-first queue without reaching for `VecDeque`.
    let mut next = 0usize;
    while next < found.len() {
        let (cell, steps) = found[next];
        next += 1;
        let Some(room) = room_at(rooms, cell) else {
            continue;
        };
        for &side in &room.doors {
            let neighbour = side.step(cell);
            if room_at(rooms, neighbour).is_some()
                && !found.iter().any(|(seen, _)| *seen == neighbour)
            {
                found.push((neighbour, steps + 1));
            }
        }
    }

    found.sort_unstable();
    found
}

/// How far `cell` is from wherever a distance table was measured, if it was reached at all.
fn steps_to(table: &[((i32, i32), u32)], cell: (i32, i32)) -> Option<u32> {
    table
        .iter()
        .find(|(seen, _)| *seen == cell)
        .map(|(_, steps)| *steps)
}

/// Picks the start, the exit, the key, the torch and the warden's post out of a room graph.
///
/// # Every rule is "furthest" or "half way", and a tie goes to the lowest cell
///
/// Deterministic all the way down, and — the reason it is worth more than a die roll — *legible when
/// a level comes out wrong*. "The exit is in the room furthest from the start" is a sentence
/// somebody can check against a picture. "The generator rolled a four" is not.
///
/// Tie-breaking is not an afterthought here. Every table these read is sorted by cell and every
/// comparison is strict, so the lowest cell wins — which is what makes a layout reproducible rather
/// than merely repeatable on one machine.
#[must_use]
fn choose_landmarks(rooms: &[PlacedRoom]) -> Landmarks {
    let start = START;
    let from_start = distances(rooms, start);

    let exit = from_start
        .iter()
        .max_by_key(|(_, steps)| *steps)
        .map_or(start, |(cell, _)| *cell);
    let from_exit = distances(rooms, exit);

    // The biggest detour: furthest from the start and from the exit *at once*. Everywhere on a
    // shortest route between the two scores the same total, so the winner is off that route
    // whenever anywhere is — which is the whole point of fetching a key.
    let key = from_start
        .iter()
        .filter(|(cell, _)| *cell != start && *cell != exit)
        .max_by_key(|(cell, steps)| steps + steps_to(&from_exit, *cell).unwrap_or(0))
        .map_or(exit, |(cell, _)| *cell);

    // One door from the start, in `Side::ALL` order. A one-room level puts it underfoot.
    let torch = room_at(rooms, start)
        .and_then(|room| room.doors.first().map(|side| side.step(start)))
        .unwrap_or(start);

    // Half way to the exit, measured in doors walked rather than in metres — a room two doors away
    // through a loop is nearer than one two cells away through a wall.
    let half = steps_to(&from_start, exit).unwrap_or(0) / 2;
    let warden = from_start
        .iter()
        .filter(|(cell, _)| *cell != start)
        .min_by_key(|(_, steps)| steps.abs_diff(half))
        .map_or(start, |(cell, _)| *cell);

    Landmarks {
        start,
        exit,
        key,
        torch,
        warden,
    }
}

/// How far apart room centres sit, in world units.
///
/// One number rather than a per-piece size, because rooms are placed on a grid without rotation
/// (see [`Side`]) — so a piece that did not fill its cell would leave a gap rather than misalign,
/// which is a content decision rather than a generator one.
pub const CELL: f32 = 12.0;

/// The prefab a room's shell comes from.
pub const ROOM_PIECE: &str = "room_shell";

/// The prefab a doorway comes from.
pub const DOORWAY_PIECE: &str = "doorway";

/// The prefab a blank wall comes from.
pub const WALL_PIECE: &str = "wall";

/// The prefab the player, the camera and the torch beam come from.
pub const PLAYER_PIECE: &str = "player_start";

/// The prefab the door out comes from.
pub const EXIT_PIECE: &str = "way_out";

/// The prefab the key and the crate it sits on come from.
pub const KEY_PIECE: &str = "lost_key";

/// The prefab the torch and the crate it sits on come from.
pub const TORCH_PIECE: &str = "dropped_torch";

/// The prefab a working ceiling lamp comes from.
pub const LAMP_PIECE: &str = "room_lamp";

/// The prefab the warden comes from.
pub const WARDEN_PIECE: &str = "warden_post";

/// The prefab the dim light leaking in from elsewhere comes from.
pub const SPILL_PIECE: &str = "spill";

/// The prefab the two HUD lines come from.
pub const HUD_PIECE: &str = "hud";

/// Every piece a generated level instances — which is also, once sorted, its `assets` block.
///
/// Listed by constant rather than by id on purpose, and **sorted at the point of use** rather than
/// here: the two orders are not the same, and hand-maintaining a sorted list of ids whose names are
/// spelled differently from their constants is exactly the sort of thing that goes quietly wrong.
/// `amadeo fmt --check` on the output is what would have caught it, and did.
pub const PIECES: [&str; 9] = [
    DOORWAY_PIECE,
    HUD_PIECE,
    KEY_PIECE,
    LAMP_PIECE,
    PLAYER_PIECE,
    ROOM_PIECE,
    SPILL_PIECE,
    TORCH_PIECE,
    WALL_PIECE,
];

/// How high off the floor the player's body sits when placed.
///
/// **A generator constant rather than an authored one, and the exception is worth naming.** Every
/// other content piece has a root at floor level and authors its own heights in its children, so
/// that moving a lamp up is an edit to `room_lamp.scene` and nothing else. The player cannot work
/// that way: an override replaces a whole `Transform`, and the character controller reads and
/// writes the *local* transform of the entity carrying it — so the player has to be a prefab root
/// with no parent, and a root's height is whatever the override says.
pub const PLAYER_STAND: f32 = 1.0;

/// How high off the floor the warden sits when placed.
///
/// [`PLAYER_STAND`]'s exception for [`PLAYER_STAND`]'s reason: `watch_for_you`, `move_the_warden`
/// and `settle_the_run` all read the warden's plain `Transform` and compare it against the player's,
/// so both have to be unparented roots or those distances are measured in different spaces.
pub const WARDEN_STAND: f32 = 0.93;

/// How far from a room's centre a prop stands, in world units.
///
/// Well inside the twelve-metre cell, so a crate never clips a wall whichever sides that room has.
const PROP_OFFSET: f32 = 3.2;

/// How far in from a wall's plane the door out sits.
///
/// A shade more than the wall's own thickness, so the door reads as set into the opening rather than
/// as a slab z-fighting with the plaster.
const DOOR_INSET: f32 = 0.28;

/// The yaw, in degrees, whose forward direction is `side`.
///
/// Forward is -Z (ADR 0018), and `Mat4::from_euler_degrees` puts the local +Z axis at
/// `[sin(yaw), 0, cos(yaw)]` — so forward is `[-sin(yaw), 0, -cos(yaw)]` and the four cardinals fall
/// out as below. Pinned by a test against the real matrix rather than trusted, because a facing that
/// is ninety degrees out is entirely plausible and entirely wrong.
#[must_use]
pub fn facing(side: Side) -> f32 {
    match side {
        Side::North => 0.0,
        Side::West => 90.0,
        Side::South => 180.0,
        Side::East => 270.0,
    }
}

/// Turns a [`Layout`] into scene text (ADR 0071 §1).
///
/// # Why three pieces rather than sixteen
///
/// The obvious design gives each room a shell chosen by which of its four sides have doors, which
/// needs a library of **sixteen** before anything can be generated at all. Instead a room is a
/// shell with no walls at all — floor and ceiling — and every side gets its own piece: a `wall`
/// where the room is closed, a `doorway` where it is not.
///
/// **Additive geometry is what forces this**, and it is worth being explicit about. A doorway
/// cannot cut a hole in a solid wall, because nothing here subtracts one shape from another. So a
/// wall is something a side *has*, not something a shell comes with, and "no door" is a piece
/// rather than the absence of one.
///
/// # A shared side is emitted once, an outer side always
///
/// A side between two rooms belongs to both, and only the room that sees it as `North` or `West`
/// writes it — every shared side is one of those from exactly one of the two cells. Emitting from
/// both would stack two walls in one place, which reads as z-fighting rather than as a bug.
///
/// A side with **no** neighbour is seen by one room only, so it is always written. Getting that
/// backwards leaves the level open to the void along half its boundary.
///
/// # Byte-stability (I2)
///
/// Rooms arrive sorted from [`lay_out`] and sides are written in [`Side::ALL`] order, so one layout
/// produces one file. `amadeo fmt --check` on the output is a free regression test for that.
#[must_use]
pub fn to_scene(layout: &Layout) -> String {
    let mut out = String::new();
    // **The seed rides in the scene's name**, which is how ADR 0071 §4's "a file can say how to make
    // it again" is honoured. The format has no comments, and inventing one to carry a number would
    // be trap 4 — the format is a designed artefact, not whatever a writer needs it to be.
    out.push_str(&format!(
        "scene warren_generated_{}\nversion 1\n\n",
        layout.seed
    ));

    // Sorted, because the assets block is (ADR 0021) and byte-stability starts at the header.
    out.push_str("assets\n");
    let mut pieces = PIECES;
    pieces.sort_unstable();
    for piece in pieces {
        out.push_str(&format!("  {piece}\n"));
    }
    out.push('\n');

    for room in &layout.rooms {
        let (x, z) = (room.cell.0 as f32 * CELL, room.cell.1 as f32 * CELL);
        out.push_str(&format!(
            "entity {} \"Room\" from {ROOM_PIECE}\n",
            cell_id("room", room.cell)
        ));
        out.push_str(&place(x, 0.0, z, 0.0));

        for side in Side::ALL {
            // A shared side is written by whichever room sees it as north or west; an outer side is
            // seen by one room and is always written. See this function's docs for why getting the
            // second half backwards leaves the level open to the void along half its boundary.
            let shared = layout.at(side.step(room.cell)).is_some();
            if shared && !matches!(side, Side::North | Side::West) {
                continue;
            }

            let half = CELL / 2.0;
            let (dx, dz) = match side {
                Side::North => (x, z - half),
                Side::South => (x, z + half),
                Side::West => (x - half, z),
                Side::East => (x + half, z),
            };
            // A side wall is the same piece turned a quarter turn.
            let turn = if matches!(side, Side::East | Side::West) {
                90.0
            } else {
                0.0
            };

            let (piece, label) = if room.doors.contains(&side) {
                (DOORWAY_PIECE, "Doorway")
            } else {
                (WALL_PIECE, "Wall")
            };
            let prefix = format!("{}_{}", label.to_lowercase(), side_name(side));
            out.push_str(&format!(
                "entity {} \"{label}\" from {piece}\n",
                cell_id(&prefix, room.cell)
            ));
            out.push_str(&place(dx, 0.0, dz, turn));
        }

        // The lamp goes in with its room rather than in a pass of its own, so a room's geometry and
        // its light sit next to each other in the file a person has to read.
        if room.lit {
            out.push_str(&format!(
                "entity {} \"Lamp\" from {LAMP_PIECE}\n",
                cell_id("lamp", room.cell)
            ));
            out.push_str(&place(x, 0.0, z, 0.0));
        }
    }

    write_contents(&mut out, layout);

    // Every entity above is written with a blank line after it, which leaves one too many at the
    // end. Trimmed here rather than by making the last writer special, so adding a tenth thing to
    // `write_contents` cannot reintroduce it. `amadeo fmt --check` on the output is what noticed.
    let trimmed = out.trim_end().to_string();
    format!("{trimmed}\n")
}

/// Writes the things that make a shape into a level: a player, a torch, a key, a door, a warden.
///
/// Split out of [`to_scene`] because it answers a different question. Everything above places
/// *geometry* from a room's own four sides; everything here places one thing in one chosen room, and
/// which room is [`Landmarks`]' answer rather than this function's.
fn write_contents(out: &mut String, layout: &Layout) {
    let marks = layout.landmarks;
    let centre = |cell: (i32, i32)| (cell.0 as f32 * CELL, cell.1 as f32 * CELL);

    // **The player faces the way out of the start room**, which is the cheapest piece of direction a
    // level can give: you wake up looking at a doorway rather than at plaster.
    let (sx, sz) = centre(marks.start);
    let look = layout
        .at(marks.start)
        .and_then(|room| room.doors.first().copied())
        .map_or(0.0, facing);
    out.push_str(&format!("entity you \"You\" from {PLAYER_PIECE}\n"));
    out.push_str(&place(sx, PLAYER_STAND, sz, look));

    let (tx, tz) = centre(marks.torch);
    out.push_str(&format!("entity torch \"Torch\" from {TORCH_PIECE}\n"));
    out.push_str(&place(tx - PROP_OFFSET, 0.0, tz - PROP_OFFSET, 0.0));

    let (kx, kz) = centre(marks.key);
    out.push_str(&format!("entity key \"Key\" from {KEY_PIECE}\n"));
    out.push_str(&place(kx + PROP_OFFSET, 0.0, kz + PROP_OFFSET, 0.0));

    // The door goes **in a wall**, not in the middle of a room, so a side has to be chosen for it.
    let side = exit_side(layout);
    let (ex, ez) = centre(marks.exit);
    let inset = CELL / 2.0 - DOOR_INSET;
    let (dx, dz) = match side {
        Side::North => (ex, ez - inset),
        Side::South => (ex, ez + inset),
        Side::West => (ex - inset, ez),
        Side::East => (ex + inset, ez),
    };
    out.push_str(&format!(
        "entity way_out \"The way out\" from {EXIT_PIECE}\n"
    ));
    // Facing **into** the room, which is the side's opposite: a door in the north wall is looked at
    // from the south.
    out.push_str(&place(dx, 0.0, dz, facing(side.opposite())));

    // Offset like the props rather than dead centre, and for a reason beyond looks: in a layout too
    // small to have a room that is not the start — a one-room level — the warden's own rule falls
    // back to the start cell, and a warden standing *inside* the player ends the run before it
    // begins. A corner it does not share with the torch or the key is the whole fix.
    let (wx, wz) = centre(marks.warden);
    out.push_str(&format!(
        "entity warden \"The warden\" from {WARDEN_PIECE}\n"
    ));
    out.push_str(&place(
        wx + PROP_OFFSET,
        WARDEN_STAND,
        wz - PROP_OFFSET,
        0.0,
    ));

    // Neither of these is placed anywhere, so neither takes an override — and an override naming a
    // component its prefab does not carry is refused at load, which is what would happen if the HUD
    // were handed a `Transform` (ADR 0029). A blank line after each keeps the file's shape uniform.
    out.push_str(&format!(
        "entity spill \"Spill from somewhere\" from {SPILL_PIECE}\n\n"
    ));
    out.push_str(&format!("entity hud \"HUD\" from {HUD_PIECE}\n\n"));
}

/// Which side of the exit room the door out is set into.
///
/// **An outer side first**, so the way out leads outside rather than into the next room. A room with
/// four neighbours has none, so a side that is merely walled is the fallback, and a room with four
/// doors — which needs a loop through every neighbour — falls back to north with a door in it.
/// All three cases produce a usable level; only the first produces a sensible one.
#[must_use]
pub fn exit_side(layout: &Layout) -> Side {
    let cell = layout.landmarks.exit;
    let Some(room) = layout.at(cell) else {
        return Side::North;
    };
    Side::ALL
        .into_iter()
        .find(|side| layout.at(side.step(cell)).is_none())
        .or_else(|| {
            Side::ALL
                .into_iter()
                .find(|side| !room.doors.contains(side))
        })
        .unwrap_or(Side::North)
}

/// A scene-safe entity id for a cell, since a negative coordinate cannot go in an identifier.
fn cell_id(prefix: &str, cell: (i32, i32)) -> String {
    let part = |value: i32| {
        if value < 0 {
            format!("n{}", value.abs())
        } else {
            value.to_string()
        }
    };
    format!("{prefix}_{}_{}", part(cell.0), part(cell.1))
}

/// The lower-case name of a side, for an entity id.
fn side_name(side: Side) -> &'static str {
    match side {
        Side::North => "north",
        Side::East => "east",
        Side::South => "south",
        Side::West => "west",
    }
}

/// A `Transform` override block, in the canonical field order the writer emits.
fn place(x: f32, y: f32, z: f32, turn: f32) -> String {
    // **`override`, not a bare component.** Every piece already puts a `Transform` on its own root,
    // and ADR 0029 refuses a silent replacement: an override has to be spelled out so that it is
    // visible in the file (I1). Emitting the bare form parses and passes `amadeo check`, and then
    // fails at *load* — which is how this was found, and is worth knowing about `check`'s reach.
    format!(
        "  override Transform\n    rotation 0.0 {turn:?} 0.0\n    scale 1.0 1.0 1.0\n    \
         translation {x:?} {y:?} {z:?}\n\n"
    )
}

// --- The HUD ------------------------------------------------------------------------------------

/// Marks the line that says what using the thing in front of you would do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, StableHash, Reflect)]
pub struct PromptLine;

impl Component for PromptLine {}

/// Marks the line that says how the run ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, StableHash, Reflect)]
pub struct EndingLine;

impl Component for EndingLine {}

/// The label [`write_the_hud`] is registered under.
pub const WRITE_THE_HUD: &str = "write_the_hud";

/// Puts this tick's prompt and ending into the two authored `Text` nodes.
///
/// # Why this runs in `PostSimulation` rather than in `Render`
///
/// `Text` is an ordinary component, so **its content is in the state hash**. That rules out writing
/// it from a draw pass, which is ADR 0063's split seen from a third side: the focus highlight is
/// substituted during collection precisely so the theme never reaches the hash.
///
/// Writing it here is safe, and the reason is specific rather than general. It is a pure function of
/// [`Looking`] and [`Outcome`], and although `Looking` is *derived* it is recomputed every tick from
/// transforms and the physics index — both already hashed — and, unlike `ComputedRect`, it **does
/// not depend on the window size**. So two machines running the same inputs write the same string.
/// A HUD line derived from a rectangle instead would put the resolution into the hash and break I3
/// for every player on a different monitor.
///
/// After `propagate_transforms`, because `Looking` is written by `update_interactions`, which is.
pub fn write_the_hud(world: &mut World) {
    let ending = match outcome(world) {
        Outcome::Playing => String::new(),
        Outcome::Escaped => "YOU GOT OUT".to_string(),
        Outcome::Caught => "IT FOUND YOU".to_string(),
    };
    // Nothing to point at once the run is over, and a stale "the door is locked" under a lose
    // screen reads as the game still running.
    let prompt = if ending.is_empty() {
        prompt(world).unwrap_or_default()
    } else {
        String::new()
    };

    let lines: Vec<(Entity, String)> = world
        .query::<(&PromptLine,)>()
        .map(|(entity, _)| (entity, prompt.clone()))
        .chain(
            world
                .query::<(&EndingLine,)>()
                .map(|(entity, _)| (entity, ending.clone())),
        )
        .collect();

    for (entity, wanted) in lines {
        // Compared before writing, so a HUD that says the same thing as last tick does not move the
        // state hash — which it would every tick otherwise, for no change anyone can see.
        if let Some(text) = world.get_mut::<Text>(entity)
            && text.content != wanted
        {
            text.content = wanted;
        }
    }
}
