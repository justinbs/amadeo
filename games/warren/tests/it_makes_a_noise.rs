//! The Warren has sound — M3 exit gate item 6, where a horror slice lives or dies.
//!
//! # What can and cannot be tested here
//!
//! ADR 0060 is blunt about this and it is worth repeating: **the part that ends in a speaker cannot
//! be verified by any test.** What can be verified is everything up to the backend — that the game
//! submits the voices it should, positioned where it says, and stops submitting them when it should.
//! `NullAudio` remembers frames instead of playing them, which is exactly the seam this needs.
//!
//! The listening procedure is `games/atrium`'s two `#[ignore]`d tests, and this game inherits it:
//! run `cargo run -p warren` and walk towards the warden. If it gets louder and pans, the spatial
//! path works.
//!
//! # Why `describe_audio` rather than the backend
//!
//! It reads the **world**, not the last frame a backend happened to keep. A real backend keeps
//! nothing, so a test that read one would work headlessly and answer nothing about the game somebody
//! is playing — which is ADR 0060's argument for `audio.describe` existing at all.

use amadeo_app::App;
use amadeo_audio::describe_audio;
use amadeo_events::WorldEvents;
use amadeo_input::{InputDriver, ScriptedSource};
use amadeo_transform::Transform;
use warren::{Outcome, player};

fn playing() -> App {
    let mut app = warren::build_simulation().expect("the game builds");
    amadeo_input::install(
        &mut app.world,
        InputDriver::new(Box::new(ScriptedSource::new())),
    );
    if let Some(screen) = app.world.resource_mut::<warren::Screen>() {
        *screen = warren::Screen::Playing;
    }
    app.run_ticks(2).expect("ticks run");
    app
}

/// What the world currently sounds like.
fn heard(app: &App) -> Vec<String> {
    describe_audio(&app.world)
        .frame
        .voices
        .iter()
        .map(|voice| voice.sound.clone())
        .collect()
}

#[test]
fn there_are_ears_and_they_are_on_the_camera() {
    // **The failure this catches has no symptom at all.** A world with no `AudioListener` submits no
    // voices whatsoever — not quiet ones, none — so every other test in this file would pass
    // vacuously against a game that made no sound. `audio.describe` reports it by name and in the
    // right order (no listener *before* no voices), which is how this was found while wiring it up.
    let app = playing();
    let report = describe_audio(&app.world);
    assert!(
        report.frame.listener.is_some(),
        "no ears: {:?}",
        report.silent_because()
    );

    // And they are where the eyes are. In first person those are the same thing, which is why this
    // game does not have to make the argument `games/atrium` does about third-person listening.
    let eyes = warren::eyes(&app.world).expect("an interactor on the camera");
    let at = app
        .world
        .get::<amadeo_transform::GlobalTransform>(eyes)
        .expect("composed")
        .to_mat4()
        .translation();
    let ears = report.frame.listener.expect("checked above").position;
    assert!(
        (ears[0] - at[0]).abs() < 0.01 && (ears[1] - at[1]).abs() < 0.01,
        "the ears are at {ears:?} and the eyes at {at:?}"
    );
}

#[test]
fn the_room_hums_and_the_warden_breathes() {
    // The two looping voices, and the two different paths through a backend: one non-spatial, which
    // plays on its bus directly, and one positioned, which gets its own track. No test can tell those
    // apart below this line — which is precisely why this asserts they are both *submitted*.
    let sounds = heard(&playing());
    assert!(
        sounds.iter().any(|id| id == "warren_tone"),
        "the room has no tone: {sounds:?}"
    );
    // **A tread rather than a breath while it has not seen you** — design direction 1, decision 10.
    // A thing that breathes continuously reads as an animal and `docs/11` §3 says it is an
    // institution; the breath is what you hear once it has noticed you, so the change of sound *is*
    // the moment of being noticed.
    assert!(
        sounds.iter().any(|id| id == "warden_tread"),
        "the warden makes no noise, which is most of what it is for: {sounds:?}"
    );
}

