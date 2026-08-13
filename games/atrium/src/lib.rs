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

use amadeo_app::{App, Paused, Stage, system};

use amadeo_audio::{
    Audio, AudioListener, AudioSource, COLLECT_AUDIO, SoundCache, SoundPlayed, collect_audio,
};
use amadeo_character::CharacterMotion;
use amadeo_core::StableHash;
use amadeo_ecs::{Component, Entity, Resource, World};
use amadeo_events::WorldEvents;
use amadeo_input::{ActionId, InputDriver, InputState, NullSource};
use amadeo_physics::{Collider, Gravity, Physics, RapierPhysics, RigidBody, Velocity};
use amadeo_reflect::Reflect;
use amadeo_render::{
    BoxMesh, Camera, DirectionalLight, Environment, Material, Mesh, PlaneMesh, PointLight,
    SortOrder, SpotLight, TextureCache,
};
use amadeo_transform::{
    GlobalTransform, PROPAGATE_TRANSFORMS, Parent, Transform, propagate_transforms,
};
use amadeo_ui::{
    COLLECT_UI, ComputedRect, Focus, Focusable, FontCache, LAYOUT_UI, NAVIGATE_FOCUS, Panel, Text,
    Theme, UiActivated, UiNode, collect_ui, layout_ui_system, navigate_focus,
};

/// Where this game's assets live, relative to the project root (ADR 0022).
const ASSET_DIRECTORY: &str = "games/atrium/assets";

/// How far the character walks between footsteps, in metres.
///
/// A stride rather than a timer, so footsteps slow down when the character does instead of marching
/// at a fixed rate while they creep. Tuned by ear against a 5 m/s walk speed, which puts them a bit
/// under half a second apart.
///
/// Public so a test can derive how many footsteps a given walk should produce, rather than hard-code
/// a number — the first version of that test expected five, got three, and the difference was the
/// plinth stopping the character early. A magic number would have made the level layout part of the
/// audio test's contract.
pub const STRIDE: f32 = 1.9;

/// The label [`play_footsteps`] is registered under.
pub const PLAY_FOOTSTEPS: &str = "play_footsteps";

/// The label [`apply_screen`] is registered under.
pub const APPLY_SCREEN: &str = "apply_screen";

/// The label [`choose_from_menu`] is registered under.
pub const CHOOSE_FROM_MENU: &str = "choose_from_menu";

/// The named action that opens and closes the pause menu. Escape, on a keyboard.
///
/// A *game* action rather than an engine one, unlike `ui_next` and friends: the engine knows how a
/// menu moves (ADR 0063) and how to stop simulating (ADR 0065), and neither of them has an opinion
/// about whether this game has a pause at all.
pub const PAUSE: &str = "pause";

/// Where the player is put back when they choose "return to start".
///
/// The same place `scenes/atrium.scene` spawns them. Duplicated rather than read back out of the
/// scene, because by the time this runs the entity has walked away from it and there is nowhere
/// left to read it *from* — a spawn point is authored data the world stops remembering.
const SPAWN: [f32; 3] = [0.0, 1.0, 2.0];

/// What the Atrium is doing, as far as the player is concerned.
///
/// # Why this is in the game and not in the engine (ADR 0065 §5)
///
/// **What screens exist is genre knowledge.** This room has somewhere to walk and a menu to stop at;
/// a strategy game has neither in that shape. Putting a `Screen` type below the module layer is what
/// invariant I4 forbids, and the engine could not know these three names anyway.
///
/// Nothing had to be built for this to work: it is an ordinary reflected resource, so it is hashed,
/// a snapshot restores it, and `amadeo query` can read it.
///
/// # This is the authority; `Paused` is projected from it
///
/// The engine knows only "are the gameplay stages running". [`apply_screen`] writes that from this,
/// every tick, and nothing else writes it — so the two cannot drift apart into a game that is
/// unpaused with a menu up, which is the bug this arrangement exists to make unavailable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, StableHash, Reflect)]
pub enum Screen {
    /// Walking around the room.
    #[default]
    Playing,
    /// The pause menu is up and the world is frozen.
    Paused,
    /// The player chose "quit"; the window closes on the next frame.
    ///
    /// Terminal — Escape does not undo it. A game shutting down and then not shutting down because
    /// somebody was still holding a key would be a memorable bug.
    Quitting,
}

impl Resource for Screen {}

/// What one of the pause menu's buttons does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, StableHash, Reflect)]
pub enum MenuChoice {
    /// Close the menu and carry on.
    #[default]
    Resume,
    /// Put the character back where they started, standing still.
    ReturnToStart,
    /// Close the game.
    Quit,
}

