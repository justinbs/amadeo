//! The lantern sweeps and the lamp flickers, and neither is a special case anywhere — ADR 0066.
//!
//! # What this is really checking
//!
//! Not that a number changed. That the whole reflected path works end to end in a real game: a
//! `.anim` file parses, `load_scene` finds it and fills the cache, a track naming `"Transform"` and
//! `"rotation"` resolves against a component **`amadeo-anim` has never heard of**, and the sampled
//! numbers land in the right field with the right shape.
//!
//! Two clips rather than one, on purpose. One animates a three-number field on a `Transform` and the
//! other a single-number field on a `PointLight` — different components, different widths, different
//! crates. A design that only worked for transforms would pass half of this file.

use amadeo_anim::{Animatable, AnimationPlayer, ClipCache};
use amadeo_app::App;
use amadeo_ecs::Entity;
use amadeo_input::{InputDriver, ScriptedSource};
use amadeo_render::PointLight;
use amadeo_transform::Transform;

fn room() -> App {
    let mut app = atrium::build_simulation().expect("the room builds");
    amadeo_input::install(
        &mut app.world,
        InputDriver::new(Box::new(ScriptedSource::new())),
    );
    app
}

/// The one entity carrying a player for `clip`.
fn animated(app: &App, clip: &str) -> Entity {
    app.world
        .query::<(&AnimationPlayer,)>()
        .filter(|(_, (player,))| player.clip == clip)
        .map(|(entity, _)| entity)
        .next()
        .unwrap_or_else(|| panic!("the scene should author a player for `{clip}`"))
}

#[test]
fn both_clips_load_and_neither_reports_a_problem() {
    // **The first thing to check, because a missing clip is silent stillness.** It is also the one
    // asset in this engine whose absence changes the state hash rather than the picture, so the
    // report is the whole diagnosis (ADR 0066).
    let app = room();
    let cache = app.world.service::<ClipCache>().expect(
        "`load_scene` installs the clip cache itself when anything names a clip — if this is \
         missing, nothing in the scene named one",
    );

    assert!(cache.is_loaded("lantern_sweep"));
    assert!(cache.is_loaded("lamp_flicker"));
    assert!(
        cache.failures().is_empty(),
        "clips loaded with problems: {:?}",
        cache.failures()
    );
    let problems: Vec<(&str, &str)> = app.asset_problems().collect();
    assert!(
        problems.is_empty(),
        "an asset failed to build: {problems:?}"
    );
}

#[test]
fn the_lantern_actually_turns() {
    let mut app = room();
    let lantern = animated(&app, "lantern_sweep");

    let start = app
        .world
        .get::<Transform>(lantern)
        .expect("the lantern has a transform")
        .rotation;

    // A second and a half, which is inside the clip's first three-second ramp.
    app.run_ticks(90).expect("ticks run");
    let later = app
        .world
        .get::<Transform>(lantern)
        .expect("still there")
        .rotation;

    assert_ne!(start[0], later[0], "the pitch should have moved");
    // And only the axis the clip animates — the other two are part of the same three-number value,
    // so a coercion that filled the first element and left the rest would pass the line above.
    assert_eq!(start[1], later[1]);
    assert_eq!(start[2], later[2]);
}

#[test]
fn the_lamp_actually_flickers() {
    // The scalar half. A `PointLight` is in `amadeo-render` and a `Transform` is in
    // `amadeo-transform`, and `amadeo-anim` depends on neither — which is the claim.
    let mut app = room();
    let lamp = animated(&app, "lamp_flicker");

    let start = app
        .world
        .get::<PointLight>(lamp)
        .expect("the lamp has a light")
        .intensity;
    app.run_ticks(60).expect("ticks run");
    let later = app
        .world
        .get::<PointLight>(lamp)
        .expect("still there")
        .intensity;

    assert_ne!(start, later);
    // Within the range the clip authors, so this is animation rather than something else having
    // written the field.
    assert!((19.0..=24.0).contains(&later), "got {later}");
}

#[test]
fn nothing_a_clip_asked_for_went_missing() {
    // **The failure this game is most likely to hit.** The allow-list is explicit (ADR 0066 §4), so
    // a component nobody allowed animates nothing — which is indistinguishable from a clip with no
    // motion in it unless something says so. This is that something.
    let mut app = room();
    app.run_ticks(30).expect("ticks run");

    let missing = app
        .world
        .service::<Animatable>()
        .expect("the game installs the allow-list")
        .missing();
    assert!(missing.is_empty(), "clips asked for {missing:?}");
}

#[test]
fn a_paused_game_does_not_animate() {
    // ADR 0065 meeting ADR 0066. `animate` is a `Simulation` system with no `.while_paused()`, so a
    // paused room is a still one — including its lights, which is the difference between a pause and
    // a screenshot with a menu over it.
    let mut app = room();
    let lantern = animated(&app, "lantern_sweep");
    app.run_ticks(30).expect("ticks run");

    // **Through `Screen`, not through `Paused`.** Writing `Paused` directly here does nothing:
    // `apply_screen` projects it from the game's screen every tick and would put it straight back.
    // That is the single-writer arrangement working, and this test failed for exactly that reason
    // when it was written the other way round.
    if let Some(screen) = app.world.resource_mut::<atrium::Screen>() {
        *screen = atrium::Screen::Paused;
    }
    app.run_ticks(1).expect("a tick runs");
    let at_pause = app
        .world
        .get::<Transform>(lantern)
        .expect("still there")
        .rotation;
    app.run_ticks(120).expect("ticks run");

    assert_eq!(
        app.world
            .get::<Transform>(lantern)
            .expect("still there")
            .rotation,
        at_pause
    );
}

#[test]
fn the_room_animates_reproducibly() {
    // Invariant I3 over animation in the real game, which is the claim that matters more than the
    // unit test one crate down: this runs alongside physics, a character and the audio pass, and a
    // clock that drifted by a tick would show up here rather than there.
    let run = || {
        let mut app = room();
        app.run_ticks(200).expect("ticks run");
        app.state_hash()
    };
    assert_eq!(run(), run());
}
