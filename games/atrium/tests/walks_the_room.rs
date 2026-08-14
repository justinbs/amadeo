//! The room is real: it loads from text, it holds someone up, and its walls stop them.
//!
//! Headless, and every assertion is about the *same* `build_simulation` the windowed game runs — so
//! these are claims about the game rather than about a second world written to be testable
//! (invariant I7).
//!
//! What these deliberately cannot check is whether it **looks** right. That is what running it is
//! for, and it is why this game exists at all: three parts of M2's gate 1 were each proved by tests
//! like these and had never been seen together.

use amadeo_app::App;
use amadeo_character::{CharacterController, CharacterMotion, MOVE_FORWARD, MOVE_RIGHT};
use amadeo_ecs::Entity;
use amadeo_input::{ActionId, InputDriver, InputState, ScriptedSource};
use amadeo_physics::{Collider, RigidBody};
use amadeo_render::{Camera, DirectionalLight, Mesh, ShadowMode};
use amadeo_transform::{GlobalTransform, Parent, Transform};

/// The room with a scripted input source, so a test can hold a direction.
fn room() -> App {
    let mut app = atrium::build_simulation().expect("the room builds");
    amadeo_input::install(
        &mut app.world,
        InputDriver::new(Box::new(ScriptedSource::new())),
    );
    app
}

/// The only entity carrying a [`CharacterController`] — the player.
fn player(app: &App) -> Entity {
    app.world
        .query::<(&CharacterController,)>()
        .map(|(entity, _)| entity)
        .next()
        .expect("the scene authored exactly one character")
}

fn position(app: &App, entity: Entity) -> [f32; 3] {
    app.world
        .get::<Transform>(entity)
        .expect("still there")
        .translation
}

/// Runs `ticks` ticks with an axis held at `value`.
fn hold(app: &mut App, action: &str, value: f32, ticks: u64) {
    // Set directly on the resource each tick rather than scripted ahead of time: `sample_input`
    // rolls current values into previous ones and then applies the source, so a value written here
    // survives into the tick that reads it.
    for _ in 0..ticks {
        if let Some(state) = app.world.resource_mut::<InputState>() {
            state.set_axis(ActionId::new(action), value);
        }
        app.run_ticks(1).expect("a tick runs");
    }
}

#[test]
fn the_interface_is_authored_in_the_scene_file_like_everything_else() {
    // **Invariant I1 for the interface.** A menu that had to be built in Rust would be a menu the
    // agent could not author and the editor could not show, which is the whole reason `amadeo-ui` is
    // retained rather than immediate (ADR 0062).
    //
    // What this catches is narrow and worth having: the scene declaring a component the game forgot
    // to register, or the font id drifting out of step with the `assets` block. Both present as an
    // interface that is silently absent.
    let app = room();

    let panels = app.world.query::<(&amadeo_ui::Panel,)>().count();
    let labels: Vec<&amadeo_ui::Text> = app
        .world
        .query::<(&amadeo_ui::Text,)>()
        .map(|(_, (text,))| text)
        .collect();

    assert_eq!(
        panels, 7,
        "the title's backing plate, the pause panel, and five buttons"
    );

    let mut spoken: Vec<&str> = labels.iter().map(|text| text.content.as_str()).collect();
    spoken.sort_unstable();
    assert_eq!(
        spoken,
        [
            "LOAD",
            "PAUSED",
            "QUIT",
            "RESUME",
            "RETURN TO START",
            "SAVE",
            "THE ATRIUM"
        ]
    );

    // Every font is named by asset id (ADR 0020), and the id has to be one the scene declares.
    for label in &labels {
        assert_eq!(label.font, "BebasNeue-Regular", "{:?}", label.content);
    }
}

#[test]
fn the_room_loads_from_its_scene_file() {
    // Everything in this game is text (I1). If the scene stopped parsing or stopped matching the
    // registered components, this is what would say so — rather than a blank window.
    let app = room();

    let meshes = app.world.query::<(&Mesh,)>().count();
    assert_eq!(
        meshes, 11,
        "a floor, four walls, four pillars, a plinth and the player's body"
    );
    assert_eq!(app.world.query::<(&Camera,)>().count(), 1);
    assert_eq!(app.world.query::<(&DirectionalLight,)>().count(), 1);
    // Static geometry plus the character, all with a shape physics can use.
    assert_eq!(app.world.query::<(&RigidBody, &Collider)>().count(), 11);
}

