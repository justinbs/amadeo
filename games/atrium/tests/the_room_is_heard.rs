//! What can be checked about the Atrium's sound without a sound card — which is not whether it
//! sounds right, and this file should not be read as claiming otherwise.
//!
//! # What is actually being tested here
//!
//! Everything up to the speaker. The scene authors two sources and a listener; the assets the game
//! ships decode; the collection pass turns all of that into an [`AudioFrame`] with the right voices
//! in it; walking around moves the spatial one relative to the ears and leaves the other alone.
//!
//! **The last step — a frame becoming a sound — has no test anywhere and cannot have one.** CI has
//! no device and neither does a headless run. That step is verified by a person listening, and
//! saying so is more useful than a test named as though it covered it.
//!
//! This is still worth having, because every failure it *can* catch is one that would otherwise
//! present as silence: a mis-declared asset, a source the scene stopped authoring, ears on the wrong
//! entity, a `.wav` that stopped decoding.

use amadeo_app::App;
use amadeo_audio::{Audio, AudioFrame, AudioListener, AudioSource, Bus, SoundCache};
use amadeo_character::{CharacterController, MOVE_FORWARD};
use amadeo_ecs::Entity;
use amadeo_input::{ActionId, InputDriver, InputState, ScriptedSource};

/// The room with a scripted input source, so a test can hold a direction.
fn room() -> App {
    let mut app = atrium::build_simulation().expect("the room builds");
    amadeo_input::install(
        &mut app.world,
        InputDriver::new(Box::new(ScriptedSource::new())),
    );
    app
}

/// The last frame the null backend was given.
fn heard(app: &App) -> AudioFrame {
    app.world
        .service::<Audio>()
        .expect("the game installs an audio service")
        .null_backend()
        .expect("headless runs use the null backend")
        .last_frame()
        .expect("a frame was submitted")
        .clone()
}

fn player(app: &App) -> Entity {
    app.world
        .query::<(&CharacterController,)>()
        .map(|(entity, _)| entity)
        .next()
        .expect("the scene authored exactly one character")
}

/// Runs `ticks` ticks with an axis held at `value`, rendering after each one.
///
/// **The render call is what drives audio**, and it is easy to leave out: `collect_audio` is in the
/// `Render` stage beside the renderer's own collection pass, and `App::run_ticks` runs only the
/// simulation stages. A test that stepped without rendering would see no frames at all — which is
/// how this file was written the first time, and the failure said "a frame was submitted" rather
/// than anything about audio.
fn hold(app: &mut App, action: &str, value: f32, ticks: u64) {
    for _ in 0..ticks {
        if let Some(state) = app.world.resource_mut::<InputState>() {
            state.set_axis(ActionId::new(action), value);
        }
        app.run_ticks(1).expect("a tick runs");
        app.render().expect("the render stage runs");
    }
}

#[test]
fn the_scene_authors_two_sources_and_one_pair_of_ears() {
    // A sound is a function of the world (ADR 0059), so this is the audio equivalent of
    // `the_room_loads_from_its_scene_file`: if the scene stopped authoring them, the game would go
    // silent with nothing else to see.
    let app = room();

    let sources: Vec<&AudioSource> = app
        .world
        .query::<(&AudioSource,)>()
        .map(|(_, (source,))| source)
        .collect();
    assert_eq!(sources.len(), 2, "the lamp's hum and the room tone");

    // One placed and one not — the two different paths through a real backend, and the reason the
    // demo has two sounds rather than one.
    assert_eq!(
        sources.iter().filter(|source| source.spatial).count(),
        1,
        "exactly one sound should be heard from somewhere"
    );
    assert!(
        sources
            .iter()
            .any(|source| source.bus == Bus::Music && !source.spatial),
        "the room tone belongs on the music bus and nowhere in particular"
    );
    assert!(
        sources.iter().all(|source| source.looping),
        "both are ambience; a one-shot has no home yet (ADR 0059)"
    );

    assert_eq!(
        app.world.query::<(&AudioListener,)>().count(),
        1,
        "one listener, and it is on the camera"
    );
}

#[test]
fn the_ears_are_on_the_camera_rather_than_the_character() {
    // **An audible choice, not a detail.** This game is third person, so the viewer should hear what
    // they can see. Moving the listener to the player would be a legitimate change and would sound
    // different, which is exactly why it is pinned rather than left to whoever edits the scene next.
    let app = room();
    let ears = app
        .world
        .query::<(&AudioListener,)>()
        .map(|(entity, _)| entity)
        .next()
        .expect("one listener");

    assert!(
        app.world.get::<amadeo_render::Camera>(ears).is_some(),
        "the listener should be the camera entity"
    );
    assert_ne!(ears, player(&app));
}