/// Attached to a menu button, saying what choosing it means.
///
/// # ADR 0063's split, cashed
///
/// `UiActivated` says *which entity* was chosen and deliberately nothing else, because the engine
/// does not know what a button means. This is the game supplying that half — and it is supplied in
/// the **scene file**, beside the button it belongs to, rather than as a table of entity ids in Rust
/// that would go stale the moment somebody reordered the menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, StableHash, Reflect)]
pub struct MenuButton {
    /// What this button does.
    pub choice: MenuChoice,
}

impl Component for MenuButton {}

/// Marks the pause menu's root node, so one system can show and hide the whole thing.
///
/// A marker rather than a hard-coded entity id: the scene file decides what the menu *is*, and this
/// says only which node is its root.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, StableHash, Reflect)]
pub struct PauseMenu;

impl Component for PauseMenu {}

/// Toggles the pause on Escape, and projects [`Screen`] onto the things that follow from it.
///
/// # Why this runs in `PreSimulation`
///
/// Because that stage runs **whether or not the game is paused** (ADR 0065), and a system that
/// stopped running while paused could never unpause. It is also after `sample_input`, which is what
/// makes `just_pressed` mean this tick's keypress.
///
/// # One writer for everything derived from the screen
///
/// The toggle is one line; the rest of this function is projection — `Paused` and the menu's
/// visibility. Doing that here rather than at each place the screen changes is what stops a menu
/// from being visible over a running game: there is one place that decides, and it runs every tick
/// rather than only on the tick something changed.
pub fn apply_screen(world: &mut World) {
    let toggled = world
        .resource::<InputState>()
        .is_some_and(|input| input.just_pressed(ActionId::new(PAUSE)));

    let screen = world.resource::<Screen>().copied().unwrap_or_default();
    let next = match (screen, toggled) {
        (Screen::Playing, true) => Screen::Paused,
        (Screen::Paused, true) => Screen::Playing,
        // Including `Quitting`, which nothing gets out of.
        (current, _) => current,
    };
    if let Some(slot) = world.resource_mut::<Screen>() {
        *slot = next;
    }

    let paused = next == Screen::Paused;

    // **While the menu is up, something is always highlighted.**
    //
    // `navigate_focus` deliberately will not do this: a menu that focused an item the moment it
    // appeared would override whatever the game wanted focused, one tick after the scene loaded —
    // ADR 0063, and there is a test named for it. So the game does it, which is the right way
    // round. The engine knows how a menu moves; the game knows when one is up.
    //
    // Written as "if nothing is focused" rather than "on the tick it opened", because that also
    // covers a highlight that fell off an item which stopped being focusable, and costs one
    // comparison to say.
    let nothing_focused = world
        .resource::<Focus>()
        .is_none_or(|focus| focus.entity.is_none());
    if paused && nothing_focused {
        focus_first_item(world);
    }
    if let Some(state) = world.resource_mut::<Paused>() {
        state.paused = paused;
    }

    // Collected before writing, because the query borrows the world. Only the nodes whose
    // visibility is actually wrong are touched -- writing an identical `UiNode` every tick would
    // work, and would also mean the state hash could never tell a menu opening from a menu that was
    // already open.
    let stale: Vec<Entity> = world
        .query::<(&PauseMenu, &UiNode)>()
        .filter(|(_, (_, node))| node.visible != paused)
        .map(|(entity, _)| entity)
        .collect();
    for root in stale {
        if let Some(node) = world.get_mut::<UiNode>(root) {
            node.visible = paused;
        }
    }
}

/// Acts on a menu button being chosen.
///
/// # Why it runs while paused
///
/// It is the one gameplay system that must (ADR 0065): everything it responds to happens while the
/// world is frozen. Registered with `.while_paused()` in `Simulation`, beside `navigate_focus`.
///
/// # It sets the screen and nothing else
///
/// "Resume" does not hide the menu or unpause the engine — it moves [`Screen`], and
/// [`apply_screen`] does the rest on the next tick. Two systems writing the same derived state is
/// how the two halves get out of step.
pub fn choose_from_menu(world: &mut World) {
    // `UiActivated` was sent last tick and swapped in at the end of it, so there is no ordering
    // constraint against `navigate_focus` here -- declaring one would suggest a same-tick handoff
    // that the event buffers do not provide.
    let chosen: Vec<MenuChoice> = world
        .read_events::<UiActivated>()
        .iter()
        .filter_map(|record| world.get::<MenuButton>(record.event.entity))
        .map(|button| button.choice)
        .collect();

    for choice in chosen {
        match choice {
            MenuChoice::Resume => {
                if let Some(screen) = world.resource_mut::<Screen>() {
                    *screen = Screen::Playing;
                }
            }
            MenuChoice::ReturnToStart => {
                return_to_start(world);
                if let Some(screen) = world.resource_mut::<Screen>() {
                    *screen = Screen::Playing;
                }
            }
            MenuChoice::Quit => {
                if let Some(screen) = world.resource_mut::<Screen>() {
                    *screen = Screen::Quitting;
                }
            }
        }
    }
}

