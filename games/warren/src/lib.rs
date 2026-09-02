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

use amadeo_app::{App, Paused, Stage, system};
use amadeo_audio::{
    Audio, AudioListener, AudioSource, COLLECT_AUDIO, SoundCache, SoundPlayed, collect_audio,
};
use amadeo_behaviour::{Behaviour, Facts};
use amadeo_character::{CharacterController, CharacterMotion};
use amadeo_core::StableHash;
use amadeo_ecs::{Component, Entity, Resource, World};
use amadeo_events::WorldEvents;
use amadeo_input::{ActionId, InputDriver, InputState, NullSource};
use amadeo_interaction::{Interactable, Interacted, Interactor, Looking};
use amadeo_inventory::{Inventory, Item, StoredIn};
use amadeo_physics::{Collider, Gravity, Physics, RapierPhysics, RigidBody, Velocity};
use amadeo_reflect::Reflect;
use amadeo_render::{Camera, Mesh, PointLight, SpotLight, TextureCache};
use amadeo_transform::{
    GlobalTransform, PROPAGATE_TRANSFORMS, Parent, Transform, propagate_transforms,
};
use amadeo_ui::{
    COLLECT_UI, ComputedRect, Focus, Focusable, FontCache, LAYOUT_UI, Text, UiActivated, UiNode,
    collect_ui, layout_ui_system,
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
///
/// **Chosen rather than picked**, which the previous one was not. The date-shaped seed this game
/// shipped with produced eight rooms in a dead-straight ninety-six-metre line with the key one door
/// from the exit — a bad draw that nothing caught, because nothing had an opinion about what a good
/// layout was. `Layout::shortcomings` now has one, `--bin layout` refuses a seed that fails it, and
/// this is the first seed that passes: fourteen rooms over seven cells by five, an eleven-door
/// journey, and the key ten doors from the door it opens.
pub const GENERATED_SEED: u64 = 3;

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
pub const BEAM_INTENSITY: f32 = 14.0;

/// How hard the hand lamp's housing spills onto what is beside you, when you are carrying it.
///
/// **A small fraction of [`BEAM_INTENSITY`], and it is not a second torch.** Its job is to put a
/// *falloff* on the lining within a few metres, which no ambient term can do. If it reads as a glow
/// around the player, or as a corridor that is bright wherever you happen to stand, it is too high —
/// at 4.2 it was both.
pub const SPILL_INTENSITY: f32 = 2.6;

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

    // **The beam is the spot light on the camera, not "the spot lights".**
    //
    // This used to write every `SpotLight` in the world, which was correct for exactly as long as
    // the torch was the only one. Session 23 made the emergency fittings spots — a point light 0.3 m
    // off its own mounting wall clips it to white at any intensity, and a spot aimed down and away
    // pools instead — and this system immediately started driving them too: every fitting in the
    // level blazed to `BEAM_INTENSITY` when you picked the torch up and went out when you dropped it.
    //
    // It is `docs/14` §4 #9's shape a third time: an authored value silently overwritten at runtime,
    // so the scene's number is dead data and an A/B on it changes nothing. It was found by an A/B
    // that came back byte-identical for the *second* time in one session.
    let Some(eyes) = eyes(world) else {
        return;
    };
    let beams: Vec<Entity> = world
        .query::<(&SpotLight, &Parent)>()
        .filter(|(_, (_, parent))| parent.0 == eyes)
        .map(|(entity, _)| entity)
        .collect();
    for beam in beams {
        if let Some(light) = world.get_mut::<SpotLight>(beam) {
            light.intensity = if holding { BEAM_INTENSITY } else { 0.0 };
        }
    }

    // **And the housing's spill, which is the same switch on a second light.** A hand lamp is a bulb
    // in a can and a can leaks: the beam is what you aim, the spill is what falls on the wall you are
    // standing beside. Engine gate review 19 measured why it has to exist — the lining had no
    // near-to-far gradient at all, because the beam is a 26° cone that only reaches a wall 2.4 m to
    // the side once it is five metres ahead, and the only other thing lighting that wall was an
    // ambient probe which is distance-independent by construction.
    //
    // **It is small, and the first attempt at it was not.** At 4.2 over a 4.5 m range it lit both
    // walls of a 4.8 m bore to about 180 and turned the opening frame into a white tiled corridor —
    // worse than the flat grey it was meant to fix, and it made the picture *more* symmetric rather
    // than less, because a light at the camera lights whatever is around the camera equally.
    //
    // Found by parent rather than by type, for the reason the block above exists: the warden carries
    // a `PointLight` too, and "the only one" is a property of today's content.
    let spills: Vec<Entity> = world
        .query::<(&PointLight, &Parent)>()
        .filter(|(_, (_, parent))| parent.0 == eyes)
        .map(|(entity, _)| entity)
        .collect();
    for spill in spills {
        if let Some(light) = world.get_mut::<PointLight>(spill) {
            light.intensity = if holding { SPILL_INTENSITY } else { 0.0 };
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

/// Puts every failing fitting at a different point in its own flicker.
///
/// # Fifteen tubes flickering in unison is a brightness pulse, not a failing circuit
///
/// `fitting_fail.anim` is 5.4 s long and its flicker occupies **0.44 s of it** — 8% — with one
/// further dip at 4.72. Every instance of `room_lamp` carries the same `AnimationPlayer` at
/// `time 0.0`, so all fifteen in the shipped level ran in lockstep: `amadeo anim` reported all of
/// them at exactly 2.10. The level was therefore either wholly steady or wholly flickering, and 92%
/// of the time it was steady.
///
/// That is why engine gate review 34 swept 300 ticks and found *"the largest luminance change
/// anywhere in the frame is 5 levels"*, and why review 33 diffed ticks 5 and 400 and got a
/// byte-identical frame. **The clip was never the problem.** `docs/11` §8 asks for *one* moving thing
/// in a still frame and says two is a screensaver; fifteen at once is neither, and fifteen at once
/// 8% of the time is nothing at all.
///
/// Staggered, some fitting is always mid-flicker, so the world is never a photograph — and no two
/// tubes fail together, which is what a circuit that has been unattended for forty years looks like.
///
/// # The phase comes from the fitting's own position, not from a die or from an entity id
///
/// It has to be **reproducible**, because `AnimationPlayer::time` is hashed simulation state
/// (ADR 0066) — two machines loading the same level must get the same phases or their state hashes
/// diverge on tick one. A position is authored data and is identical everywhere; an entity id would
/// be reproducible today and would silently change the moment anything altered spawn order.
///
/// It runs here rather than as a system because it is a property of *loading*: a system would have
/// to decide whether each player had already been staggered, and the only cheap test for that —
/// `time == 0.0` — is a value a looping clip can legitimately return to.
fn stagger_the_fittings(world: &mut World) {
    let placed: Vec<(amadeo_ecs::Entity, [f32; 3])> = world
        .query::<(&amadeo_anim::AnimationPlayer, &GlobalTransform)>()
        .filter(|(_, (player, _))| player.clip == "fitting_fail")
        .map(|(entity, (_, at))| (entity, at.translation()))
        .collect();

    for (entity, at) in placed {
        // FNV-1a over the position's bits, which is `StableHasher`'s own mixing function — the
        // point is only that two fittings a bore apart land far from each other in the loop, and
        // that the same fitting lands in the same place on every machine.
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        for value in at {
            for byte in value.to_bits().to_be_bytes() {
                hash ^= u64::from(byte);
                hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
            }
        }
        let phase = (hash % 5400) as f32 / 1000.0;
        if let Some(player) = world.get_mut::<amadeo_anim::AnimationPlayer>(entity) {
            player.time = phase;
        }
    }
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
    // Every shape, material and look the engine can read out of an asset file, in one call — a game
    // whose own assets fail the validator it ships with is worse than one with no validator. Naming
    // them by hand is what left ADR 0074's parametric set unvalidatable in every game.
    app.register_asset_components()?;

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
    // **Animation, and the reason this game has any is that it had none** (ADR 0066). Engine gate
    // review 30 took two captures fifteen seconds of game time apart and found them byte-identical
    // across all 2,073,600 pixels: *"a horror interior in which nothing whatever moves is a still
    // life, and the engine has had the system for four milestones."* `games/atrium` has had a
    // flickering lamp since M2.
    //
    // One clip, on the emergency fittings: a circuit that has been running unattended for forty
    // years and is going. It is a `SpotLight.intensity` track named as a **field in a text file**,
    // so `amadeo-anim` knows nothing about lights and adding a second animated property later is a
    // scene edit rather than engine work.
    //
    // **`Animatable` is an allow-list and forgetting a line here is loud** — an unallowed target is
    // reported by name in `Animatable::missing` rather than silently doing nothing (ADR 0066 §4).
    let mut animatable = amadeo_anim::Animatable::new();
    animatable.allow::<amadeo_render::SpotLight>();
    app.insert_service(animatable);
    app.register_component::<amadeo_anim::AnimationPlayer>()?;
    app.register_component::<amadeo_anim::AnimationClip>()?;
    app.register_event::<amadeo_anim::AnimationFinished>();
    // In `Simulation`, for the reason `games/atrium` gives: a clip that wrote a `Transform` would be
    // read by physics and `propagate_transforms` in the same tick, and running it later would apply
    // this tick's animation to next tick's physics.
    //
    // **And `.while_paused()`, because the first screen a player sees is a paused one.** `Screen`
    // projects `Title`, `Paused` and `Ended` onto the engine's `Paused`, which by ADR 0065 skips
    // `Simulation` for every system that has not opted in -- so the fittings were frozen at t=0.02
    // behind the title, and 6.7 seconds of it changed **zero pixels**. Engine gate review 32
    // measured `--ticks 5` and `--ticks 400` byte-identical and called the first screen a
    // photograph; `docs/11` Â§8 asks for exactly one moving thing on it.
    //
    // This is the opt-in ADR 0065 exists for, and it is the game's call rather than the engine's:
    // the only clip in the Warren drives a `SpotLight.intensity`, so what keeps running behind a
    // pause menu is a failing tube rather than anything a player could act on. **A clip that moved a
    // `Transform` would not take this line** -- animation is simulation (ADR 0066), so a moving
    // platform running under a pause menu is a platform you could be carried off by while reading it.
    app.add_system(
        Stage::Simulation,
        system(amadeo_anim::ANIMATE, amadeo_anim::animate).while_paused(),
    );

    amadeo_behaviour::install(&mut app)?;

    // This game's own marks, and the one thing it can end as.
    app.register_component::<WayOut>()?;
    app.register_component::<Warden>()?;
    // What the floor is made of, so a step on timber does not sound like a step on concrete (F6 b).
    app.register_component::<Footing>()?;
    app.register_component::<Socket>()?;
    app.register_component::<PromptLine>()?;
    app.register_component::<Reticle>()?;
    app.register_component::<EndingLine>()?;
    app.register_component::<Verdict>()?;
    app.insert_resource(Outcome::default());

    // The shell (ADR 0065). `Screen` is this game's and is the **authority**; the engine's `Paused`
    // is projected from it by `apply_screen`, which is what stops a menu hanging over a running
    // game. `Requested` records what the menu asked the disk for; nothing inside a tick does it.
    app.register_component::<Menu>()?;
    app.register_component::<MenuButton>()?;
    app.insert_resource(Screen::default());
    app.insert_resource(Requested::default());
    // **The engine's `Paused`, and forgetting this line is silent.** `apply_screen` projects the
    // screen onto it with `resource_mut`, which hands back `None` when the resource was never
    // inserted — so every pause was a no-op, the world kept simulating behind the title screen, and
    // the only visible sign was that the view still turned with the mouse. Justin found it by
    // playing the game, which is the only way it could have been found: the test that should have
    // caught it checked the player's *translation*, and a player with no input does not move.
    app.insert_resource(Paused::default());

    // The interface (ADR 0062). `ComputedRect` is registered although nothing authors one, because
    // it is a component an agent should be able to *see* — "where did that line end up" is the
    // question `world.entity` exists to answer.
    app.register_component::<UiNode>()?;
    app.register_component::<ComputedRect>()?;
    app.register_component::<Text>()?;
    app.register_component::<amadeo_ui::Panel>()?;
    app.register_component::<Focusable>()?;
    app.insert_resource(Focus::default());
    app.register_event::<UiActivated>();

    // **Layout before collection, and the ordering is load-bearing**: `collect_ui` reads the
    // rectangles `layout_ui_system` writes, so the other way round draws an empty interface on the
    // first frame and a one-frame-stale one forever after.
    //
    // No `Theme` asset — this game ships none, so the built-in Signage look draws it. That is
    // `TextureCache`'s argument again: a last resort that is itself a file cannot cover the case
    // where files are the problem.
    app.insert_service(FontCache::new());
    app.insert_service(amadeo_render::Overlay::default());

    // Sound (ADR 0059). **The ears go on the camera**, which in a first-person game is also your
    // head — so the argument the Atrium had to make (third person, and the viewer should hear what
    // they can see) does not even arise here. `AudioListener` is authored on the same entity as the
    // `Camera` in `player_start.scene`.
    app.register_component::<AudioSource>()?;
    app.register_component::<AudioListener>()?;
    app.register_event::<SoundPlayed>();
    app.insert_service(SoundCache::new());
    app.insert_resource(Stride::default());
    app.insert_resource(Sounded::default());

    // **`NullAudio` here and kira in the windowed build**, the same split the renderer has: a
    // headless run, a test and the agent all get a backend that remembers frames instead of making
    // a noise, and `main.rs` swaps in the one with a speaker behind it.
    app.insert_service(Audio::headless());

    // Input is sampled before anything reads it, which is what `PreSimulation` is for. Both the
    // character and the camera read *named actions*, so this is the only place in the game that
    // knows a keyboard or a mouse exists.
    app.add_system(
        Stage::PreSimulation,
        system(amadeo_input::SAMPLE_INPUT, amadeo_input::sample_input),
    );
    // In `PreSimulation`, because that stage runs whether or not the game is paused — a system that
    // stopped while paused could never unpause. After the sample, so `just_pressed` means this tick.
    app.add_system(
        Stage::PreSimulation,
        system(APPLY_SCREEN, apply_screen).after(amadeo_input::SAMPLE_INPUT),
    );

    let document = amadeo_scene::parse(scene)
        .map_err(|error| anyhow::anyhow!("games/warren/scenes/: {error}"))?;
    app.load_scene(&document)?;
    stagger_the_fittings(&mut app.world);

    // **`InputState` before the snapshot below, and this line is load-bearing.**
    //
    // `amadeo_input::install` inserts it, and every caller of this function installs a driver
    // *afterwards* — so without this the snapshot records a world with no `InputState`, and
    // `InputState` is a hashed resource. Restoring it then rebuilds a world that is genuinely
    // different from the one the file describes, and ADR 0069's integrity check refuses it with
    // "something about this build differs from the one that took the snapshot". Which was true, and
    // was this.
    //
    // Inserting it here is harmless twice over: `install` replaces it with an identical default a
    // moment later, and a game with no driver at all still wants somewhere for input to be.
    app.insert_resource(amadeo_input::InputState::new());

    // **The world exactly as it loaded**, kept so a run can be started over. Captured here rather
    // than by the caller because this is the only moment it is true: one tick later the player has
    // begun to fall, and a "fresh start" that restored a world mid-fall would be subtly not one.
    let fresh = amadeo_snapshot::to_text(&app.capture_snapshot());
    app.insert_service(FreshStart(fresh));

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
        system(WATCH_FOR_YOU, watch_for_you)
            .before(amadeo_behaviour::RUN_BEHAVIOURS)
            // **After the step, or the sight cast queries an empty index.** ADR 0054's rule: a
            // backend answers from an index `step_physics` builds, so asking first finds nothing in
            // the way anywhere -- which is exactly the defect this cast exists to fix, restored.
            .after(amadeo_physics::STEP_PHYSICS),
    );
    app.add_system(
        Stage::Simulation,
        system(MOVE_THE_WARDEN, move_the_warden)
            .after(amadeo_behaviour::RUN_BEHAVIOURS)
            .after(amadeo_physics::STEP_PHYSICS),
    );
    // What it sounds like follows from what its machine settled into, so this runs after the machine
    // and after the thing that moves it.
    app.add_system(
        Stage::Simulation,
        system(VOICE_THE_WARDEN, voice_the_warden).after(MOVE_THE_WARDEN),
    );
    // Last, so it judges where everything actually ended up this tick rather than where it was.
    app.add_system(
        Stage::PostSimulation,
        system(SETTLE_THE_RUN, settle_the_run).after(PROPAGATE_TRANSFORMS),
    );
    // After the run is settled, so the ending appears on the tick it happens rather than the next.
    //
    // **And `.while_paused()`, which is not optional** (ADR 0065). The tick a run ends on is the
    // last tick this stage runs at all: `apply_screen` sees the outcome next tick and freezes the
    // world, so a HUD that stopped with it would keep whatever it said at the moment of dying —
    // leaving "unlock the door and leave" sitting under the ending, which reads as the game still
    // running. Everything it writes is a pure function of hashed state, so running it while paused
    // is safe as well as necessary.
    app.add_system(
        Stage::PostSimulation,
        system(WRITE_THE_HUD, write_the_hud)
            .after(SETTLE_THE_RUN)
            .while_paused(),
    );

    // **The two menu systems, and both must run while paused** (ADR 0065). Everything they respond
    // to happens while the world is frozen — and on the title screen the world has never run at all,
    // so without `.while_paused()` the game could not be started, let alone unpaused.
    app.add_system(
        Stage::Simulation,
        system(amadeo_ui::NAVIGATE_FOCUS, amadeo_ui::navigate_focus).while_paused(),
    );
    app.add_system(
        Stage::Simulation,
        system(CHOOSE_FROM_MENU, choose_from_menu).while_paused(),
    );

    // **After `propagate_transforms`**, so a footstep is placed where the character ended up this
    // tick rather than where it was last tick — and so `play_the_run` can read the composed
    // transform of a thing on a plinth, which is a prefab child and has no world position of its
    // own.
    app.add_system(
        Stage::PostSimulation,
        system(PLAY_FOOTSTEPS, play_footsteps).after(PROPAGATE_TRANSFORMS),
    );
    // After the run is settled, so the ending sounds on the tick it happens. **Not
    // `.while_paused()`**, unlike the HUD: a sting is a one-shot and is emitted on the tick the
    // outcome changes, which is the last tick this stage runs — and a system that kept running
    // would keep re-reading an `Interacted` event queue nothing is filling.
    app.add_system(
        Stage::PostSimulation,
        system(PLAY_THE_RUN, play_the_run)
            .after(SETTLE_THE_RUN)
            .after(TAKE_WHAT_YOU_USED),
    );

    // **How blocked each sound is, written before the frame is collected** (ADR 0086). `docs/11` §9
    // makes this a gameplay requirement rather than polish — *"a warden exactly as loud through a
    // wall as through a doorway makes the whole mechanic a lie"* — and this game is made of
    // corridors.
    //
    // **`PostSimulation`, which is after `step_physics` by STAGE rather than by label.** `cast_shape`
    // answers from an index the step builds, so casting earlier reports every path clear -- the defect
    // itself, restored silently. An `.after(STEP_PHYSICS)` cannot express it: that label is registered
    // in `Simulation` and a dependency across stages does not resolve, which fails loudly as
    // `UnknownLabel` rather than quietly. Stage order is the guarantee here.
    //
    // After `propagate_transforms` as well, because a listener on a camera is a child and its world
    // position is only correct once the chain is composed. `occlusion` is a hashed component field, so
    // this sits deliberately **inside** the deterministic zone even though what it feeds does not.
    app.add_system(
        Stage::PostSimulation,
        system(amadeo_app::OCCLUDE_VOICES, amadeo_app::occlude_voices).after(PROPAGATE_TRANSFORMS),
    );

    // Collected in `Render`, where nothing it does can reach the state hash — `Audio` is a Service,
    // and ADR 0009 puts those outside it. What the game decided to *play* was decided above, in the
    // deterministic zone.
    app.add_system(Stage::Render, system(COLLECT_AUDIO, collect_audio));

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

/// How far along the bore, from its centre, the warden's post stands.
///
/// # It is 1.5 and not [`PROP_OFFSET`], so that a fitting is BEHIND it
///
/// A section's two fittings sit at `FITTING_OFFSET` of ±3.0 along the bore, and the landmark
/// camera stands back along the same axis. At the old 3.4 the warden was *past* the near fitting, so
/// that fitting sat between the camera and the figure and lit the side facing the lens — engine gate
/// review 33 measured the result and it is the whole of why the outline fails: the lower two thirds
/// of the warden read **8.5 against a background of 8.3**, *"not silhouetted, unlit in front of
/// unlit"*.
///
/// At 1.5 the figure stands **1.5 m in front of** the fitting at `-3.0`, on the same haunch, so the
/// light is behind it and the coat is a shape against a lit wall. That is `docs/11` §4's own frame —
/// *the silhouette appears at the pool's edge, passes through, and is gone* — and it is the
/// designer's suggestion S1, which review 33 endorsed rather than overruled.
///
/// **Turning the camera cannot substitute for this and `Side::South` is not the answer**: the post is
/// south of its cell centre, so a south-side camera at the landmark distance stands beyond the
/// bulkhead and photographs the level from outside it. Tried, reverted, recorded.
pub const WARDEN_ALONG: f32 = 1.5;

/// How high the warden's eye sits above its own origin, in metres.
///
/// The void under the brim is at local `0.98` in `warden_head.mesh`; this is the same place, and a
/// sight line is cast from here rather than from the floor so a bunk does not blind it.
///
/// **It rose from 0.55 when the figure became 2.15 m tall** (designer direction 2, D2). The old
/// number was the head height of a 1.80 m figure and moving the mesh without moving this would have
/// left the thing looking out of its own chest -- which nothing would have reported, because a sight
/// line has no picture. It still cannot see across a bore wall: the lining reaches 2.3 m to the
/// springing and the eye is at 1.91 m of world height.
pub const WARDEN_EYE: f32 = 0.98;

/// How high the player's eye sits above their origin, in metres.
///
/// The sight line ends here rather than at the feet, for the mirror of [`WARDEN_EYE`]'s reason: a
/// line to the floor is blocked by every duckboard between the two.
pub const PLAYER_EYE: f32 = 0.6;

/// The radius of the sight probe, in metres.
///
/// **Not a zero-width ray**, for `modules/amadeo-interaction`'s reason: a hairline ray threads the
/// gap between two lining plates and reports a clear line where a person would see a wall. This is
/// how much of a gap counts as a gap.
pub const SIGHT_PROBE: f32 = 0.12;

/// The radius of the capsule the warden walks with, in metres.
///
/// Narrower than the coat it wears — the greatcoat's hem is 0.36 and this is what fits through a
/// doorway. A collider matching the widest part of a costume is how a figure gets stuck on nothing.
pub const WARDEN_RADIUS: f32 = 0.28;

/// The straight section of that capsule, in metres. Total height is this plus twice the radius.
pub const WARDEN_BODY: f32 = 1.1;

/// The tallest thing it steps over rather than stopping at, in metres.
///
/// A duckboard is 40 mm and a threshold is more; this clears both.
pub const WARDEN_STEP: f32 = 0.35;

/// How far below itself it looks for floor to stay stuck to, in metres.
///
/// Without it, walking off a duckboard launches it, which reads as a hovering figure.
pub const WARDEN_SNAP: f32 = 0.3;

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

/// The yaw that points a thing along a horizontal direction, in degrees.
///
/// **Yaw 0 faces −z**, which is [`facing`]'s own convention: `Side::North` is `z − 1` and
/// `facing(North)` is `0.0`. So the direction a yaw θ looks along is `(−sin θ, −cos θ)`, and
/// inverting that is `atan2(−dx, −dz)` rather than the `atan2(dx, dz)` that looks right. Getting it
/// wrong turns the warden exactly around, which reads as a figure walking at you backwards.
fn facing_along(toward: [f32; 2]) -> f32 {
    (-toward[0]).atan2(-toward[1]).to_degrees()
}

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
    let near: Vec<(Entity, [f32; 3])> = world
        .query::<(&Warden, &Transform)>()
        .filter(|(_, (_, at))| distance(at.translation, you) <= WARDEN_SIGHT)
        .map(|(entity, (_, at))| (entity, at.translation))
        .collect();

    // Distance is necessary and was never sufficient. Every warden inside the radius now has to
    // *see* the player as well, which is the second half and the one that makes a wall mean
    // something.
    let seen: Vec<(Entity, bool)> = near
        .into_iter()
        .map(|(entity, at)| (entity, can_see(world, at, you)))
        .collect();

    // Everything out of range is cleared, so walking away still ends a pursuit.
    let all: Vec<Entity> = world
        .query::<(&Warden,)>()
        .map(|(entity, _)| entity)
        .collect();
    for entity in all {
        let sees = seen.iter().any(|(seen, yes)| *seen == entity && *yes);
        if let Some(facts) = world.get_mut::<Facts>(entity) {
            facts.set("sees_you", sees);
        }
    }
}

/// Whether there is an unobstructed line from the warden's eye to the player's.
///
/// # This is what `ShapeHit::entity` was added for
///
/// Its own doc comment says so: *"an AI's line of sight asks whether the thing in the way is the
/// player or a wall."* The field landed in session 17 and nothing used it for this until engine gate
/// review 28 pointed out that the antagonist of a horror game sees through cast-iron bulkheads.
///
/// # A sphere rather than a ray, for `modules/amadeo-interaction`'s reason
///
/// A zero-width ray threads the gap between two lining plates and reports open air where a person
/// sees a wall. [`SIGHT_PROBE`] is how much of a gap counts as a gap.
///
/// # No solver means every line is clear, and that is stated rather than hidden
///
/// Against `NullPhysics` every cast reports clear, so the warden sees through walls again — the same
/// control case `modules/amadeo-character` asserts, where the character walks through them. The
/// tests say so out loud rather than quietly passing.
fn can_see(world: &World, from: [f32; 3], to: [f32; 3]) -> bool {
    let Some(physics) = world.service::<Physics>() else {
        return true;
    };
    let eye = [from[0], from[1] + WARDEN_EYE, from[2]];
    let head = [to[0], to[1] + PLAYER_EYE, to[2]];
    let motion = [head[0] - eye[0], head[1] - eye[1], head[2] - eye[2]];

    let cast = amadeo_physics::ShapeCast {
        skin: 0.0,
        ..amadeo_physics::ShapeCast::new(
            amadeo_physics::Shape::Sphere {
                radius: SIGHT_PROBE,
            },
            eye,
            motion,
        )
    };
    match physics.cast_shape(&cast) {
        // Nothing in the way at all.
        None => true,
        // Something is. It only fails to block if it *is* the player — a cast that stops on the
        // thing it was aimed at is a clear line, and `None` here would mean static level geometry.
        Some(hit) => hit.entity.is_some() && hit.entity == player(world),
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

    // Where each one ends up, decided against the level before anything is written. Collected in
    // one pass because `move_shape` needs the service mutably and writing a `Transform` needs the
    // world mutably, and those cannot be held at once.
    let mut arrivals: Vec<(Entity, [f32; 3], f32)> = Vec::new();
    if let Some(physics) = world.service_mut::<Physics>() {
        for (entity, at) in &moving {
            let gap = distance(*at, you);
            if gap <= f32::EPSILON {
                continue;
            }
            let step = WARDEN_SPEED * amadeo_core::FIXED_DT;
            // Horizontal only, so it walks the floor rather than swimming towards your eyes.
            let toward = [(you[0] - at[0]) / gap, (you[2] - at[2]) / gap];

            // **Moved by the same query a character is moved by, so a wall stops it.** It used to
            // write the translation straight in, which is why it walked through cast iron for six
            // milestones. `move_shape` slides along whatever it hits, which is all a warden needs:
            // it does not have to be clever, it has to not be a ghost.
            let request = amadeo_physics::ShapeMove {
                max_slope_degrees: 55.0,
                step_height: WARDEN_STEP,
                snap_distance: WARDEN_SNAP,
                ..amadeo_physics::ShapeMove::new(
                    amadeo_physics::Shape::Capsule {
                        radius: WARDEN_RADIUS,
                        height: WARDEN_BODY,
                    },
                    *at,
                    [toward[0] * step, 0.0, toward[1] * step],
                )
            };
            let moved = physics.move_shape(&request);
            // **Facing where it is going.** Nothing turned it before, so the coat's closure, its
            // strap and the lamp it carries were all on whichever side the level generator happened
            // to leave facing away. A thing that walks at you while pointing elsewhere is not
            // frightening, it is broken.
            let facing = facing_along(toward);
            arrivals.push((*entity, moved.translation, facing));
        }
    } else {
        // No solver: the old straight-line walk, which is the control case the tests assert.
        for (entity, at) in &moving {
            let gap = distance(*at, you);
            if gap <= f32::EPSILON {
                continue;
            }
            let step = WARDEN_SPEED * amadeo_core::FIXED_DT;
            let toward = [(you[0] - at[0]) / gap, (you[2] - at[2]) / gap];
            let facing = facing_along(toward);
            arrivals.push((
                *entity,
                [at[0] + toward[0] * step, at[1], at[2] + toward[1] * step],
                facing,
            ));
        }
    }

    for (entity, arrived, facing) in arrivals {
        if let Some(transform) = world.get_mut::<Transform>(entity) {
            transform.translation = arrived;
            transform.rotation[1] = facing;
        }
    }
}

/// The id of the loop the warden makes when it has not seen you.
pub const WARDEN_TREAD: &str = "warden_tread";

/// The id of the loop it makes when it has.
pub const WARDEN_BREATH: &str = "warden_breath";

/// The id of the room tone.
pub const WARREN_TONE: &str = "warren_tone";

/// The room tone's gain when the warden is as far away as it can see.
pub const TONE_FAR: f32 = 0.55;

/// The room tone's gain when it is on top of you.
///
/// **At least 2:1 against [`TONE_FAR`]**, which is `docs/13` §1b's F6 clause (c). The bed is the one
/// sound that is always there, so it is the only one that can tell you something without competing
/// with anything — and `docs/11` §9 wants near-silence as the default, which means the *rise* has to
/// carry the information rather than the level.
pub const TONE_NEAR: f32 = 1.2;

/// The label [`voice_the_warden`] is registered under.
pub const VOICE_THE_WARDEN: &str = "voice_the_warden";

/// Swaps what the warden sounds like, and leans on the room tone as it closes.
///
/// # Two things rather than one, because they are one cue
///
/// Design direction 1 (`docs/15` §5) took the breath off the warden's constant channel: a thing that
/// breathes continuously reads as an animal, and `docs/11` §3 says it is an institution. So it treads
/// while it has not seen you and breathes only while it has — which makes the change of sound itself
/// the moment you have been noticed, with nothing on screen having to say so.
///
/// The bed does the ranged half. `docs/11` §9 wants near-silence as the default *"so that a single
/// sound is an event"*, and a bed that leans up as the warden closes is the cheapest way to be told
/// something is coming without being told where — which is the tension the whole game is built on.
///
/// **Both are written into hashed component fields**, so a save restores the state of the chase
/// rather than resetting it to calm, and a replay reproduces it.
pub fn voice_the_warden(world: &mut World) {
    let mut nearest = f32::INFINITY;
    let wardens: Vec<(Entity, bool, [f32; 3])> = world
        .query::<(&Warden, &Behaviour, &Transform)>()
        .map(|(entity, (_, mind, at))| (entity, mind.state == "pursue", at.translation))
        .collect();

    if let Some(you) = player_at(world) {
        for (_, _, at) in &wardens {
            nearest = nearest.min(distance(*at, you));
        }
    }

    for (entity, pursuing, _) in wardens {
        let wanted = if pursuing {
            WARDEN_BREATH
        } else {
            WARDEN_TREAD
        };
        if let Some(source) = world.get_mut::<AudioSource>(entity)
            && source.sound != wanted
        {
            // A clip swap is a stop and a start to `VoiceTracker`, never an update — which is what
            // makes the change audible as a change rather than as a crossfade.
            source.sound = wanted.to_string();
        }
    }

    // Linear between arm's length and the edge of its sight. Outside that the bed sits at `TONE_FAR`,
    // which is what the game sounds like when nothing is looking for you.
    let lean = if nearest.is_finite() {
        let span = (WARDEN_SIGHT - WARDEN_REACH).max(0.001);
        let along = ((nearest - WARDEN_REACH) / span).clamp(0.0, 1.0);
        TONE_NEAR + (TONE_FAR - TONE_NEAR) * along
    } else {
        TONE_FAR
    };

    let beds: Vec<Entity> = world
        .query::<(&AudioSource,)>()
        .filter(|(_, (source,))| source.sound == WARREN_TONE)
        .map(|(entity, _)| entity)
        .collect();
    for bed in beds {
        if let Some(source) = world.get_mut::<AudioSource>(bed) {
            source.gain = lean;
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
    // **The label the shelter would have put on it**, not the verb -- design direction 1, decision 4.
    // `Locked` is a game state told to the player; `SECURED` is the institution's own word for the
    // same fact and is what is stencilled on a real bulkhead. `WAY OUT` is kept exactly: it is London
    // Underground's own phrase for an exit, which is this game's precise typology.
    let wanted = if has_key {
        "WAY OUT"
    } else {
        "WAY OUT · SECURED"
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
    /// What state this length of bore was left in.
    pub condition: Condition,
}

/// What was done to a length of bore before you got here — `docs/11` §5.2's binding rule.
///
/// > **Rooms may repeat. No two may be in the same condition.**
///
/// That sentence replaces the withdrawn "no two spaces the same size", and it is the thing that
/// stops a repeated piece reading as machine-made. It matters *more* in a tube than it did in rooms:
/// a room's repetition is only visible across a playthrough, and a bore's repetition is visible down
/// its own length in a single frame.
///
/// **Deliberately cheap.** A condition is dressing and nothing else — which props stand in a section
/// — so it costs no new geometry, no new topology and no change to the room graph. §5.2 names seven;
/// three are drawn here, and the two that need more than dressing (flooded needs a second floor
/// material, burnt out needs soot) wait until they can be done properly rather than being faked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Condition {
    /// Bunks still made up, bedding on them. Somebody was living here when it stopped.
    #[default]
    SleptIn,
    /// The frames are here and the bedding is gone. Somebody left, and took what was worth taking.
    Stripped,
    /// Cleared out entirely and used for storage. Crates, no bunks.
    Stores,
    /// The water got in. Standing water over the screed, and the one reflective surface in the game.
    ///
    /// `docs/11` §6 is specific about why this is roughness 0.35 with a little metallic rather than
    /// a mirror: it would be reflecting a deliberately near-black ambient, so a polished floor would
    /// read as a *hole*. Its read comes from the hand lamp's specular — the one surface that shows
    /// you where you are by throwing your own light back at you.
    Flooded,
    /// The lining came in. Ring sections out of true, and the spoil under them.
    ///
    /// `docs/11` §10 chose this shape over "debris" and said why: *"A pile of boxes reads as a pile
    /// of boxes; a tunnel ring that has come out of true reads as a tunnel that has failed"* — and it
    /// is the same primitive the walls are already made of.
    Collapsed,
    /// Re-racked as an archive: the frames raised and loaded, the bunks never taken out.
    ///
    /// `docs/11` §2's best environmental-storytelling idea — one object, two eras, readable at a
    /// glance — and it costs nothing but a placement, because racking *is* a bunk frame.
    Archive,
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

    /// Everything about this layout that would make it a poor level to play, in plain sentences.
    ///
    /// # Why a generator needs this and why it did not have it
    ///
    /// `is_connected` and `has_loop` check that a layout is *valid*. Nothing checked that it was any
    /// **good** — and the seed this game shipped with was a bad draw: the key landed one door from
    /// the door it opens, so a player walked ninety-six metres in a straight line, picked the key up
    /// next door to the exit, and used it. There was no fetch in the level at all.
    ///
    /// It shipped because a bad layout looks exactly like a good one from the outside. It loads, it
    /// validates, every test passes, and the capture is a room. **The only thing that can tell them
    /// apart is a rule written down**, which is what this is.
    ///
    /// Returned as sentences rather than a `bool` because "this seed is bad" is not actionable and
    /// "the key is 1 door from the exit; it should be at least 3" is.
    #[must_use]
    pub fn shortcomings(&self) -> Vec<String> {
        let mut found = Vec::new();
        let marks = self.landmarks;
        let from_start = distances(&self.rooms, marks.start);
        let from_exit = distances(&self.rooms, marks.exit);
        let steps = |table: &[((i32, i32), u32)], cell| steps_to(table, cell).unwrap_or(0);

        if !self.is_connected() {
            found.push("some rooms cannot be reached from the start".to_string());
        }
        if !self.has_loop() {
            found.push(
                "there is no loop, so every dead end has to be backtracked out of".to_string(),
            );
        }

        // **The one that shipped.** A key beside the door it opens is a lock with its own key taped
        // to it. Three doors is not far; it is far enough that fetching it is a journey.
        let key_to_exit = steps(&from_exit, marks.key);
        if key_to_exit < 3 {
            found.push(format!(
                "the key is {key_to_exit} door(s) from the exit; it wants at least 3, or opening \
                 the door is not something the player travelled for"
            ));
        }

        // A key you trip over on the way out of the first room is not found, it is handed to you.
        let key_from_start = steps(&from_start, marks.key);
        if key_from_start < 3 {
            found.push(format!(
                "the key is {key_from_start} door(s) from the start, which is close enough to pick \
                 up before the level has begun"
            ));
        }

        // Two objectives in one room is one room doing two jobs and every other room doing none.
        if marks.key == marks.torch {
            found.push("the key and the torch are in the same room".to_string());
        }

        // The whole journey. Anything shorter is a corridor with a door at the end.
        let journey = steps(&from_start, marks.exit);
        if journey < 4 {
            found.push(format!("the exit is only {journey} door(s) from the start"));
        }

        found
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

    let mut rooms: Vec<PlacedRoom> = cells
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
                // Filled in by `light_the_sections` below, once every room has a condition —
                // because which fittings work is decided by what happened to the section.
                lit: true,
                // Filled in below, once every room's neighbours are known.
                condition: Condition::SleptIn,
            }
        })
        .collect();

    assign_conditions(&mut rooms);

    light_the_sections(&mut rooms);

    let landmarks = choose_landmarks(&rooms);
    Layout {
        seed,
        rooms,
        landmarks,
    }
}

/// Decides which fittings are still on the circuit — **by what happened to the section, not by a
/// die**.
///
/// # The draw this replaces, and why the rule is better than the number
///
/// It used to be `cell == START || rng.chance(LAMPS_WORKING)`: an independent coin per section at
/// 45%. On the shipped seed that lit six sections of fourteen, and engine gate review 19 measured
/// the consequence — eight sections with no light source in them at all, three of eight submitted
/// frames containing no light source, and a tunnel lining lit by the ambient probe and nothing else.
///
/// The fix is not a higher probability. A coin gives no *reason*, and `assign_conditions` already
/// made this argument one field along: a die roll is what makes a generator look like a generator.
/// So a fitting is dark where the section it hangs in has **failed structurally** — the water got in,
/// or the lining came down. Both of those are conditions a player can see from the doorway, so the
/// dark stretches have a cause that is visible in the same frame as the dark.
///
/// That leaves roughly ten sections of fourteen lit, with pools and real dark between them rather
/// than eight-of-fourteen blackout, and the sections that *are* dark are the ones a player would
/// expect to be. The start is always lit whatever happened to it: waking in the pitch dark before
/// finding the torch is not atmosphere, it is a player who cannot tell the game has started.
fn light_the_sections(rooms: &mut [PlacedRoom]) {
    for room in rooms.iter_mut() {
        let dark = matches!(
            room.condition,
            Condition::Flooded | Condition::Collapsed | Condition::Stripped
        );
        room.lit = room.cell == START || !dark;
    }
}

/// The cell every walk begins at, which is also where the player wakes up.
pub const START: (i32, i32) = (0, 0);

/// Gives every section a condition, **by rule rather than by die** — `docs/11` §5.2.
///
/// # A die roll is what makes a generator look like a generator
///
/// The first version was `rng.below_u32(3)`, three ways, independent per room. Engine gate review 15
/// pointed out that this contradicts the very sentence it was built to satisfy: *"Rooms may repeat.
/// No two may be in the same condition."* An i.i.d. draw over fourteen rooms puts two of the same
/// state next to each other about a third of the time, and a run of three is not rare — and in a
/// tube, where you see several sections down the length in one frame, adjacent repeats are exactly
/// what reads as machine-made.
///
/// So the rule is: **walk the rooms in cell order and give each one a condition none of its already-
/// placed neighbours has.** Ties break towards the least-used condition so far, which spreads the
/// three evenly without counting anything twice. No randomness at all, which also means this cannot
/// disturb `lit`'s draws — the sequence a seeded `Rng` produces is part of a level's identity (I3).
///
/// It is not the mission-first generator `docs/11` §5.1 asks for; that is §10's item 3 and a larger
/// job. It is the property that section asked for, at the cost of one pass over a sorted list.
fn assign_conditions(rooms: &mut [PlacedRoom]) {
    const ORDER: [Condition; 6] = [
        Condition::SleptIn,
        Condition::Stripped,
        Condition::Stores,
        Condition::Flooded,
        Condition::Collapsed,
        Condition::Archive,
    ];

    // How many rooms already carry each condition, so ties go to the one used least.
    let mut used = [0usize; ORDER.len()];

    for index in 0..rooms.len() {
        let cell = rooms[index].cell;

        // What the neighbours already decided. Only rooms *before* this one in cell order have been
        // assigned, which is what makes one pass enough.
        let mut taken = [false; ORDER.len()];
        for side in Side::ALL {
            let at = side.step(cell);
            if let Some(neighbour) = rooms[..index].iter().find(|room| room.cell == at) {
                let which = ORDER
                    .iter()
                    .position(|candidate| *candidate == neighbour.condition)
                    .unwrap_or(0);
                taken[which] = true;
            }
        }

        // The least-used condition no neighbour has; if a room is boxed in by all of them, the
        // least-used one regardless, because a level is better than a panic.
        let pick = (0..ORDER.len())
            .filter(|which| !taken[*which])
            .min_by_key(|which| (used[*which], *which))
            .unwrap_or_else(|| {
                (0..ORDER.len())
                    .min_by_key(|which| (used[*which], *which))
                    .unwrap_or(0)
            });

        rooms[index].condition = ORDER[pick];
        used[pick] += 1;
    }
}

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

/// The highest-scoring item, with ties going to the **first** — which, since every table here is
/// sorted by cell, means the lowest cell.
///
/// # Why not `max_by_key`
///
/// Because `max_by_key` returns the **last** maximum, and several comments in this file claimed the
/// opposite ("ties go to the lowest cell", "the lowest cell wins a tie"). They were wrong, and it
/// was not cosmetic: the key-placement rule below ties on every room in a branchless layout, so the
/// tie-break *was* the rule, and it silently chose the room with the highest coordinate.
///
/// Written out rather than reached for with `min_by_key` on a negated score, which would need signed
/// arithmetic on unsigned step counts and would be the sort of cleverness that hides this again.
fn best<T, K: Ord>(items: impl Iterator<Item = T>, score: impl Fn(&T) -> K) -> Option<T> {
    let mut best: Option<(K, T)> = None;
    for item in items {
        let key = score(&item);
        match &best {
            Some((current, _)) if *current >= key => {}
            _ => best = Some((key, item)),
        }
    }
    best.map(|(_, item)| item)
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

    let exit = best(from_start.iter(), |(_, steps)| (*steps, 0)).map_or(start, |(cell, _)| *cell);
    let from_exit = distances(rooms, exit);

    // **Two scores, and the second one is the fix.**
    //
    // The first is the detour: furthest from the start and from the exit *at once*. Everywhere on a
    // shortest route between the two scores the same total, so a room that scores higher is off that
    // route — which is the whole point of fetching a key.
    //
    // That is right when the graph has branches and **silently useless when it does not**. A layout
    // that is a tree plus one closing edge, with both arcs the same length, gives *every* room the
    // same total — so the choice falls entirely to the tie-break, and the tie-break used to be
    // whatever `max_by_key` happened to return. It returns the **last** maximum, the table is sorted
    // by cell, so the winner was the highest coordinate: reliably a room near the exit.
    //
    // On the seed this game shipped with, that put the key **one door from the door it opens**. The
    // level had no fetch in it at all, and nothing noticed, because the rule's *docs* said the
    // opposite and no test compared the two.
    //
    // So the second score is distance from the exit. When the detour is real it changes nothing;
    // when every room ties, it turns "somewhere near the exit" into "as far from the exit as this
    // layout allows", which is the best a branchless graph can do.
    let key = best(
        from_start
            .iter()
            .filter(|(cell, _)| *cell != start && *cell != exit),
        |(cell, steps)| {
            let from_the_exit = steps_to(&from_exit, *cell).unwrap_or(0);
            (steps + from_the_exit, from_the_exit)
        },
    )
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

/// Half the width of the bore, in world units — so the side walls stand this far off the centreline.
///
/// **The bore does not fill its cell, and that is the architecture.** A cell is 12 m and the tube is
/// 4.8 m, which leaves 3.6 m of ground either side; a door on an east or west side is therefore not
/// an opening between two rooms but a **cross-passage** through that ground, which is exactly what
/// `docs/11` §5.2 asks for and could not be had while a room filled its cell.
pub const BORE_HALF_WIDTH: f32 = 2.4;

/// How far a head sits inside the end of its own bore.
///
/// **Not zero, and the reason is two rooms with a wall between them.** Each bore caps its own end, so
/// two cells that do not share a door put a head on either side of the same plane. Coincident plates
/// z-fight; inset by half a plate's thickness they sit back to back, which is also what a real pair
/// of bulkheads does.
const HEAD_INSET: f32 = 0.125;

/// The prefab twelve metres of bore comes from — deck and crown, no sides.
pub const ROOM_PIECE: &str = "bore_section";

/// The prefab a side wall with a cross-passage through it comes from.
pub const DOORWAY_PIECE_A: &str = "bore_wall_open_a";

/// The same wall with its passage cut south of centre. See [`doorway_piece`].
pub const DOORWAY_PIECE_B: &str = "bore_wall_open_b";

/// The prefab a blank side wall comes from.
pub const WALL_PIECE: &str = "bore_wall";

/// The prefab a bulkhead closing the end of a bore comes from.
pub const HEAD_PIECE: &str = "bore_end";

/// The prefab half a cross-passage comes from.
///
/// **Half**, because a passage between two cells is written by both of them: each writes the 3.6 m
/// from its own wall to the boundary. That is the same rule the old shells used for a shared side,
/// turned inside out — a shared *wall* is written once, a shared *passage* is written from both ends.
pub const PASSAGE_PIECE: &str = "cross_passage_open";

/// The prefab an enamel section plate comes from — the plate, its rule and its surround.
///
/// The **letter** is a separate piece, because a prefab names its mesh and a sign that named one
/// could only ever say one thing. See [`SECTION_LETTERS`].
pub const SIGN_PIECE: &str = "section_sign";

/// How far along the bore a cross-passage is cut from the middle of its wall, in metres.
///
/// # `docs/11` §5.3's one prohibition, and why it was being broken by arithmetic
///
/// *"Never let a straight run be visible end to end."* Every bore runs north–south and a passage goes
/// east–west between two of them, and `bore_side_open` was built with a **mirror about its own
/// middle** — so every aperture in the level sat at the same offset along its bore, three cells
/// joined east–west lined their openings up exactly, and a player saw through all of them.
///
/// Engine gate review 20 called the result *"the worst frame in the game"*: eight identical, evenly
/// spaced door frames receding to a vanishing point. Review 22 confirmed the diagnosis and pointed
/// out that the fix is the same four boxes with the `mirror` flag dropped — which is what
/// `bore_side_open_a` and `_b` are.
///
/// **A door-graph fix was tried first and reverted.** Closing one door of a straight run is cheaper,
/// but fourteen doors over fourteen rooms is a spanning tree plus one edge, so nearly every closure
/// disconnects the level and the connectivity check put every door straight back — the generated
/// scene came out byte-identical. A `collapse` dropped in the aperture was the other suggestion and
/// does not work either: its collider is 3.4 m wide and 1.2 m tall, sized for a 4.8 m bore, so in a
/// 2 m passage it does not break a sightline, it walls the level off.
///
/// **1.4 m either way is a 2.8 m stagger over a 12 m pitch** — about 13° off the axis, enough that a
/// player standing in one aperture cannot see through the next but one.
pub const PASSAGE_STAGGER: f32 = 1.4;

/// Which way this boundary's passage is offset, **keyed off the boundary rather than the cell**.
///
/// A passage is written by both of the cells it joins, so the two have to reach the same answer or
/// half a tube meets a wall. Keying off the western cell of the pair gives that for free: the east
/// wall of `(i, j)` and the west wall of `(i + 1, j)` both hash `(i, j)`.
#[must_use]
pub fn passage_stagger(cell: (i32, i32), side: Side) -> f32 {
    let key = match side {
        Side::West => (cell.0 - 1, cell.1),
        // North and south are bulkheads rather than passages, so the value is never used there.
        _ => cell,
    };
    if (key.0 + key.1).rem_euclid(2) == 0 {
        PASSAGE_STAGGER
    } else {
        -PASSAGE_STAGGER
    }
}

/// The wall piece whose aperture lands on `stagger` once the wall is turned to face the bore.
///
/// **The west wall is the same piece turned about**, so its local `+z` points along world `−z`. A
/// mesh cut at local +1.4 lands at −1.4 when it is used as a west wall, and the two halves of one
/// boundary would miss each other by 2.8 m. The turn is undone here.
#[must_use]
pub fn doorway_piece(stagger: f32, side: Side) -> &'static str {
    let north_of_centre = match side {
        Side::West => stagger < 0.0,
        _ => stagger > 0.0,
    };
    if north_of_centre {
        DOORWAY_PIECE_A
    } else {
        DOORWAY_PIECE_B
    }
}

/// The section letters, in the order the shelter's own alphabet runs.
///
/// # Six, and rectilinear, and that is a real constraint rather than a shrug
///
/// A letter here is a `CompoundMesh` of boxes (see [`SIGN_PIECE`] for why it is geometry rather than
/// a picture), so the alphabet available is the one a stencil can cut without a diagonal or a curve.
/// `docs/11` §5.4 asks for a *naval* name per section, and the constraint costs nothing: **EXMOUTH,
/// FROBISHER, HOWE, INGLEFIELD, LEAKE, TORRINGTON** are all real First Sea Lords and admirals, and
/// all five initials are straight lines **and horizontally symmetric**, which the second half of
/// this matters as much as the first: a flag-mounted sign is read from both sides, so a letter that
/// is not its own mirror image reads backwards from one of them. `E`, `F` and `L` were built first
/// and discarded for exactly that.
///
/// # Chosen by position, which is what makes it wayfinding rather than decoration
///
/// `docs/11` §5.4 is emphatic that a player who sees a letter learns nothing unless the letters are
/// **ordered along the route** — otherwise the whole scheme is set dressing. So a section's letter is
/// its distance from the start, modulo six: cross a door and the letter advances, always in the same
/// direction, so *"I have gone from F to H"* means *"I am further in"* without a map, a minimap or a
/// compass.
///
/// It also gives the adjacency property for free rather than by search, and **on any seed**: see
/// [`section_index`] for why the first attempt at that argument was wrong and what fixed it.
pub const SECTION_LETTERS: [&str; 5] = [
    "section_letter_h",
    "section_letter_i",
    "section_letter_m",
    "section_letter_o",
    "section_letter_t",
];

/// The prefab a bunk that was slept in comes from.
pub const BUNK_MADE_PIECE: &str = "bunk_made";

/// The prefab a section cleared out for storage comes from.
pub const STORES_PIECE: &str = "stores";

/// The prefab standing water over a section's deck comes from.
pub const FLOODED_PIECE: &str = "flooded";

/// The prefab a fallen ring and its spoil come from.
pub const COLLAPSED_PIECE: &str = "collapsed";

/// The prefab a section re-racked as an archive comes from.
pub const ARCHIVE_PIECE: &str = "archive";

/// The prefab a bunk that was stripped comes from.
pub const BUNK_STRIPPED_PIECE: &str = "bunk_stripped";

/// The piece id of a bunk whose bedding was rolled and left on it.
///
/// **A third dressing, and it is a third thing that happened here rather than a third look.** R2
/// measured twelve bunk placements over two variants, and review 17 put it plainly: *the bunks were
/// perfectly made, identically, everywhere.* Made means somebody was living here when it stopped;
/// stripped means somebody cleared it out afterwards; **rolled means somebody left properly, in their
/// own time, and expected the place to be used again.** A section with one of each is a section where
/// people made different decisions.
pub const BUNK_ROLLED_PIECE: &str = "bunk_rolled";

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

/// The prefab the warden's tally board comes from — a slate, a chalk ledge, and the light you keep
/// the count by.
///
/// # It is the thing the figure is seen against, and that is why it exists
///
/// Designer direction 3's D6 puts it here as fiction — *"you cannot chalk a number in the dark"*,
/// so the board is beside a light and the warden stands there because that is where the board is.
/// Engine gate review 35 arrived at the same object from the other end and made it the row's first
/// ordered change: the warden's open side is seen against a plated bulkhead **4.5 m** behind it at a
/// mean luma of **11**, and *"a uniformly lit wall behind a figure is a studio; a lit board with a
/// shape in front of it is a scene."*
///
/// So it goes on the bulkhead the camera looks at past the figure, and it carries its own reading
/// light rather than borrowing one — which is what stops it being a lit patch with no cause, and
/// what lets it be placed anywhere a section keeps a count.
pub const TALLY_BOARD_PIECE: &str = "tally_board_piece";

/// The prefab a night light comes from — the fitting nobody ever switched off.
///
/// # There were two lighting systems down here, and this game only had one
///
/// Clapham South was lit by cast-iron bulkhead fittings bolted into the tunnelling rings, and
/// **every fourth one stayed lit through the night** so people could find the lavatories. That is a
/// second circuit with a real reason, and `docs/11` §4 already established it — *"the panels are on
/// the standby ring and have always been live; the lights are what the isolator brings up."*
///
/// Designer direction 3 states the split in one line, and it is the sentence to keep when the
/// numbers change: **the emergency circuit lights the floor so you can work; the night circuit
/// lights the wall so you can find your way.** [`LAMP_PIECE`] is pitched −52° and pools on the deck,
/// which is right and must not be re-aimed — `docs/11` §4's silhouette appears at that pool's edge.
/// This one grazes **along the lining** instead.
///
/// # It is not a workaround for one dark section, and it fixes a real fault
///
/// Engine gate review 34 failed the warden's silhouette because *"there is no fixture in the frame
/// at all"*, and the reason turned out to be that `light_the_sections` had darkened the warden's
/// whole section. But that rule governs the **emergency** circuit only. A night light is on the
/// standby ring, so it is unaffected by flood or collapse — which means no section is ever without a
/// light source, an isolator's promise is legible because you can see the dark fittings it would
/// bring up, and the ring lining's normal map finally gets the one thing that makes cast iron read:
/// light along it rather than at it. A downlight at −52° shows a normal map nothing.
///
/// **Every second cell**, which is the historical every-fourth-fitting rule at this generator's two
/// fittings per cell, so a visible run holds the same number of fixtures it always did.
pub const NIGHT_LIGHT_PIECE: &str = "night_light";

/// The prefab a fitting that is **not** on the circuit comes from — the same housing and tube, dark.
///
/// # Why a dead fitting is a piece rather than an absence
///
/// Until session 24 an unlit section had no fitting *at all*: [`PlacedRoom::lit`] gated whether the
/// entity was written, so eight of fourteen sections contained no fixture and no light. That is
/// `docs/14` §4 #4 from the inside — *the light with no fixture, and the fixture with no light* —
/// and it reads as a tunnel nobody ever wired rather than as one whose lighting has failed.
///
/// A dead fitting says the opposite and costs one prefab: the bracket, the shade and the tube are
/// all still there, and the tube is a dull grey rather than emitting. A player who has walked past
/// three working ones knows exactly what they are looking at.
pub const DEAD_LAMP_PIECE: &str = "room_lamp_dead";

/// The prefab the warden comes from.
pub const WARDEN_PIECE: &str = "warden_post";

/// The prefab the two HUD lines come from.
pub const HUD_PIECE: &str = "hud";

/// The prefab the room tone comes from.
pub const AMBIENCE_PIECE: &str = "ambience";

/// Every piece a generated level instances — which is also, once sorted, its `assets` block.
///
/// Listed by constant rather than by id on purpose, and **sorted at the point of use** rather than
/// here: the two orders are not the same, and hand-maintaining a sorted list of ids whose names are
/// spelled differently from their constants is exactly the sort of thing that goes quietly wrong.
/// `amadeo fmt --check` on the output is what would have caught it, and did.
pub const PIECES: [&str; 30] = [
    AMBIENCE_PIECE,
    BUNK_MADE_PIECE,
    BUNK_ROLLED_PIECE,
    BUNK_STRIPPED_PIECE,
    DOORWAY_PIECE_A,
    DOORWAY_PIECE_B,
    HEAD_PIECE,
    HUD_PIECE,
    KEY_PIECE,
    LAMP_PIECE,
    DEAD_LAMP_PIECE,
    NIGHT_LIGHT_PIECE,
    TALLY_BOARD_PIECE,
    PASSAGE_PIECE,
    PLAYER_PIECE,
    ROOM_PIECE,
    SIGN_PIECE,
    SECTION_LETTERS[0],
    SECTION_LETTERS[1],
    SECTION_LETTERS[2],
    SECTION_LETTERS[3],
    SECTION_LETTERS[4],
    STORES_PIECE,
    FLOODED_PIECE,
    COLLAPSED_PIECE,
    ARCHIVE_PIECE,
    TORCH_PIECE,
    WALL_PIECE,
    EXIT_PIECE,
    WARDEN_PIECE,
];

/// How far to the side of the player the thing they woke up next to stands, in metres.
///
/// Nearly at the lining: a crate in a shelter stands against a wall, and a prop at the edge of the
/// frame is what makes the two halves of a picture different without putting an obstacle in the
/// middle of the only route out.
pub const WOKE_ASIDE: f32 = 2.0;

/// How far ahead of the player the thing they woke up next to stands, in metres.
///
/// Only just ahead, because [`WOKE_ASIDE`] does the work: the prop sits at the edge of the frame
/// rather than in the middle of it. Keeping it out of the torch's 26° cone matters as much as where
/// it looks — a beam is inverse-square, and the first version put a crate 3.4 m dead ahead, which
/// saturated 4% of the frame and read as a rendering fault rather than as a lit crate.
pub const WOKE_AHEAD: f32 = 1.2;

/// How far off the bore's centreline the player wakes up, in metres.
///
/// **A composition number, and the smallest change that answers review 19's symmetry finding.** The
/// bore is 4.8 m wide, so this is over a third of the way towards one wall: far enough that the two
/// halves of the frame are different pictures, near enough that you are still plainly in a corridor
/// rather than pressed against its side. It is also where somebody asleep on a deck would actually
/// be — against a wall, not down the middle of the traffic route.
pub const PLAYER_OFF_AXIS: f32 = 0.8;

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

/// How far from a bore's centre a prop stands, **along** the bore, in world units.
///
/// Along rather than across, because the bore is only 4.8 m wide and 12 m long: a prop pushed
/// sideways ends up in the lining, and one pushed along the length is out of the way of the passage
/// opening, which is centred.
pub const PROP_OFFSET: f32 = 3.4;

/// How far off the centreline a prop stands, in world units.
///
/// Enough to be against a wall rather than in the middle of the floor — nobody leaves a crate where
/// people walk — and not so far that a 0.9 m crate meets the 2.4 m lining.
const PROP_SIDE: f32 = 1.5;

/// How far along the bore, from its centre, a fitting sits.
///
/// Clear of the cross-passage opening, which is 2 m wide and centred on the cell.
const FITTING_OFFSET: f32 = 3.0;

/// How far along the bore, from its centre, a section plate sits.
///
/// The other side of the opening from the fitting, so a junction reads as *lit sign* rather than as
/// a lamp and a plate stacked on each other.
const SIGN_OFFSET: f32 = 3.2;

/// How far along the bore the key board hangs, from the middle of its section.
///
/// **Its own number because every other thing on that wall already had one, and they collided.**
/// The section plate is at `SIGN_OFFSET` 3.2, the fittings at `FITTING_OFFSET` +/-3.0, and a
/// cross-passage opening is centred and about 1.8 m wide -- so the first attempt at `PROP_OFFSET`
/// 3.4 put the key board 0.2 m from the sign and the two interpenetrated in the one snapshot
/// committed for photographing the key. 1.5 clears the passage, the fittings and the plate.
pub const KEY_ALONG: f32 = 1.5;

/// How far the key board stands off the lining plane, in world units.
///
/// The lining plates project into the bore, so a board mounted flush is a board with ribs across its
/// edges -- and its edging is the `accent` mark that says it can be acted on (`docs/11` §5a).
pub const BOARD_PROUD: f32 = 0.12;

/// How far a bunk's centreline stands off the bore's, in world units.
///
/// A bunk is 0.72 m across and the lining is at 2.4 m, so this puts its outer upright a hand's width
/// off the wall and leaves 3.3 m of clear deck to walk down — which is what a shelter berth looks
/// like and what keeps the middle of the tube walkable.
const BUNK_SIDE: f32 = 1.95;

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
            "entity {} \"Bore\" from {ROOM_PIECE}\n",
            cell_id("bore", room.cell)
        ));
        out.push_str(&place(x, 0.0, z, 0.0));

        // **Every bore runs north-south**, so the level is parallel tubes joined by cross-passages
        // rather than a grid of rooms — `docs/11` §5.2's architecture, out of the graph that was
        // already there. It is one rule rather than a per-cell choice for a reason: with a mixed
        // grain, a door between a north-south bore and an east-west one meets a 4.8 m opening with a
        // 2 m passage and leaves the level open around it, which needs a piece nothing else wants.
        for side in [Side::East, Side::West] {
            let open = room.doors.contains(&side);
            // **Which way this boundary's passage is staggered**, and both cells must agree. The piece
            // is chosen so that after the west wall's half turn the two apertures land on one world z.
            let stagger = passage_stagger(room.cell, side);
            let (piece, label) = if open {
                (doorway_piece(stagger, side), "Wall with a passage")
            } else {
                (WALL_PIECE, "Wall")
            };
            // The east wall faces the bore across its own +X; the west wall is the same piece
            // turned about, which is what puts its ribs and its skirting on the inside.
            let (dx, turn) = match side {
                Side::East => (x + BORE_HALF_WIDTH, 0.0),
                _ => (x - BORE_HALF_WIDTH, 180.0),
            };
            out.push_str(&format!(
                "entity {} \"{label}\" from {piece}\n",
                cell_id(&format!("wall_{}", side_name(side)), room.cell)
            ));
            out.push_str(&place(dx, 0.0, z, turn));

            // **Half a cross-passage, from this bore's wall to the cell boundary.** The room on the
            // other side writes the other half from its own wall, so a door between two cells is
            // 7.2 m of low square passage and neither cell has to know how wide the other one is.
            if open {
                out.push_str(&format!(
                    "entity {} \"Cross-passage\" from {PASSAGE_PIECE}\n",
                    cell_id(&format!("passage_{}", side_name(side)), room.cell)
                ));
                // The tube follows its own aperture. The wall cannot be shifted -- it is 12 m
                // long and moving it would open a gap at one end -- so the *mesh* carries the
                // offset, and this, which is a free-standing tube, is simply placed at it.
                out.push_str(&place(dx, 0.0, z + stagger, turn));
            }
        }

        // A bore's two ends. A door means the next bore carries straight on, so there is nothing to
        // write; anything else is capped, including the level's outer boundary — which is what
        // stops a Warren a hundred feet down being open to the sky.
        for side in [Side::North, Side::South] {
            if room.doors.contains(&side) {
                continue;
            }
            let (dz, turn) = match side {
                Side::North => (z - CELL / 2.0 + HEAD_INSET, 180.0),
                _ => (z + CELL / 2.0 - HEAD_INSET, 0.0),
            };
            out.push_str(&format!(
                "entity {} \"Bulkhead\" from {HEAD_PIECE}\n",
                cell_id(&format!("head_{}", side_name(side)), room.cell)
            ));
            out.push_str(&place(x, 0.0, dz, turn));
        }

        // The fitting goes in with its bore rather than in a pass of its own, so a section's
        // geometry and its light sit next to each other in the file a person has to read.
        //
        // **On a haunch, and on alternating sides.** Overhead at the crown it would be 4 m up and
        // wash rather than pool; and a light down the middle of a long tube is the most symmetrical
        // composition available, which is exactly the machine-made read `docs/11` §6 is trying to
        // avoid. `FITTING_OFFSET` keeps it clear of the passage opening, which is centred.
        //
        // **Every section has one; only some of them work.** Engine gate review 19 measured six
        // fittings across fourteen sections and found eight sections containing no light source at
        // all — so the fixture is always written and [`PlacedRoom::lit`] chooses which prefab, dead
        // or alive. See [`DEAD_LAMP_PIECE`].
        // **Two of them, on opposite haunches**, and both halves of that are load-bearing.
        //
        // *Two*, because a section is twelve metres long and one fitting in it is a light every
        // twelve metres — which is why engine gate review 19 could walk a whole bore and find no
        // near-to-far gradient on the lining at all. At six metres the nearest one dominates, so the
        // wall beside you is brighter than the wall at the end, which is what a corridor looks like.
        //
        // *Opposite*, because two lights on the same side of a symmetrical tube is the most
        // symmetrical composition available, and the same review measured the authored frame as
        // mirror-symmetric to within five levels out of 255. Alternating them means the left and the
        // right of any frame are lit differently, for free, on every seed.
        let east = (room.cell.0 + room.cell.1).rem_euclid(2) == 0;
        let fitting = if room.lit {
            LAMP_PIECE
        } else {
            DEAD_LAMP_PIECE
        };
        for (index, along) in [-FITTING_OFFSET, FITTING_OFFSET].into_iter().enumerate() {
            // The first one takes the section's own side and the second takes the other, so the
            // alternation runs *along* the bore as well as across the level.
            let this_side_east = if index == 0 { east } else { !east };
            let (dx, turn) = if this_side_east {
                (x + BORE_HALF_WIDTH, 0.0)
            } else {
                (x - BORE_HALF_WIDTH, 180.0)
            };
            out.push_str(&format!(
                "entity {} \"Fitting\" from {fitting}\n",
                cell_id(&format!("fitting{index}"), room.cell)
            ));
            out.push_str(&place(dx, 0.0, z + along, turn));
        }

        // A plate at every junction, which is `docs/11` §5.4's rule. A section with no passage off
        // it is not a junction and does not get one — a sign in every bay is signage nobody reads.
        if room
            .doors
            .iter()
            .any(|side| matches!(side, Side::East | Side::West))
        {
            out.push_str(&format!(
                "entity {} \"Section plate\" from {SIGN_PIECE}\n",
                cell_id("sign", room.cell)
            ));
            out.push_str(&place(x + BORE_HALF_WIDTH, 0.0, z + SIGN_OFFSET, 0.0));

            // **And the letter, which is a second piece and the whole point of the first.** Engine
            // gate review 16 found fourteen signs in the shipped level and one letter mesh between
            // them: every plate said `H`. A player who reaches the second sign and reads the same
            // letter learns in one glance that the world is machine-assembled -- which is worse than
            // no sign at all, because `docs/11` §5.4 makes this the entire wayfinding system.
            let letter = SECTION_LETTERS[section_index(layout, room.cell)];
            out.push_str(&format!(
                "entity {} \"Section letter\" from {letter}\n",
                cell_id("letter", room.cell)
            ));
            out.push_str(&place(x + BORE_HALF_WIDTH, 0.0, z + SIGN_OFFSET, 0.0));
        }

        // **A night light every second cell, on the standby ring.** Designer direction 3's D3: the
        // real rule was every fourth *fitting* and this generator writes two per cell, so this is
        // the same density. It goes on the haunch the berths are not on, at the cell's own end, so
        // its patch of raked iron is a landmark along the bore rather than a wash over everything.
        if (room.cell.0 + room.cell.1).rem_euclid(2) == 0 {
            let (nx, nturn) = if east {
                (x + BORE_HALF_WIDTH, 0.0)
            } else {
                (x - BORE_HALF_WIDTH, 180.0)
            };
            out.push_str(&format!(
                "entity {} \"Night light\" from {NIGHT_LIGHT_PIECE}\n",
                cell_id("night", room.cell)
            ));
            out.push_str(&place(nx, 0.0, z + 1.6, nturn));
        }

        write_condition(&mut out, room, x, z);
    }

    write_contents(&mut out, layout);

    // Every entity above is written with a blank line after it, which leaves one too many at the
    // end. Trimmed here rather than by making the last writer special, so adding a tenth thing to
    // `write_contents` cannot reintroduce it. `amadeo fmt --check` on the output is what noticed.
    let trimmed = out.trim_end().to_string();
    format!("{trimmed}\n")
}