#[test]
fn both_of_the_games_own_wav_files_decode() {
    // Session 9's lesson, applied to audio: a game whose own asset fails the validator it ships with
    // is worse than one that has no validator. `SoundCache` reports a failure rather than raising
    // one, so without this the symptom of a broken `.wav` is silence.
    let mut app = room();
    hold(&mut app, MOVE_FORWARD, 0.0, 2);

    let cache = app
        .world
        .service::<SoundCache>()
        .expect("the game installs a sound cache");

    let problems: Vec<String> = cache
        .failures()
        .map(|(id, failure)| format!("{id}: {failure}"))
        .collect();
    assert!(problems.is_empty(), "{problems:#?}");

    assert!(cache.is_decoded("lamp_hum"), "the lamp's hum should decode");
    assert!(cache.is_decoded("room_tone"), "the room tone should decode");
}

#[test]
fn the_generated_hum_is_mono_and_loops_without_a_click() {
    // **Both properties are audible and neither is visible.** A stereo file placed in the world has
    // its own left and right, so a position has nothing left to decide; and a loop whose last sample
    // is far from its first steps the waveform once per loop, which is a tick you cannot un-hear.
    //
    // The `tone` generator guarantees the second by making every partial complete a whole number of
    // cycles. This checks the file it produced rather than the intention.
    let mut app = room();
    hold(&mut app, MOVE_FORWARD, 0.0, 2);

    let cache = app.world.service::<SoundCache>().expect("installed");
    let hum = cache.get("lamp_hum").expect("decoded");

    assert_eq!(hum.channels, 1, "a placed sound should be mono");

    // The seam is one sample wide, so the step across it should be about the size of an ordinary
    // step within the clip. Deliberately **not** "the first and last samples are both near zero":
    // that is only true of a clip fading in and out, and it would pass for a clip of silence.
    let first = hum.samples.first().copied().expect("not empty");
    let last = hum.samples.last().copied().expect("not empty");
    let seam = (first - last).abs();
    let ordinary = (hum.samples[1] - hum.samples[0]).abs();
    assert!(
        seam <= ordinary * 4.0 + 1e-3,
        "the loop seam steps by {seam}, where a normal step is {ordinary} — this will click"
    );
}

#[test]
fn walking_away_from_the_lamp_moves_it_relative_to_the_ears() {
    // The whole point of a spatial voice, and the one thing about it that a headless test can see:
    // the voice carries a world position and the listener carries another, and walking changes the
    // distance between them. What a backend *does* with that — attenuation, panning — is kira's,
    // and hearing it is Justin's.
    let mut app = room();
    hold(&mut app, MOVE_FORWARD, 0.0, 2);

    let before = heard(&app);
    let listener = before.listener.expect("the camera has ears");
    let lamp = before
        .voices
        .iter()
        .find(|voice| voice.sound == "lamp_hum")
        .expect("the lamp is humming");
    let lamp_position = lamp.position.expect("a placed sound carries a position");
    let started_at = distance(listener.position, lamp_position);

    // Walk backwards, away from the lamp in the room's north-west corner.
    hold(&mut app, MOVE_FORWARD, -1.0, 90);

    let after = heard(&app);
    let listener = after.listener.expect("still has ears");
    let lamp = after
        .voices
        .iter()
        .find(|voice| voice.sound == "lamp_hum")
        .expect("still humming");
    let ended_at = distance(
        listener.position,
        lamp.position.expect("still carries a position"),
    );

    // The margin comes from watching the numbers rather than from a guess: this walk measures
    // 11.56 -> 13.81, so a metre of movement is a quarter of the change and would not be produced
    // by float wobble. The south wall is what caps it — the character stops against it.
    assert!(
        ended_at > started_at + 1.0,
        "walking away should put the lamp further off: {started_at} -> {ended_at}"
    );

    // And the lamp itself has not moved. The distance changed because the *ears* did, which is what
    // a listener attached to a moving camera is for.
    assert_eq!(lamp.position, Some(lamp_position));
}

#[test]
fn the_room_tone_never_gains_a_position_however_far_you_walk() {
    // A soundtrack that pans as the player turns around is the single most obvious way for game
    // audio to sound broken, and the mistake that causes it — treating every voice as placeable —
    // would pass every other test in this file.
    let mut app = room();
    hold(&mut app, MOVE_FORWARD, -1.0, 90);

    let frame = heard(&app);
    let tone = frame
        .voices
        .iter()
        .find(|voice| voice.sound == "room_tone")
        .expect("the room tone is playing");
    assert_eq!(tone.position, None);
    assert_eq!(tone.bus, Bus::Music);
}

#[test]
fn the_sound_of_the_room_cannot_move_its_state_hash() {
    // ADR 0059's structural claim, checked against the *real game* rather than against a world
    // written to demonstrate it. `Audio` and `SoundCache` are services, so ADR 0009 excludes them by
    // trait bound — what this can still catch is the collection pass writing somewhere else by
    // accident, which is a mistake anyone could make while adding a feature to it.
    let mut app = room();
    hold(&mut app, MOVE_FORWARD, 0.0, 5);

    let before = app.world.state_hash();
    amadeo_audio::collect_audio(&mut app.world);
    amadeo_audio::collect_audio(&mut app.world);
    assert_eq!(before, app.world.state_hash());
}

fn distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    let dx = a[0] - b[0];
    let dy = a[1] - b[1];
    let dz = a[2] - b[2];
    (dx * dx + dy * dy + dz * dz).sqrt()
}