#[test]
fn the_wardens_voice_is_where_the_warden_is() {
    // **The whole reason the audio is here.** A horror slice lives on knowing where something is
    // without seeing it, and a spatial voice that did not track its entity would be a sound coming
    // from the wrong corridor — worse than no sound, because a player would trust it.
    let mut app = playing();
    let warden = app
        .world
        .query::<(&warren::Warden,)>()
        .map(|(entity, _)| entity)
        .next()
        .expect("a warden");

    let moved = [12.0, 0.93, -24.0];
    if let Some(transform) = app.world.get_mut::<Transform>(warden) {
        transform.translation = moved;
    }
    app.run_ticks(2).expect("ticks run");

    let voice = describe_audio(&app.world)
        .frame
        .voices
        .into_iter()
        .find(|voice| voice.sound == warren::WARDEN_TREAD)
        .expect("it is still making a noise");
    let at = voice.position.expect("and it is somewhere, not everywhere");
    assert!(
        (at[0] - moved[0]).abs() < 0.01 && (at[2] - moved[2]).abs() < 0.01,
        "the warden is at {moved:?} and its voice at {at:?}"
    );
}

#[test]
fn the_room_tone_is_not_from_anywhere() {
    // The control for the test above. A backend has two paths and this is the other one: a
    // non-spatial source has no position at all, rather than a position at the origin — which would
    // put the Warren's ambience in one particular corner of it.
    let app = playing();
    let voice = describe_audio(&app.world)
        .frame
        .voices
        .into_iter()
        .find(|voice| voice.sound == "warren_tone")
        .expect("the room hums");
    assert!(
        voice.position.is_none(),
        "the room tone came from {:?}",
        voice.position
    );
}

#[test]
fn walking_taps_out_footsteps_and_standing_still_does_not() {
    // A one-shot (ADR 0061), and the two halves that matter: that walking makes them, and that
    // standing still makes none. The second is the one worth pinning — `collect_audio` runs in
    // `Render` and the loop renders uncapped, so a naive read plays a footstep per *drawn frame*.
    let mut app = playing();
    app.run_ticks(30).expect("it settles on the floor");

    let quiet = app.world.read_events::<amadeo_audio::SoundPlayed>().len();
    app.run_ticks(60).expect("a second of standing still");
    assert_eq!(
        app.world.read_events::<amadeo_audio::SoundPlayed>().len(),
        quiet,
        "standing still should not make a sound"
    );

    // Now walk. Driven through the named action a keyboard would set, so what is exercised is the
    // same path a player takes.
    let mut steps = 0;
    for _ in 0..180 {
        if let Some(state) = app.world.resource_mut::<amadeo_input::InputState>() {
            state.set_axis(
                amadeo_input::ActionId::new(amadeo_character::MOVE_FORWARD),
                1.0,
            );
        }
        app.run_ticks(1).expect("a tick runs");
        steps += app
            .world
            .read_events::<amadeo_audio::SoundPlayed>()
            .iter()
            // **Any of the three**, since F6 clause (b) made the clip depend on what is underfoot: the
            // Warren has duckboards, screed and standing water and had one sound for all of them.
            .filter(|record| record.event.sound.starts_with("step_"))
            .count();
    }
    assert!(
        steps > 0,
        "three seconds of walking should have made a footstep"
    );
}

#[test]
fn an_ending_sounds_once_and_not_every_tick() {
    // `Outcome` does not change back, so a system that played a sting whenever the run was over
    // would play one every tick for the rest of the game — which is what the `Sounded` resource
    // exists to prevent, and is exactly the shape of ADR 0061's watermark bug one level up.
    let mut app = playing();
    app.run_ticks(30).expect("it settles");

    let warden = app
        .world
        .query::<(&warren::Warden,)>()
        .map(|(entity, _)| entity)
        .next()
        .expect("a warden");
    let at = app
        .world
        .get::<Transform>(warden)
        .expect("placed")
        .translation;
    if let Some(transform) = app
        .world
        .get_mut::<Transform>(player(&app.world).expect("p"))
    {
        transform.translation = [at[0], 1.0, at[2]];
    }

    let mut stings = 0;
    for _ in 0..90 {
        app.run_ticks(1).expect("a tick runs");
        stings += app
            .world
            .read_events::<amadeo_audio::SoundPlayed>()
            .iter()
            .filter(|record| record.event.sound == "caught")
            .count();
    }

    assert_eq!(warren::outcome(&app.world), Outcome::Caught);
    assert_eq!(stings, 1, "being caught should sound exactly once");
}