/// Puts the highlight on the lowest-numbered menu button.
///
/// Every `Focusable` in this game is a pause-menu button, so there is nothing to narrow the search
/// to. A game with two menus would look under the one it just opened; the walk to do that is
/// `Parent`, and it is worth writing only when there are two.
///
/// Ties break by entity — spawn order, which is the order the scene file lists them — so this picks
/// the same button every time rather than whichever the storage happened to yield first.
fn focus_first_item(world: &mut World) {
    let mut items: Vec<(i32, Entity)> = world
        .query::<(&Focusable,)>()
        .filter(|(_, (focusable,))| focusable.enabled)
        .map(|(entity, (focusable,))| (focusable.order, entity))
        .collect();
    items.sort_by_key(|(order, entity)| (*order, entity.index(), entity.generation()));

    let first = items.first().map(|(_, entity)| *entity);
    if let Some(focus) = world.resource_mut::<Focus>() {
        focus.entity = first;
    }
}

/// Puts every character back at [`SPAWN`], standing still.
///
/// **The velocity matters as much as the position.** A character teleported while walking arrives
/// still walking, and the first thing they do is stroll off the spot they were just put back on.
fn return_to_start(world: &mut World) {
    let characters: Vec<Entity> = world
        .query::<(&CharacterMotion, &Transform)>()
        .map(|(entity, _)| entity)
        .collect();

    for entity in characters {
        if let Some(transform) = world.get_mut::<Transform>(entity) {
            transform.translation = SPAWN;
            transform.rotation = [0.0; 3];
        }
        if let Some(motion) = world.get_mut::<CharacterMotion>(entity) {
            motion.velocity = [0.0; 3];
        }
    }
}

/// How far the character has walked since the last footstep.
///
/// # Why this is a resource and not a service
///
/// It is **gameplay state**. Where you are in your stride decides when the next footstep happens,
/// and ADR 0059's whole point is that the *decision* to play a sound is simulation while the playing
/// is not. So this is hashed, it reproduces in a replay, and a snapshot restores you mid-stride —
/// where a service would silently reset your gait every time a save was loaded.
#[derive(Debug, Default, Clone, PartialEq, StableHash, Reflect)]
pub struct Stride {
    /// Metres walked since the last footstep.
    #[reflect(unit = "m")]
    pub since_last: f32,
}

impl Resource for Stride {}