/// Writes what a section was left full of — `docs/11` §5.2's *"no two may be in the same
/// condition"*.
///
/// # Dressing only, and that is the point
///
/// Every branch here places existing pieces at different places. Nothing generates geometry, nothing
/// touches the room graph, and nothing needs a second material. That is what makes the rule
/// affordable: the expensive-sounding half of §5.2 turns out to be a `match` in the writer.
///
/// The bunks stand against one wall or the other by the section's own parity, offset along the bore
/// rather than across it, because the bore is 4.8 m wide and 12 m long. **This used to be phrased as
/// "the side the fitting is not on", and that stopped being true in session 24** when every section
/// gained a fitting on *each* haunch: the parity now decides which wall the berths take, and both
/// walls have a light over them either way.
fn write_condition(out: &mut String, room: &PlacedRoom, x: f32, z: f32) {
    // The same parity the fittings use, read the other way, so the berths and the section's first
    // fitting are on opposite walls.
    let east = (room.cell.0 + room.cell.1).rem_euclid(2) == 0;
    let side = if east { -BUNK_SIDE } else { BUNK_SIDE };

    // **Three of the six place one thing and stop**, because what makes a section read as a
    // *different* section is not a variant of the same prop. Engine gate review 17 counted the whole
    // vocabulary a player met across eight frames -- bunks, a ladder, one fitting, one sign -- and
    // pointed out that the three original conditions differed only by whether a mattress mesh was
    // present, while the three `docs/11` §5.2 names that would change what a room *looks* like were
    // exactly the ones missing.
    let single = match room.condition {
        Condition::Stores => Some((STORES_PIECE, "Stores", side, -2.2)),
        Condition::Flooded => Some((FLOODED_PIECE, "Standing water", 0.0, 0.0)),
        Condition::Collapsed => Some((COLLAPSED_PIECE, "A fall", side * 0.45, 1.4)),
        _ => None,
    };
    if let Some((piece, label, across, along)) = single {
        out.push_str(&format!(
            "entity {} \"{label}\" from {piece}\n",
            cell_id("condition", room.cell)
        ));
        out.push_str(&place(x + across, 0.0, z + along, 0.0));
        return;
    }

    let piece = match room.condition {
        Condition::Stripped => BUNK_STRIPPED_PIECE,
        Condition::Archive => ARCHIVE_PIECE,
        _ => BUNK_MADE_PIECE,
    };
    // Which berth in a slept-in section was rolled rather than left made. By cell rather than by
    // die, so it is a property of the level and not of a random draw -- and alternating on the cell
    // means the rolled one is not always the same berth, which is what would read as a rule.
    let rolled_first = (room.cell.0 + room.cell.1).rem_euclid(2) == 0;

    // **The two berths are no longer identical**, which review 17 measured as "both bunks in shot,
    // same stripe phase, same bolster at the same end, no sag, nothing over an edge". The cheapest
    // real fix is a quarter turn on one of them: the ticking is directional, so a rotated berth
    // shows its stripes running the other way and its bolster at the other end, from one number.
    for (index, along) in [-2.4_f32, 0.6].into_iter().enumerate() {
        let turn = if index == 0 { 0.0 } else { 180.0 };
        // **One berth in a slept-in section was rolled, and it is not always the same berth.** The
        // turn above stops the two looking identical; this stops them having had the same history.
        // A section left by two people who made different decisions is content; twelve bunks in one
        // state is a rule the player can read.
        let dressed = if piece == BUNK_MADE_PIECE && (index == 0) == rolled_first {
            BUNK_ROLLED_PIECE
        } else {
            piece
        };
        out.push_str(&format!(
            "entity {} \"Berth\" from {dressed}\n",
            cell_id(&format!("berth{index}"), room.cell)
        ));
        out.push_str(&place(x + side, 0.0, z + along, turn));
    }
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
    // **Along the bore, not at whichever door happens to sort first.** Every bore runs north–south
    // (`docs/11` §5.2), so an east or west door is a cross-passage through a side wall 2.4 m away:
    // facing one means waking up with your nose against a wall, which is what happened the first
    // time this placed a prop as well. Falling back to any door keeps a one-door end section working.
    let start_room = layout.at(marks.start);
    let ahead = start_room
        .and_then(|room| {
            room.doors
                .iter()
                .copied()
                .find(|side| matches!(side, Side::North | Side::South))
        })
        // **And along the bore even when the only door is a cross-passage.** Facing an east or west
        // door means waking up aimed through a passage and down the whole length of the level —
        // which `docs/11` §5.3 prohibits by name (*"never let a straight run be visible end to
        // end"*), and which fills the opening frame with sixty metres of fogged distance instead of
        // the room you are actually in. You wake up seeing your own bay: the deck, a fitting, the
        // bulkhead at the end. The way out is something you find by turning your head, which is the
        // first thing this game asks you to do anyway.
        .unwrap_or(Side::North);
    let look = facing(ahead);
    // **And stands off the centreline**, which is a composition decision rather than a gameplay one.
    // Engine gate review 19 measured the authored frame as mirror-symmetric to within **4.87 levels
    // out of 255** on row 600 — `games/atrium` scores 96 on the same test — because a camera on the
    // axis of a symmetrical bore, looking along it, produces a picture whose two halves are the same
    // picture. Bilateral symmetry that exact is a stronger machine-made tell than a box mesh is, and
    // no amount of dressing fixes it while the camera sits on the axis.
    // **Across the view, not across the bore**, and the difference is not pedantry: the start
    // section's only door is often a cross-passage, so the player faces *along* `x` and an offset in
    // `x` slides them towards the camera rather than away from the axis, changing nothing about the
    // picture. `right` is the look direction turned a quarter turn, so this is off-axis whichever
    // way they woke up facing.
    let (fx, fz) = {
        let (cx, cz) = ahead.step((0, 0));
        (cx as f32, cz as f32)
    };
    let (rx, rz) = (-fz, fx);
    out.push_str(&format!("entity you \"You\" from {PLAYER_PIECE}\n"));
    out.push_str(&place(
        sx + rx * PLAYER_OFF_AXIS,
        PLAYER_STAND,
        sz + rz * PLAYER_OFF_AXIS,
        look,
    ));

    // **Something in front of you when you wake up, on the other side of the bore.**
    //
    // Engine gate review 19 found three of nine frames containing no prop of any kind, and one of
    // those three was the authored camera — the frame item 24 is judged on, and the frame a
    // screenshot would be taken from. It also does the other half of the symmetry work: the player
    // is off-axis one way and this is off-axis the other, so the two halves of the picture hold
    // different things rather than the same thing twice.
    //
    // **Ahead is derived from where the player is looking**, not assumed. A cell is 12 m along the
    // bore and 4.8 m across it, so how far "ahead" can be depends on which way that is: down the
    // tube there is room for a prop at 3.4 m, across it the wall is at 2.4 m and the same number
    // would put a crate inside the lining. That is `PROP_SIDE`/`PROP_OFFSET`'s lesson one prop along.
    //
    // **Ahead is along the bore and across is `x`, and mixing the two put the camera inside a
    // crate.** The first version resolved "ahead" from the door and then subtracted the player's
    // lateral offset from the same coordinate, so on a start room whose first door faces east the
    // prop landed 10 cm in front of the player's face — a flat wall filling the whole frame, which
    // measured as a perfectly good 4% clipped and looked like a rendering fault. Every bore runs
    // north–south, so **across the bore is always `x`** and along it is always `z`.
    // Slightly ahead and well over to the other side, so it sits at the edge of the frame rather
    // than in the middle of it — foreground interest on one side is what breaks the symmetry, and a
    // prop dead ahead would only be a second thing on the axis. Off to the side also keeps it out of
    // the torch's 26° cone, which saturates anything inside about three metres.
    out.push_str(&format!(
        "entity woke \"What you woke up next to\" from {STORES_PIECE}\n"
    ));
    // **On the wall the berths are NOT on, and that is a placement bug rather than a preference.**
    // `WOKE_ASIDE` is 2.0 and `BUNK_SIDE` is 1.95, so a crate stack put on the berths' side lands
    // inside their footprint — and it did. Engine gate reviews 20 and 30 both found a bunk's corner
    // post terminating on a crate lid, not reaching the deck and casting nothing. Two pieces dropped
    // into one footprint by a generator that does not check is the most machine-made thing a frame
    // can contain.
    //
    // The old form offset along `right`, which is derived from the way the player happens to be
    // *facing* — so which wall the crate landed on had nothing to do with which wall the berths took.
    // `write_condition` decides that from the cell's own parity, so this asks the same question and
    // takes the other side. Across the bore is always `x` (every bore runs north–south), which is why
    // this is a bare offset rather than a rotated one.
    let berths_east = (marks.start.0 + marks.start.1).rem_euclid(2) == 0;
    let aside = if berths_east { WOKE_ASIDE } else { -WOKE_ASIDE };
    out.push_str(&place(
        sx + fx * WOKE_AHEAD + aside,
        0.0,
        sz + fz * WOKE_AHEAD,
        facing(ahead),
    ));

    // **Across the bore is `PROP_SIDE` and along it is `PROP_OFFSET`, and they are different
    // numbers now.** One offset used on both axes put a crate 3.2 m off the centreline of a tube
    // that is 2.4 m to its wall — inside the lining, invisible, and reported by nothing.
    let (tx, tz) = centre(marks.torch);
    out.push_str(&format!("entity torch \"Torch\" from {TORCH_PIECE}\n"));
    out.push_str(&place(tx - PROP_SIDE, 0.0, tz - PROP_OFFSET, 0.0));

    let (kx, kz) = centre(marks.key);
    out.push_str(&format!("entity key \"Key\" from {KEY_PIECE}\n"));
    // **Proud of the lining, not flush with it.** At `BORE_HALF_WIDTH` exactly, the board sits in the
    // plane of the plates -- and the plates stand proud of that plane, so a rib ate the whole right
    // vertical of its orange edging and the top of it, which is what F5 clause (c) measures. A notice
    // board is screwed *onto* a wall, so `BOARD_PROUD` brings it into the bore in front of the ribs.
    out.push_str(&place(
        kx + BORE_HALF_WIDTH - BOARD_PROUD,
        0.0,
        kz + KEY_ALONG,
        0.0,
    ));

    // **The way out is set into a bulkhead**, not into a side wall, which is why `exit_side` now
    // answers north or south. A side wall is 2.3 m to the springing and the door is 2.34 m in its
    // frame, so a door in one would stand through the haunch; a head is 3.6 m of flat plate and is
    // what a shelter's exit actually is.
    let side = exit_side(layout);
    let (ex, ez) = centre(marks.exit);
    let inset = CELL / 2.0 - HEAD_INSET - DOOR_INSET;
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
    // **On the wall the berths are NOT on**, which is `woke`'s correction one landmark along and the
    // third instance of the same defect in this file. `write_condition` picks the berths' wall from
    // the cell's own parity and this asked for `+PROP_SIDE` unconditionally, so on half of all seeds
    // the warden stood 0.45 m across and 1.0 m along from a bunk -- and on the shipped level it did:
    // warden at world (37.5, -3.4) against a berth at (37.95, -2.4).
    //
    // It is a picture problem before it is a placement one. F2 measures the figure on twenty sampled
    // rows and calls a row usable only if its pixels form one unbroken run; the bunk broke seven of
    // them, session 26 searched five camera framings without reaching sixteen, and engine gate review
    // 30 ruled it *"a finding about where the level puts the warden, not about the lens."* It is also
    // simply wrong in the fiction: nobody stands their post in front of a bunk.
    let berths_east = (marks.warden.0 + marks.warden.1).rem_euclid(2) == 0;
    let across = if berths_east { PROP_SIDE } else { -PROP_SIDE };
    out.push_str(&place(wx + across, WARDEN_STAND, wz - WARDEN_ALONG, 0.0));

    // **The post is lit, and the light is BEHIND the figure rather than on it.**
    //
    // `docs/11` §3 wants a shape you resolve out of the dark, and that needs something behind it to
    // be dark *against*. Engine gate review 34 measured what the game actually had: the figure at a
    // median of 16.9 against a background of 21.2 on one side and 10.0 on the other -- **five levels
    // of separation, with the sign inverted on the right** -- and found *no fixture in the frame at
    // all*. Session 27 had already moved the post in front of the section's near fitting to get this
    // and it changed nothing, for a reason neither party checked until now: **both fittings in this
    // section are `room_lamp_dead`.** [`light_the_sections`] darkens a section that failed
    // structurally, the warden's happened to be one, and a backlight that does not exist cannot be
    // stood in front of.
    //
    // So the station carries its own working fitting, on the **opposite haunch** and set back behind
    // the post, aimed across the bore so its pool falls on the lining the camera sees *past* the
    // figure. That is the designer's S1, which review 34 upheld, and it buys the separation without
    // adding a single level to the coat -- which is what clause (e) and §3 both want.
    //
    // **It does not break [`light_the_sections`]'s rule**, which is that a *section*'s emergency
    // circuit fails where the section failed. This is not that circuit: a warden's post is a place
    // somebody chose to stand, and the one thing such a place has is a working light.
    let post_side = if berths_east {
        BORE_HALF_WIDTH
    } else {
        -BORE_HALF_WIDTH
    };
    let post_turn = if berths_east { 0.0 } else { 180.0 };
    out.push_str(&format!(
        "entity {} \"The light over the post\" from {LAMP_PIECE}\n",
        cell_id("post_lamp", marks.warden)
    ));
    out.push_str(&place(
        wx + post_side,
        0.0,
        wz - WARDEN_ALONG - 1.3,
        post_turn,
    ));

    // **And a night light on the warden's own wall, behind it.** Designer direction 3's D6, and it
    // is a story beat rather than a placement: **you cannot chalk a number in the dark.** `docs/11`
    // §3a has the boards still being kept up to date and a check sounding like chalk on a board, so
    // the board is beside a night light because the count has to be readable at night — and the
    // warden stands there because that is where the board is.
    //
    // The two lights do different halves of the same job and neither replaces the other: the
    // working lamp above pools on the **deck**, which is where the figure's lower third gets its
    // separation, and this one grazes the **lining**, which is the only thing behind its chest and
    // head. Engine gate review 34 failed the silhouette at five levels because the second half did
    // not exist.
    // **The board goes on the bulkhead the camera looks at past the figure.** A north bulkhead sits
    // at `CELL / 2 - HEAD_INSET` from its cell's centre and faces back into the bore, so the board
    // is authored projecting along its own `-z` and takes the same turn. Offset toward the warden's
    // side so it lands behind the figure's open edge rather than beside it.
    out.push_str(&format!(
        "entity {} \"The tally board\" from {TALLY_BOARD_PIECE}\n",
        cell_id("tally", marks.warden)
    ));
    out.push_str(&place(
        wx + across * 1.15,
        0.0,
        wz - CELL / 2.0 + HEAD_INSET + 0.4,
        180.0,
    ));

    let night_side = post_side;
    let night_turn = post_turn;
    out.push_str(&format!(
        "entity {} \"The night light over the post\" from {NIGHT_LIGHT_PIECE}\n",
        cell_id("post_night", marks.warden)
    ));
    out.push_str(&place(
        wx + night_side,
        0.0,
        wz - WARDEN_ALONG - 1.0,
        night_turn,
    ));

    // **Neither of these two is placed anywhere**, so none takes an override — and an override naming
    // a component its prefab does not carry is refused at load, which is what would happen if the
    // HUD were handed a `Transform` (ADR 0029). A blank line after each keeps the file's shape
    // uniform.
    //
    // The room tone is here rather than per room on purpose: it is not *from* anywhere. A
    // non-spatial source plays on its bus directly, so where its entity sits never matters, and one
    // per room would be fourteen copies of one drone beating against itself.
    out.push_str(&format!(
        "entity ambience \"The Warren itself\" from {AMBIENCE_PIECE}\n\n"
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
    // **North and south only.** Every bore runs north-south, so those are the two sides that carry a
    // bulkhead; east and west are 2.3 m side walls with cross-passages through them, and a 2.34 m
    // door set into one would stand through the haunch and out the other side of the lining.
    const ENDS: [Side; 2] = [Side::North, Side::South];
    ENDS.into_iter()
        .find(|side| layout.at(side.step(cell)).is_none())
        .or_else(|| ENDS.into_iter().find(|side| !room.doors.contains(side)))
        .unwrap_or(Side::North)
}

/// Which of [`SECTION_LETTERS`] a cell carries: its **grid** distance from the start, modulo five.
///
/// Which section letter a cell carries — `docs/11` §5.4.
///
/// # A section is a lettered STRETCH, not a cell, and that is what makes the letters mean anything
///
/// The previous form was `manhattan(cell, start) % 5`, which is a **distance ring**: it rises
/// whichever way you walk away from the start and repeats every five cells, so the letters cannot
/// say *further in* and two plates at opposite ends of the level can carry the same one. §5.4 names
/// three requirements and that met one. Engine gate reviews 19, 25 and 30 all filed it.
///
/// A real deep shelter letters **stretches** — Clapham South's sixteen sub-shelters were A to P, each
/// one a run of bunks hundreds long, not a room. So the cells are ranked by how far into the level
/// they are and cut into as many contiguous stretches as there are letters, ascending.
///
/// Two cells in the same stretch share a letter, and that is correct rather than a clash: they are
/// the same section. What must not happen — and cannot, by construction — is the *same letter in two
/// places*, because a letter is one contiguous band of the ranking.
///
/// # Ranked by grid distance, and the ranking is what carries the old lesson
///
/// An earlier attempt used [`Layout::distances_from`] — how many *doors* away a cell is — and broke
/// on seed 2, because two cells sharing a **wall** rather than a door can sit at the same
/// door-distance. Grid distance has no such case: a grid is bipartite under grid adjacency, so any
/// two cells sharing a side differ by exactly one however the doors fall. That property is what
/// makes the ranking monotonic along the spine no matter what the generator did with the doors.
///
/// Ties are broken by **cell order**, which `Layout::rooms` already guarantees is sorted — so the
/// answer is a property of the layout rather than of iteration order, and two runs agree (I3).
#[must_use]
pub fn section_index(layout: &Layout, cell: (i32, i32)) -> usize {
    let start = layout.landmarks.start;
    let reach = |at: (i32, i32)| (at.0 - start.0).unsigned_abs() + (at.1 - start.1).unsigned_abs();

    // Every cell in the level, ordered by how far into it they are. `rooms` is already sorted by
    // cell, and `sort_by_key` is stable, so equal distances keep that order.
    let mut ranked: Vec<(i32, i32)> = layout.rooms.iter().map(|room| room.cell).collect();
    ranked.sort_by_key(|at| (reach(*at), *at));

    let total = ranked.len().max(1);
    let rank = ranked.iter().position(|at| *at == cell).unwrap_or(0);
    // Cut the ranking into as many contiguous stretches as there are letters. The `min` guards the
    // last cell, which would otherwise land one past the end.
    (rank * SECTION_LETTERS.len() / total).min(SECTION_LETTERS.len() - 1)
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

/// Which of the two endings a piece of the ended screen belongs to.
///
/// # Two registers, not two strings — design direction 1, decision 3
///
/// Both outcomes used one panel, one scale and one placement, and only the words changed. `docs/11`
/// §8 already names the game's two typographic registers — **the interface is signage** and **the
/// pause menu is a form** — and the endings are where that pays: escaping a place should not read
/// like being caught in it.
///
/// **Escaped, the sign speaks.** The line at `Title` scale, upper-left, over a *light* scrim, so the
/// frame you escaped through is clearly readable behind it. You can still see.
///
/// **Caught, the record speaks.** The line at `Body` scale, low and left, where a reference number
/// goes on a form, over a *deep* scrim. It should read as an entry being written rather than as a
/// result being announced — which is the same movement `ACCOUNTED FOR` makes in the words.
///
/// The medium of this game is light, so this makes the outcome legible *as* light: you got out and
/// you can still see, or you were caught and it is nearly gone. **A player knows which ending they
/// got before they read a word.**
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, StableHash, Reflect)]
pub struct Verdict {
    /// `true` for the parts shown when the warden reaches you.
    #[reflect(default = false)]
    pub caught: bool,
}

impl Component for Verdict {}

/// Marks the line that says how the run ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, StableHash, Reflect)]
pub struct EndingLine;

impl Component for EndingLine {}

/// Marks a piece of the reticle — `docs/11` §8.
///
/// # Why the game has one at all
///
/// Interaction is a sphere swept along the camera's forward (`amadeo-interaction`), and until this
/// existed **nothing on screen said where that pointed**. A player who could not pick something up
/// had no way to tell whether they were too far away or aimed five degrees off, which the design
/// calls *"a usability failure at the core verb, not a polish item"* — and it is right: the failure
/// and the success look identical, so the player cannot learn the verb by using it.
///
/// The specification is *"the smallest mark that reads — a single dim pixel cluster, opening
/// slightly when something is in reach"*, so it is five nodes: a 3 × 3 dot that is always there, and
/// four ticks that appear fourteen pixels out when [`Looking::at`] is `Some`. The ticks take the
/// theme's `Accent`, which is safety orange — §5a reserves that colour for **things you can act on**,
/// and something in reach is exactly that. The reticle therefore obeys the same rule as the world.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, StableHash, Reflect)]
pub struct Reticle {
    /// Whether this piece appears only when something is in reach.
    ///
    /// `false` is the dot, which is always shown while playing. `true` is a tick, which is the
    /// "opening" half.
    #[reflect(default = false)]
    pub opens: bool,
}

impl Component for Reticle {}

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
        // **`ACCOUNTED FOR`, not `IT FOUND YOU`** -- design direction 1, decision 2 (`docs/15` §5).
        // The old line is a narrator stating a fact the player just watched, and its pronoun frames the
        // warden as a monster that hunts. `docs/11` §3 is emphatic that it is not: it is an institution
        // still performing its function, and **the function is counting**. This is that institution's
        // own word for what has happened to you.
        //
        // `YOU GOT OUT` stays, and the asymmetry is the point rather than an oversight: when you
        // escape you get the last word, and when you are caught the shelter does.
        Outcome::Caught => "ACCOUNTED FOR".to_string(),
    };
    // Nothing to point at unless the game is actually being played. A stale "the door is locked"
    // under a lose screen reads as the game still running, and one behind the title plate reads as
    // the title screen being a level.
    //
    // **Both conditions, and they are not the same tick.** `settle_the_run` ends the run in this
    // stage and `apply_screen` moves the screen in the *next* tick's `PreSimulation`, so testing the
    // screen alone leaves the prompt up for one frame underneath the ending. Testing the outcome
    // alone would leave it up behind the pause menu and the title plate. One line each.
    let playing = screen(world) == Screen::Playing && ending.is_empty();
    let prompt = if playing {
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

    // **Which ending's furniture is on screen** (design direction 1, decision 3). Both outcomes used
    // one panel and one scale, so only the words told you which had happened; each has its own scrim
    // and its own register now, and this is what shows one and hides the other.
    let caught = outcome(world) == Outcome::Caught;
    let verdicts: Vec<(Entity, bool)> = world
        .query::<(&Verdict,)>()
        .map(|(entity, (verdict,))| (entity, verdict.caught == caught))
        .collect();
    for (entity, shown) in verdicts {
        // Compared before writing, for `Text`'s reason below: a write every tick would move the state
        // hash for a change nobody can see.
        if let Some(node) = world.get_mut::<UiNode>(entity)
            && node.visible != shown
        {
            node.visible = shown;
        }
    }

    for (entity, wanted) in lines {
        // Compared before writing, so a HUD that says the same thing as last tick does not move the
        // state hash — which it would every tick otherwise, for no change anyone can see.
        if let Some(text) = world.get_mut::<Text>(entity)
            && text.content != wanted
        {
            text.content = wanted;
        }
    }

    // **The reticle** — `docs/11` §8, and see [`Reticle`] for why the game has one.
    //
    // Here rather than in its own system because it is the same function of the same two facts the
    // prompt is: a mark that says "you are aimed at something" and a line that says what that
    // something would do are two spellings of one answer, and splitting them is how they drift into
    // disagreeing — a prompt with no reticle under it, or the reverse.
    //
    // It reads the prompt *string* rather than asking `Looking` again, which makes that claim
    // literal rather than a hope: there is exactly one place the answer comes from, so the two
    // cannot disagree even in principle.
    let in_reach = !prompt.is_empty();
    let pieces: Vec<(Entity, bool)> = world
        .query::<(&Reticle,)>()
        .map(|(entity, (reticle,))| (entity, playing && (!reticle.opens || in_reach)))
        .collect();
    for (entity, wanted) in pieces {
        // Compared before writing, for the prompt's reason one paragraph up: `UiNode::visible` is a
        // hashed field, and writing the same value every tick moves the state hash for nothing.
        if let Some(node) = world.get_mut::<UiNode>(entity)
            && node.visible != wanted
        {
            node.visible = wanted;
        }
    }
}