#[test]
fn every_sound_the_game_names_is_an_asset_it_has() {
    // **ADR 0060's rule, and the reason it is worth a test of its own: there is no placeholder
    // sound and there must not be one.** Magenta works for a missing texture because nobody ships
    // magenta; every possible placeholder *sound* is indistinguishable from content. So a sound the
    // game asks for and does not have is silence — and silence has no symptom whatsoever.
    //
    // Checked against the cache's own failures rather than against a list written here, which would
    // simply be the same typo twice.
    let mut app = playing();
    app.run_ticks(30).expect("it settles");

    let failures = describe_audio(&app.world).failures;
    assert!(
        failures.is_empty(),
        "the game asked for sounds it does not have: {failures:?}"
    );
}

// ---------------------------------------------------------------------------------------------
// The ear — engine gate row F6, and `docs/11` §9 makes it a gameplay requirement rather than
// polish: *"a warden exactly as loud through a wall as through a doorway makes the whole mechanic a
// lie."* The game is made of corridors and its antagonist is found by ear.
//
// **These need a real solver.** Against `NullPhysics` every cast reports clear and nothing is ever
// occluded, which is the control case asserted last.

/// The gain a named sound is submitted at, or `None` if it is not being made.
fn gain_of(app: &App, sound: &str) -> Option<f32> {
    describe_audio(&app.world)
        .frame
        .voices
        .into_iter()
        .find(|voice| voice.sound == sound)
        .map(|voice| voice.gain)
}

/// The gain of whichever loop the warden is currently making.
///
/// **Either clip**, because it treads until it sees you and breathes afterwards, and a test about
/// how loud it is must not also be a test of which state it is in.
fn warden_gain(app: &App) -> Option<f32> {
    gain_of(app, warren::WARDEN_TREAD).or_else(|| gain_of(app, warren::WARDEN_BREATH))
}

/// Holds an entity still for `ticks` ticks, re-pinning it every tick.
///
/// The warden moves under its own power the moment it sees you, so a measurement taken over ninety
/// ticks would be a measurement at some *other* distance. Pinning is what keeps "the same distance"
/// true, which is the whole comparison here.
fn hold(app: &mut App, entity: amadeo_ecs::Entity, at: [f32; 3], ticks: u64) {
    for _ in 0..ticks {
        if let Some(transform) = app.world.get_mut::<Transform>(entity) {
            transform.translation = at;
        }
        app.run_ticks(1).expect("a tick runs");
    }
}

/// Puts an entity somewhere and lets a few ticks run.
fn place(app: &mut App, entity: amadeo_ecs::Entity, at: [f32; 3]) {
    if let Some(transform) = app.world.get_mut::<Transform>(entity) {
        transform.translation = at;
    }
    app.run_ticks(4).expect("ticks run");
}

fn the_warden(app: &App) -> amadeo_ecs::Entity {
    app.world
        .query::<(&warren::Warden,)>()
        .map(|(entity, _)| entity)
        .next()
        .expect("a warden")
}

#[test]
fn a_wall_makes_the_warden_quieter_than_an_open_line_at_the_same_distance() {
    let mut app = playing();
    let warden = the_warden(&app);
    let you = warren::player(&app.world).expect("a player");
    place(&mut app, you, [0.0, 1.0, 0.0]);

    // Same distance both times. The only difference is the lining between.
    let range = warren::BORE_HALF_WIDTH + 1.6;

    // Held still, and long enough for the ease to settle: `occlusion` moves at a bounded rate per
    // tick on purpose, so a reading taken immediately is a reading of the transition.
    hold(&mut app, warden, [0.0, 0.93, -range], 90);
    let clear = warden_gain(&app).expect("it is making a noise");

    hold(&mut app, warden, [range, 0.93, 0.0], 90);
    let blocked = warden_gain(&app).expect("it is still making a noise");

    assert!(
        blocked <= clear * 0.31,
        "through a bore wall it should be at most 0.30x an open line at the same distance, \
         and it is {blocked:.3} against {clear:.3}"
    );
}