/// Emits a [`SoundPlayed`] every [`STRIDE`] metres the character walks on the ground.
///
/// # Why this lives in the game rather than in `modules/amadeo-character`
///
/// **A footstep is content.** How often one happens, what it sounds like, and whether a character
/// makes one at all are questions about *this* game — a floating drone and a person in boots share a
/// character controller and should not share a gait. Invariant I4's rule one level up: the module
/// knows how to move, and the game knows what moving sounds like.
///
/// # It only counts horizontal distance, and only on the ground
///
/// Falling is not walking, so `grounded` gates it; and vertical speed must not count towards a
/// stride, or a character jumping on the spot would tap out footsteps in mid-air.
pub fn play_footsteps(world: &mut amadeo_ecs::World) {
    let walked: Vec<(f32, [f32; 3])> = world
        .query::<(&CharacterMotion, &Transform)>()
        .filter(|(_, (motion, _))| motion.grounded)
        .map(|(_, (motion, transform))| {
            // Horizontal only — see above.
            let speed = (motion.velocity[0] * motion.velocity[0]
                + motion.velocity[2] * motion.velocity[2])
                .sqrt();
            (speed * amadeo_core::FIXED_DT, transform.translation)
        })
        .collect();

    let mut steps: Vec<[f32; 3]> = Vec::new();
    if let Some(stride) = world.resource_mut::<Stride>() {
        for (distance, position) in walked {
            stride.since_last += distance;
            // `while` rather than `if`, so a tick that covers more than one stride emits more than
            // one footstep. It cannot happen at this speed and it is one character; it can happen
            // the moment somebody adds a sprint or a debug teleport, and a silent cap is the kind of
            // thing nobody thinks to look for.
            while stride.since_last >= STRIDE {
                stride.since_last -= STRIDE;
                steps.push(position);
            }
        }
    }

    for position in steps {
        world.send_event(SoundPlayed::at("footstep", position));
    }
}

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
    // The interface (ADR 0062). `ComputedRect` is registered although nothing authors one, because
    // it is a component the agent should be able to *see* — "where did that button end up" is the
    // question `world.entity` exists to answer.
    app.register_component::<UiNode>()?;
    app.register_component::<ComputedRect>()?;
    app.register_component::<Panel>()?;
    app.register_component::<Text>()?;
    // The interactive half (ADR 0063), plus this game's two additions: which node is the menu, and
    // what each of its buttons means.
    app.register_component::<Focusable>()?;
    app.register_component::<MenuButton>()?;
    app.register_component::<PauseMenu>()?;
    // Registered because this game *ships* `assets/looks/atrium.theme`, even though no entity in the
    // room carries one. Session 9's lesson again: a game whose own asset fails the validator it
    // ships with is worse than one that has no validator.
    app.register_component::<Theme>()?;
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

    // The interface, in `Render` because it is presentation: `ComputedRect` is derived and the draw
    // data goes into a service, so none of it can reach the state hash.
    //
    // **Layout before collection, and the ordering is load-bearing** — `collect_ui` reads the
    // rectangles `layout_ui_system` writes, so the other way round draws an empty interface on the
    // first frame and a one-frame-stale one forever after.
    app.insert_service(FontCache::new());
    app.insert_service(amadeo_render::Overlay::default());

    // The look, from `assets/looks/signage.theme` (ADR 0064). A `.theme` is a scene file holding one
    // `Theme`, exactly as a `.material` and an `.environment` are — and `amadeo-ui` sits below
    // `amadeo-scene`, so it cannot parse its own asset. `App` can see both crates and does the
    // reading.
    //
    // **A theme that will not load is survivable**, and the built-in Signage look is what draws
    // instead — the same fallback `TextureCache` has, for the same reason: a last resort that is
    // itself a file cannot cover the case where files are the problem.
    let wanted: std::collections::BTreeSet<String> =
        std::iter::once("signage".to_string()).collect();
    if let Some((_, theme)) = app
        .read_component_assets::<Theme>(&wanted)
        .into_iter()
        .next()
    {
        app.insert_service(theme);
    }
    app.add_system(Stage::Render, system(LAYOUT_UI, layout_ui_system));
    app.add_system(
        Stage::Render,
        system(COLLECT_UI, collect_ui).after(LAYOUT_UI),
    );

    // The pause menu (ADR 0065). Three pieces of state, all hashed, all ordinary:
    //
    // - `Screen` is this game's, and is the authority (I4 -- what screens exist is genre knowledge).
    // - `Paused` is the engine's, and is projected from `Screen` by `apply_screen`.
    // - `Focus` is `amadeo-ui`'s, and a game with a menu has to install it (ADR 0063).
    app.insert_resource(Screen::default());
    app.insert_resource(Paused::default());
    app.insert_resource(Focus::default());
    app.register_event::<UiActivated>();

    // **In `PreSimulation`, which is the stage that runs while paused.** A toggle that stopped
    // running when the game paused could never unpause it. After `sample_input`, so `just_pressed`
    // means this tick.
    app.add_system(
        Stage::PreSimulation,
        system(APPLY_SCREEN, apply_screen).after(amadeo_input::SAMPLE_INPUT),
    );

    // The two systems that survive a pause, and the only two. `navigate_focus` belongs in
    // `Simulation` on its own merits -- it is hashed state changing in response to hashed input
    // (ADR 0063) -- and `.while_paused()` is what keeps it alive there while the room does not move.
    app.add_system(
        Stage::Simulation,
        system(NAVIGATE_FOCUS, navigate_focus).while_paused(),
    );
    app.add_system(
        Stage::Simulation,
        system(CHOOSE_FROM_MENU, choose_from_menu).while_paused(),
    );

    // The one-shot half (ADR 0059's named gap, filled in session 16). Registering the event is what
    // arranges for its buffers to swap each tick; without that a footstep is sent and never read.
    app.register_event::<SoundPlayed>();
    app.insert_resource(Stride::default());

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

    // **In `PostSimulation`, which puts it inside the deterministic zone on purpose.** Deciding that
    // a footstep happened is gameplay: it depends on how far the character walked, it goes into the
    // state hash as a queued event, and it has to reproduce in a replay. Only the *playing* is
    // machinery, and that is `collect_audio` in the `Render` stage.
    app.add_system(
        Stage::PostSimulation,
        system(PLAY_FOOTSTEPS, play_footsteps).after(PROPAGATE_TRANSFORMS),
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