// --- The shell: a title screen, a pause, a save, and a way to try again -------------------------

/// What the Warren is doing, as far as the player is concerned.
///
/// # This is the authority; the engine's `Paused` is projected from it
///
/// ADR 0065 §5: what screens exist is genre knowledge (I4), so the engine has no concept of one. It
/// knows only whether the gameplay stages are running, and [`apply_screen`] writes that from this
/// every tick. Nothing else writes `Paused`, so the two cannot drift into a menu over a running
/// game.
///
/// `games/atrium` has the same enum with three variants. This one has five, and the two extra are
/// the whole of what M3's exit gate item 1 asks for beyond a pause menu: somewhere to start from and
/// somewhere to arrive when the run ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, StableHash, Reflect)]
pub enum Screen {
    /// The title plate, before a run has begun. **The default**, so a fresh world starts here.
    #[default]
    Title,
    /// Walking around the Warren.
    Playing,
    /// The pause menu is up and the world is frozen.
    Paused,
    /// The run is over, one way or the other, and the ending is on screen.
    ///
    /// Entered from [`Outcome`] rather than chosen: `settle_the_run` decides *that* a run ended and
    /// this decides what the player sees when it did, which is the same split `Screen` and `Paused`
    /// draw one level down.
    Ended,
    /// The player chose to quit; the window closes on the next frame.
    ///
    /// Terminal — nothing gets out of it. A game shutting down and then not shutting down because
    /// somebody was still holding a key would be a memorable bug.
    Quitting,
}