#[test]
fn occlusion_never_jumps_in_one_tick() {
    // F6 clause (a)'s second half, and the reason ADR 0086 eases rather than assigning: a single
    // cast answers blocked-or-clear, so writing its answer straight in steps the gain from full to
    // nothing in one tick every time the listener crosses a doorway edge. That is an audible click.
    let mut app = playing();
    let warden = the_warden(&app);
    let you = warren::player(&app.world).expect("a player");
    place(&mut app, you, [0.0, 1.0, 0.0]);
    hold(&mut app, warden, [0.0, 0.93, -3.0], 90);

    // Straight through the lining, which is the worst case: the cast's answer flips in one tick.
    let across = [warren::BORE_HALF_WIDTH + 1.6, 0.93, 0.0];
    let mut last = warden_gain(&app).expect("a noise");
    for tick in 0..90 {
        if let Some(transform) = app.world.get_mut::<Transform>(warden) {
            transform.translation = across;
        }
        app.run_ticks(1).expect("a tick runs");
        let now = warden_gain(&app).expect("a noise");
        assert!(
            (now - last).abs() <= 0.15,
            "gain moved {:.3} in one tick ({last:.3} -> {now:.3}) at tick {tick}",
            (now - last).abs()
        );
        last = now;
    }
}

#[test]
fn a_step_sounds_like_what_it_lands_on() {
    // F6 clause (b). The Warren has three floor surfaces and had one clip.
    use warren::Surface;
    assert_ne!(Surface::Timber.footstep(), Surface::Screed.footstep());
    assert_ne!(Surface::Water.footstep(), Surface::Screed.footstep());
    assert_ne!(Surface::Timber.footstep(), Surface::Water.footstep());

    let app = playing();
    // The duckboard run is down the centreline, so a step there is timber and one against the
    // lining is not. Both are the same bore section, which is what makes this a surface test rather
    // than a level test.
    assert_eq!(
        warren::footing_at(&app.world, [0.0, 0.05, 0.0]),
        Surface::Timber,
        "the centreline of a bore section is a duckboard run"
    );
    assert_eq!(
        warren::footing_at(&app.world, [warren::BORE_HALF_WIDTH - 0.2, 0.05, 0.0]),
        Surface::Screed,
        "against the lining there are no duckboards, so it is bare deck"
    );
}

#[test]
fn the_room_tone_leans_as_the_warden_closes() {
    // F6 clause (c). The bed is the only sound that is always there, so it is the one thing that can
    // tell you something without competing with anything.
    let mut app = playing();
    let warden = the_warden(&app);
    let you = warren::player(&app.world).expect("a player");
    place(&mut app, you, [0.0, 1.0, 0.0]);

    place(&mut app, warden, [0.0, 0.93, -(warren::WARDEN_SIGHT + 4.0)]);
    let far = gain_of(&app, warren::WARREN_TONE).expect("there is a room tone");

    place(&mut app, warden, [0.0, 0.93, -warren::WARDEN_REACH]);
    let near = gain_of(&app, warren::WARREN_TONE).expect("there is still a room tone");

    assert!(
        near >= far * 2.0,
        "the bed should span at least 2:1 between sight range and arm's length, \
         and it is {near:.3} against {far:.3}"
    );
}

#[test]
fn the_bed_survives_a_save() {
    // The lean is written into a hashed component field rather than into the mixer, so a save
    // restores the state of the chase rather than resetting it to calm.
    let mut app = playing();
    let warden = the_warden(&app);
    let you = warren::player(&app.world).expect("a player");
    place(&mut app, you, [0.0, 1.0, 0.0]);
    place(&mut app, warden, [0.0, 0.93, -warren::WARDEN_REACH]);

    let leaning = gain_of(&app, warren::WARREN_TONE).expect("a room tone");
    let snapshot = app.capture_snapshot();
    let text = amadeo_snapshot::to_text(&snapshot);
    assert!(
        text.contains(&format!("gain {leaning}")),
        "the lean is not in the snapshot, so a save would restore a calm room: {leaning}"
    );
}
