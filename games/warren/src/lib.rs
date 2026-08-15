//! **The Warren** — M3's exit gate, in progress: a first-person atmospheric horror slice.
//!
//! ```text
//! cargo run -p warren
//! ```
//!
//! WASD to walk, the mouse to look, F to use what is in front of you. The torch is on the near
//! crate; picking it up lights the beam.
//!
//! # What exists so far, and what does not
//!
//! **One handcrafted room, and a loop you can win and lose.** Find the torch, find the key, reach
//! the door — and the warden is looking for you. That is `docs/05`'s exit gate items 1 (a playable
//! loop with a win and a lose state) and 3 (a pursuing entity with distinct AI states, driven by
//! `mod-behaviour`).
//!
//! A HUD says what is in reach and how the run ended, authored in the scene like everything else.
//!
//! **Still missing, and it is most of the gate**: bounded procedural interiors assembled from
//! handcrafted room pieces (**Q40** — the artefact question comes before the algorithm), a title
//! screen, and audio. Save and resume are not wired up either; `games/atrium` proves that mechanism
//! and this game has not needed it.
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
}

/// A whole interior, as cells and the doors between them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layout {
    /// The seed that produced it, so a file can say how to make it again (ADR 0071 §4).
    pub seed: u64,
    /// Every room, **sorted by cell** so the output is byte-stable (I2).
    pub rooms: Vec<PlacedRoom>,
}

impl Layout {
    /// The room at a cell, if one was placed there.
    #[must_use]
    pub fn at(&self, cell: (i32, i32)) -> Option<&PlacedRoom> {
        self.rooms.iter().find(|room| room.cell == cell)
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
    let mut cells: Vec<(i32, i32)> = vec![(0, 0)];
    let mut doors: Vec<((i32, i32), Side)> = Vec::new();

    // The walk. `path` is where it can back up to, which is what stops it dead-ending early.
    let mut path: Vec<(i32, i32)> = vec![(0, 0)];
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

    let rooms = cells
        .iter()
        .map(|&cell| {
            let mut open: Vec<Side> = Side::ALL
                .into_iter()
                .filter(|&side| joined(&doors, cell, side))
                .collect();
            open.sort_unstable();
            PlacedRoom { cell, doors: open }
        })
        .collect();

    Layout { seed, rooms }
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