impl Resource for Screen {}

/// Which screen a menu belongs to.
///
/// **One marker with a field, rather than one marker type per menu.** The Atrium has a single
/// `PauseMenu` component because it has a single menu; three of them would be three components,
/// three queries and three chances for a menu to be left visible over another. This way
/// [`apply_screen`] shows exactly the menus whose screen is current, and adding a fourth menu is a
/// scene edit with no Rust at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, StableHash, Reflect)]
pub struct Menu {
    /// The screen this menu is the interface for.
    pub screen: Screen,
}

impl Component for Menu {}

/// What choosing one of the menu's buttons means.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, StableHash, Reflect)]
pub enum MenuChoice {
    /// Start a run from the title screen.
    #[default]
    Begin,
    /// Close the pause menu and carry on.
    Resume,
    /// Write the world to [`SAVE_FILE`].
    Save,
    /// Put the world back from [`SAVE_FILE`].
    Load,
    /// Throw this run away and start another.
    TryAgain,
    /// Close the game.
    Quit,
}

/// Attached to a menu button, saying what choosing it means.
///
/// ADR 0063's split cashed, exactly as `games/atrium` cashes it: `UiActivated` names an entity and
/// deliberately nothing else, because the engine does not know what a button *means*. This is the
/// game supplying that half, in the scene file beside the button rather than as a table of entity
/// ids in Rust that would go stale the moment somebody reordered the menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, StableHash, Reflect)]
pub struct MenuButton {
    /// What this button does.
    pub choice: MenuChoice,
}