#[test]
fn the_sun_casts_shadows() {
    // The setting that makes this a shadow demo rather than a lighting one, read off the authored
    // component rather than assumed from the file's contents.
    let app = room();
    let (_, (sun,)) = app
        .world
        .query::<(&DirectionalLight,)>()
        .next()
        .expect("one sun");
    assert_eq!(sun.shadows, ShadowMode::Orthogonal);
    assert!(
        sun.shadow_distance >= 20.0,
        "the box has to cover a 20-unit room, got {}",
        sun.shadow_distance
    );
}

#[test]
fn the_character_stands_on_the_floor_rather_than_falling_through_it() {
    // Rapier is on for this game, so this is the real solver rather than the null one -- and the
    // resting height is half the capsule's total height above the floor's top surface.
    let mut app = room();
    let player = player(&app);
    app.run_ticks(120).expect("two seconds");

    let height = position(&app, player)[1];
    assert!(
        (height - 1.0).abs() < 0.1,
        "should be resting at about 1.0, got {height}"
    );
    assert!(
        app.world
            .get::<CharacterMotion>(player)
            .expect("there")
            .grounded
    );
}

#[test]
fn a_wall_stops_the_wanderer() {
    // The player starts at z = 5 facing -Z, so walking forward crosses the room. The north wall's
    // inner face is at z = -9.75, and the capsule's radius plus its skin keeps it short of that.
    let mut app = room();
    let player = player(&app);

    hold(&mut app, MOVE_FORWARD, 1.0, 300);

    let z = position(&app, player)[2];
    assert!(
        z > -9.75,
        "the north wall is at z = -9.75; the wanderer reached {z}"
    );
    assert!(
        z < -3.0,
        "but it should have crossed most of the room, not been stuck at the start; got {z}"
    );
}

#[test]
fn a_pillar_is_solid_too() {
    // Walls are one shape at the room's edge. A pillar is a different size in the middle of the
    // floor, so this is what says the room is *made of* collision rather than having a collidable
    // outline — which is what it would be if only the walls worked.
    //
    // Written the lazy way first, walking sideways into the east wall and calling it a pillar test.
    // It passed, and it was checking the same thing `a_wall_stops_the_wanderer` already does.
    let mut app = room();
    let player = player(&app);

    // Line the wanderer up with the northeast pillar, which stands at x = 4 and spans z = -4.5 to
    // -3.5. Placed rather than walked to, so the test is about the pillar and not about steering.
    let mut start = *app.world.get::<Transform>(player).expect("there");
    start.translation = [4.0, 1.0, 2.0];
    app.world.insert(player, start);

    hold(&mut app, MOVE_FORWARD, 1.0, 200);

    let at = position(&app, player);
    assert!(
        at[2] > -3.5,
        "the pillar's near face is at z = -3.5; the wanderer reached {at:?}"
    );
    assert!(
        at[2] < 0.0,
        "but it should have walked up to it rather than never moving; got {at:?}"
    );
    // And it stayed lined up with the pillar rather than sliding off it, which is what says it was
    // stopped by the pillar rather than by nothing at all.
    assert!(
        (at[0] - 4.0).abs() < 1.5,
        "should have stopped in front of the pillar, not slid past it; got {at:?}"
    );
}

#[test]
fn the_camera_is_a_child_of_the_player_and_follows_it() {
    // **ADR 0031's claim being cashed rather than repeated**: a camera parented to a character *is*
    // a follow camera, with no special case anywhere in the engine. The scene file nests one entity
    // inside another and that is the whole mechanism.
    let mut app = room();
    let player = player(&app);
    let (eye, _) = app.world.query::<(&Camera,)>().next().expect("one camera");

    assert_eq!(
        app.world.get::<Parent>(eye).map(|parent| parent.0),
        Some(player),
        "the camera should be parented to the player"
    );

    app.run_ticks(30).expect("settle");
    let before = app
        .world
        .get::<GlobalTransform>(eye)
        .expect("propagated")
        .translation();

    hold(&mut app, MOVE_FORWARD, 1.0, 60);

    let after = app
        .world
        .get::<GlobalTransform>(eye)
        .expect("propagated")
        .translation();
    assert!(
        (before[2] - after[2]).abs() > 1.0,
        "the camera should have moved with the player; {before:?} then {after:?}"
    );
}

#[test]
fn the_room_is_reproducible() {
    // I3, on the whole game rather than on a subsystem. Rapier's `enhanced-determinism` is what
    // makes this meaningful with a real solver in the loop (ADR 0036).
    let run = || {
        let mut app = room();
        let player = player(&app);
        hold(&mut app, MOVE_FORWARD, 1.0, 90);
        hold(&mut app, MOVE_RIGHT, 1.0, 90);
        (app.world.state_hash(), position(&app, player))
    };
    assert_eq!(run(), run());
}