impl Component for MenuButton {}

/// Where this game's one save lives, relative to the working directory.
///
/// One slot, and a path rather than a save system — M3's exit gate asks for "save, quit, resume from
/// save", which is one slot. **Where a save file should actually live is Q38**, so this is
/// deliberately a plain relative path that will not have to be unpicked.
pub const SAVE_FILE: &str = "warren.save";

/// Renames that let a save written by an older build still load (ADR 0069).
///
/// Optional, and absent is the normal case. A missing file means no redirects rather than an error.
pub const REDIRECT_FILE: &str = "warren.redirects";

/// Something the platform layer carries out between ticks, because a simulation cannot touch a disk.
///
/// # Why a resource and not a function call
///
/// Reading and writing files is **not gameplay**, and a system that did it would put the state of a
/// filesystem inside a deterministic tick — a replay would then depend on what was on disk when it
/// ran. So the menu records the *decision*, which is hashed and replays like any other, and the
/// caller acts on it between ticks. `Screen::Quitting` uses the same split: closing a window is not
/// gameplay either.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, StableHash, Reflect)]
pub struct Requested {
    /// Write the world out.
    pub save: bool,
    /// Read it back.
    pub load: bool,
    /// Throw this run away and start a fresh one.
    pub restart: bool,
}

impl Resource for Requested {}

/// The world exactly as it loaded, kept so that a run can be started over.
///
/// # A service, and that is what makes it work
///
/// It has to survive a restore, and ADR 0009 says a snapshot restores resources and **never**
/// services. A resource holding this would be replaced by the very restore it exists to perform, so
/// the second attempt at trying again would restore a snapshot of a world that was itself restored.
///
/// It is also not gameplay by any reading — the text of a snapshot has no business in a state hash.
#[derive(Debug)]
pub struct FreshStart(pub String);

impl amadeo_ecs::Service for FreshStart {}

/// The label [`apply_screen`] is registered under.
pub const APPLY_SCREEN: &str = "apply_screen";

/// The label [`choose_from_menu`] is registered under.
pub const CHOOSE_FROM_MENU: &str = "choose_from_menu";

/// The action that opens and closes the pause menu.
pub const PAUSE: &str = "pause";

/// Which screen the game is on.
#[must_use]
pub fn screen(world: &World) -> Screen {
    world.resource::<Screen>().copied().unwrap_or_default()
}

/// Moves the screen on, then projects everything that follows from it.
///
/// # Why this runs in `PreSimulation`
///
/// That stage runs **whether or not the game is paused** (ADR 0065), and a system that stopped
/// running while paused could never unpause. It is also after `sample_input`, which is what makes
/// `just_pressed` mean this tick's keypress.
///
/// # One writer for everything derived from the screen
///
/// The transitions are a handful of lines and the rest is projection: the engine's `Paused`, which
/// menus are visible, and where the highlight sits. Doing all of that here rather than at each place
/// the screen changes is what stops a menu being visible over a running game — there is one place
/// that decides, and it runs every tick rather than only on the tick something changed.
pub fn apply_screen(world: &mut World) {
    let toggled = world
        .resource::<InputState>()
        .is_some_and(|input| input.just_pressed(ActionId::new(PAUSE)));

    let current = screen(world);
    let next = match (current, toggled) {
        (Screen::Playing, true) => Screen::Paused,
        (Screen::Paused, true) => Screen::Playing,
        // **The run ending moves the screen, and nothing else does.** `settle_the_run` decides that
        // a run is over; this decides what is on screen because of it. Checked every tick rather
        // than on the tick it changed, so a restored save that was already over arrives here too.
        (Screen::Playing, false) if outcome(world) != Outcome::Playing => Screen::Ended,
        // Including `Title`, `Ended` and `Quitting`. Escape does not leave any of them: there is
        // nothing to pause on a title screen, and a run that has ended has no state to go back to.
        (screen, _) => screen,
    };
    if let Some(slot) = world.resource_mut::<Screen>() {
        *slot = next;
    }

    // Everything except `Playing` is a screen with a menu on it, so everything except `Playing`
    // freezes the world. That is one line because the enum was chosen to make it one.
    let paused = next != Screen::Playing;
    if let Some(state) = world.resource_mut::<Paused>() {
        state.paused = paused;
    }

    // Collected before writing, because the query borrows the world. Only the nodes whose
    // visibility is actually wrong are touched: writing an identical `UiNode` every tick would work
    // and would also mean the state hash could never tell a menu opening from one already open.
    let stale: Vec<(Entity, bool)> = world
        .query::<(&Menu, &UiNode)>()
        .filter(|(_, (menu, node))| node.visible != (menu.screen == next))
        .map(|(entity, (menu, _))| (entity, menu.screen == next))
        .collect();
    for (root, wanted) in stale {
        if let Some(node) = world.get_mut::<UiNode>(root) {
            node.visible = wanted;
        }
    }

    // **The highlight has to be inside the menu that is up.** `focusable_in_order` already ignores
    // anything in a hidden subtree (session 18), so a focus left on a button of the menu that just
    // closed is not merely wrong, it is unreachable — the player would press a direction and watch
    // nothing happen. Re-seating it whenever it is not on something reachable covers the screen
    // changing, a button being disabled, and the world being restored, all in one comparison.
    //
    // `navigate_focus` deliberately will not do this for us (ADR 0063): a menu that focused an item
    // the moment it appeared would override whatever the game wanted focused. The engine knows how a
    // menu moves; the game knows when one is up.
    let reachable = amadeo_ui::focusable_in_order(world);
    let settled = world
        .resource::<Focus>()
        .and_then(|focus| focus.entity)
        .filter(|entity| reachable.contains(entity));
    let wanted = if paused {
        settled.or_else(|| reachable.first().copied())
    } else {
        None
    };
    if let Some(focus) = world.resource_mut::<Focus>()
        && focus.entity != wanted
    {
        focus.entity = wanted;
    }
}

/// Acts on a menu button being chosen.
///
/// # Why it runs while paused
///
/// It is the one gameplay system that must (ADR 0065): everything it responds to happens while the
/// world is frozen, and on the title screen the world has never run at all.
///
/// # It records, and lets one place project
///
/// "Resume" does not hide a menu or unpause the engine — it moves [`Screen`], and [`apply_screen`]
/// does the rest next tick. Save, load and restart do not touch a disk; they move [`Requested`], and
/// [`serve_requests`] does that between ticks. Two writers for one piece of derived state is exactly
/// how the halves get out of step.
pub fn choose_from_menu(world: &mut World) {
    // `UiActivated` was sent last tick and swapped in at the end of it, so there is no ordering
    // constraint against `navigate_focus` -- declaring one would suggest a same-tick handoff the
    // event buffers do not provide.
    let chosen: Vec<MenuChoice> = world
        .read_events::<UiActivated>()
        .iter()
        .filter_map(|record| world.get::<MenuButton>(record.event.entity))
        .map(|button| button.choice)
        .collect();

    for choice in chosen {
        match choice {
            MenuChoice::Begin | MenuChoice::Resume => {
                if let Some(screen) = world.resource_mut::<Screen>() {
                    *screen = Screen::Playing;
                }
            }
            MenuChoice::Save => {
                if let Some(request) = world.resource_mut::<Requested>() {
                    request.save = true;
                }
            }
            MenuChoice::Load => {
                if let Some(request) = world.resource_mut::<Requested>() {
                    request.load = true;
                }
            }
            MenuChoice::TryAgain => {
                if let Some(request) = world.resource_mut::<Requested>() {
                    request.restart = true;
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

/// Carries out whatever the menu asked for, between ticks.
///
/// # Why this is not a system
///
/// It reads and writes **files**, and a system doing that would put the state of a filesystem inside
/// a deterministic tick: a replay would then depend on what happened to be on disk when it ran,
/// which is invariant I3 gone for anything that ever saved.
///
/// **`Physics::reset` after any world replacement is the load-bearing line.** Not for the reason it
/// looks like — rapier rebuilds its contacts from the components either way (measured in
/// `amadeo-physics/tests/reset_clears_the_solver.rs`) — but because replacing a world must drop the
/// static geometry belonging to the one being left, and a generated interior is a great deal of
/// static geometry.
///
/// Returns what it did, so a caller can say so. Failures are reported and survivable: a game that
/// refused to start because a save file was missing would be worse than one that carries on.
///
/// # Errors
///
/// Never. A failed save or load is reported through the returned strings rather than propagated,
/// because neither is a reason to stop the game.
pub fn serve_requests(app: &mut App) -> Vec<String> {
    let mut said = Vec::new();
    let request = app
        .world
        .resource::<Requested>()
        .copied()
        .unwrap_or_default();
    if !request.save && !request.load && !request.restart {
        return said;
    }
    // Cleared first, so a request that fails is not retried every frame for the rest of the game.
    if let Some(slot) = app.world.resource_mut::<Requested>() {
        *slot = Requested::default();
    }

    if request.save {
        let text = amadeo_snapshot::to_text(&app.capture_snapshot());
        said.push(match std::fs::write(SAVE_FILE, &text) {
            Ok(()) => format!("saved to {SAVE_FILE} ({} bytes)", text.len()),
            Err(error) => format!("could not write {SAVE_FILE}: {error}"),
        });
    }

    if request.load {
        match std::fs::read_to_string(SAVE_FILE) {
            Ok(text) => match restore_from(app, &text) {
                Ok(notes) => {
                    said.push(format!("loaded {SAVE_FILE}"));
                    said.extend(notes);
                }
                Err(why) => said.push(why),
            },
            Err(error) => said.push(format!("could not read {SAVE_FILE}: {error}")),
        }
    }

    if request.restart {
        // Cloned out first: the restore needs the world mutably and the text lives in a service on
        // it. A snapshot of this level is a few hundred kilobytes and a restart is a keypress, so
        // the copy is not worth a dance around the borrow checker.
        let fresh = app
            .world
            .service::<FreshStart>()
            .map(|start| start.0.clone());
        match fresh {
            Some(text) => match restore_from(app, &text) {
                Ok(_) => {
                    // **Straight into the run, not back to the title.** The snapshot was taken
                    // before the first tick, so the screen it holds is `Title` — restoring it and
                    // stopping would send somebody who asked to try again back to the menu they
                    // just left.
                    if let Some(screen) = app.world.resource_mut::<Screen>() {
                        *screen = Screen::Playing;
                    }
                    said.push("started again".to_string());
                }
                Err(why) => said.push(why),
            },
            None => said.push("nothing recorded the world it started as".to_string()),
        }
    }

    said
}

/// Puts a snapshot back, leniently, and drops whatever the solver was caching.
///
/// Lenient because a save may have been written by an older build (ADR 0069) — and the restart path
/// uses the same door, because a snapshot taken by *this* build matches the layout fingerprint and
/// therefore takes the strict path, hash check and all. **Leniency costs nothing when it is not
/// needed**, which is the whole of ADR 0069's argument.
fn restore_from(app: &mut App, text: &str) -> Result<Vec<String>, String> {
    let snapshot =
        amadeo_snapshot::parse(text).map_err(|error| format!("will not parse: {error}"))?;

    // Absent is the normal case, and an unreadable one is worth complaining about rather than
    // ignoring: a redirect file that silently does nothing is how a rename turns into data loss.
    let redirects = match std::fs::read_to_string(REDIRECT_FILE) {
        Ok(text) => amadeo_snapshot::Redirects::parse(&text)
            .map_err(|error| format!("{REDIRECT_FILE} will not parse: {error}"))?,
        Err(_) => amadeo_snapshot::Redirects::new(),
    };

    let report = app
        .restore_save(&snapshot, &redirects)
        .map_err(|error| format!("will not restore: {error}"))?;

    // The solver is holding the world that was just replaced — see [`serve_requests`].
    if let Some(physics) = app.world.service_mut::<Physics>() {
        physics.reset();
    }
    Ok(report.lines())
}

// --- Sound (ADR 0059) ---------------------------------------------------------------------------

/// How far the player walks between footsteps, in metres.
///
/// An eyeball number, and the one that decides whether the gait reads as walking or as jogging. Set
/// by arithmetic against the authored 2.6 m/s rather than by ear, which is the honest description of
/// it. Shorter than the Atrium's 1.9 because this character moves at half the speed.
pub const STRIDE: f32 = 0.95;

/// What the floor is made of where a thing stands — `docs/13` §1b's F6 clause (b).
///
/// # Why an authored volume rather than a downward cast
///
/// The obvious implementation sweeps a shape down and asks what it hit. It cannot work here and the
/// reason is worth writing down: **the duckboards have no collider.** They are a decorative run laid
/// over the deck, and the only collider in a bore section is the `slab` the deck sits on — so a cast
/// downward reports "screed" everywhere, including where a player is audibly walking on timber.
///
/// Giving every decorative surface a collider to answer a question about *sound* would put geometry
/// in the solver for the solver's own sake. A volume authored beside the mesh it describes says the
/// same thing, costs nothing at runtime, and is visible to `describe` and to a person reading the
/// piece.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, StableHash, Reflect)]
pub enum Surface {
    /// Bare concrete deck. The default, because most of the Warren is.
    #[default]
    Screed,
    /// A duckboard run — the timber walkway down a bore's centreline.
    Timber,
    /// Standing water.
    Water,
}

impl Surface {
    /// The clip id a step on this makes.
    #[must_use]
    pub fn footstep(self) -> &'static str {
        match self {
            Surface::Screed => "step_screed",
            Surface::Timber => "step_timber",
            Surface::Water => "step_water",
        }
    }
}

/// A box within which the floor is made of something in particular.
///
/// Authored on the entity carrying the surface it describes, so moving a duckboard run moves what it
/// sounds like with it. Extents are half-widths in the entity's own space, and the test is `y` as
/// well as `x`/`z`: a walkway on an upper deck must not decide what the deck below sounds like.
#[derive(Debug, Clone, Copy, PartialEq, StableHash, Reflect)]
pub struct Footing {
    /// What a step here sounds like.
    pub surface: Surface,
    /// Half-extents of the box, in world units.
    #[reflect(unit = "world units", default = [1.0, 0.5, 1.0])]
    pub half_extent: [f32; 3],
}

impl Default for Footing {
    fn default() -> Self {
        Self {
            surface: Surface::Screed,
            half_extent: [1.0, 0.5, 1.0],
        }
    }
}

impl Component for Footing {}

/// What the floor is made of under a given place.
///
/// **The smallest matching volume wins**, so a duckboard run laid inside a flooded section reads as
/// timber rather than as whichever the query happened to reach first — query order is reproducible
/// but it is not a ranking, and "the more specific surface" is what a person means.
#[must_use]
pub fn footing_at(world: &World, at: [f32; 3]) -> Surface {
    let mut best: Option<(f32, Surface)> = None;
    for (_, (footing, transform, global)) in
        world.query::<(&Footing, &Transform, Option<&GlobalTransform>)>()
    {
        let centre = match global {
            Some(global) => global.translation(),
            None => transform.translation,
        };
        let inside = (0..3)
            .all(|axis| (at[axis] - centre[axis]).abs() <= footing.half_extent[axis].max(0.0));
        if !inside {
            continue;
        }
        let volume = footing.half_extent[0] * footing.half_extent[1] * footing.half_extent[2];
        if best.is_none_or(|(smallest, _)| volume < smallest) {
            best = Some((volume, footing.surface));
        }
    }
    best.map_or(Surface::Screed, |(_, surface)| surface)
}

/// How far the player has walked since the last footstep.
///
/// # A hashed resource rather than a service, and that matters
///
/// Where you are in your gait decides *when the next step happens*, so a save that did not restore
/// it would resume mid-stride and take its next step at the wrong moment. `games/atrium` reached
/// the same conclusion; it is worth restating because "sound state" is exactly the sort of thing
/// that looks like it belongs outside the simulation, and this half of it does not.
#[derive(Debug, Clone, Copy, PartialEq, Default, StableHash, Reflect)]
pub struct Stride {
    /// Metres walked since the last footstep.
    #[reflect(unit = "m")]
    pub since_last: f32,
}

impl Resource for Stride {}

/// The label [`play_footsteps`] is registered under.
pub const PLAY_FOOTSTEPS: &str = "play_footsteps";

/// The label [`play_the_run`] is registered under.
pub const PLAY_THE_RUN: &str = "play_the_run";

/// Emits a [`SoundPlayed`] every [`STRIDE`] metres the character walks on the ground.
///
/// # Why this lives in the game rather than in `modules/amadeo-character`
///
/// **A footstep is content.** How often one happens, what it sounds like, and whether a character
/// makes one at all are questions about *this* game — invariant I4's rule one level up: the module
/// knows how to move, and the game knows what moving sounds like.
///
/// # Horizontal distance only, and only on the ground
///
/// Falling is not walking, so `grounded` gates it; and vertical speed must not count towards a
/// stride, or a character dropping down a step would tap one out in mid-air.
pub fn play_footsteps(world: &mut World) {
    let walked: Vec<(f32, [f32; 3])> = world
        .query::<(&CharacterMotion, &Transform)>()
        .filter(|(_, (motion, _))| motion.grounded)
        .map(|(_, (motion, transform))| {
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
            // `while` rather than `if`, so a tick covering more than one stride emits more than one
            // footstep. It cannot happen at this speed; it can the moment somebody adds a sprint,
            // and a silent cap is the kind of thing nobody thinks to look for.
            while stride.since_last >= STRIDE {
                stride.since_last -= STRIDE;
                steps.push(position);
            }
        }
    }

    for position in steps {
        // **Which clip depends on what is underfoot** (F6 clause b). One `footstep` for a game with
        // three floor surfaces meant a whole review cycle spent making the duckboards legible while
        // walking onto them sounded exactly like walking on concrete, and wading sounded like it too.
        world.send_event(SoundPlayed::at(
            footing_at(world, position).footstep(),
            position,
        ));
    }
}

/// Sounds the three moments of a run: picking something up, getting out, and being caught.
///
/// # Why one system rather than three lines in three places
///
/// Each of these could be a `send_event` inside the system that causes it, and `take_what_you_used`
/// nearly got one. Keeping them together means **"what does this game make a noise about" is one
/// list in one place** — the same argument `settle_the_run` makes for endings, and the reason a
/// silent game is diagnosable at all.
///
/// It reads the same events and resources those systems write, so it has to run after them.
///
/// # An ending sounds once, and that is what the comparison is for
///
/// [`Outcome`] does not change back, so a system that played a sting whenever the run was over would
/// play one every tick for the rest of the game. What is compared is the *previous* value, kept in
/// [`Sounded`] — hashed, like everything else a save has to put back.
pub fn play_the_run(world: &mut World) {
    // Picked up: the same `Interacted` events `take_what_you_used` reads, filtered to the ones that
    // turned out to be items. Placed where the thing was, so it pans from the plinth.
    let taken: Vec<[f32; 3]> = world
        .read_events::<Interacted>()
        .iter()
        .map(|record| record.event)
        .filter(|event| world.get::<Item>(event.target).is_some())
        .filter_map(|event| {
            world
                .get::<GlobalTransform>(event.target)
                .map(|global| global.to_mat4().translation())
        })
        .collect();
    for at in taken {
        world.send_event(SoundPlayed::at("taken", at));
    }

    let now = outcome(world);
    let before = world
        .resource::<Sounded>()
        .copied()
        .unwrap_or_default()
        .outcome;
    if now != before {
        if let Some(slot) = world.resource_mut::<Sounded>() {
            slot.outcome = now;
        }
        // **From where the player is**, not from nowhere. An ending is about you, and a sting that
        // arrives centred while everything else in the mix is positioned reads as a different game.
        let at = player_at(world).unwrap_or([0.0; 3]);
        match now {
            Outcome::Playing => {}
            Outcome::Escaped => {
                world.send_event(SoundPlayed::at("escaped", at));
            }
            Outcome::Caught => {
                world.send_event(SoundPlayed::at("caught", at));
            }
        }
    }
}

/// What has already been sounded, so a one-shot happens once.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, StableHash, Reflect)]
pub struct Sounded {
    /// The outcome the last sting was played for.
    pub outcome: Outcome,
}

impl Resource for Sounded {}
