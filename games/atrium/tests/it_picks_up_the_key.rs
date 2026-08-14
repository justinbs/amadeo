//! Walking up to a thing, using it, and carrying it away — in the real game.
//!
//! # Why this test is the point of the work rather than a check on it
//!
//! `modules/amadeo-interaction` was built in session 17 **with no game using it**, which is the
//! "designed against zero users" risk `CLAUDE.md` names and which `amadeo-camera` avoided by living
//! in a game first. `modules/amadeo-inventory` was written in the same session as this file for the
//! same reason. This is what makes both of them reviewed rather than merely tested: the sweep has to
//! find a real collider in a real room, the item has to leave the world, and the join between the
//! two modules has to be something *this game* wrote.
//!
//! It needs a solver. Against `NullPhysics` every cast reports clear, so nothing is ever in reach —
//! which is `modules/amadeo-interaction`'s own control case, and the reason `games/atrium` enables
//! `amadeo-physics/rapier`.

use amadeo_app::App;
use amadeo_character::{CharacterController, MOVE_FORWARD};
use amadeo_core::Tick;
use amadeo_ecs::{Entity, World};
use amadeo_input::{ActionId, InputDriver, InputState, ScriptedSource};
use amadeo_interaction::{Interactable, Interactor, Looking, USE};
use amadeo_inventory::{Inventory, Item, StoredIn, contents, count_of, drop_at, store};
use amadeo_transform::Transform;

fn room() -> App {
    let mut app = atrium::build_simulation().expect("the room builds");
    amadeo_input::install(
        &mut app.world,
        InputDriver::new(Box::new(ScriptedSource::new())),
    );
    app
}

fn player(app: &App) -> Entity {
    app.world
        .query::<(&CharacterController,)>()
        .map(|(entity, _)| entity)
        .next()
        .expect("one character")
}

/// The child entity that does the reaching.
///
/// **Not the player.** A sweep is horizontal and starts wherever the interactor is, so one at the
/// capsule's centre travels at plinth-top height and stops against the plinth's front face — what a
/// thing rests on is exactly what blocks the sweep to it. So the `Interactor` sits on a child a
/// little higher up, which is also the arrangement `modules/amadeo-interaction`'s own docs call the
/// usual one and which no game had exercised until this one.
fn hand(app: &App) -> Entity {
    app.world
        .query::<(&Interactor,)>()
        .map(|(entity, _)| entity)
        .next()
        .expect("one interactor")
}

fn key(world: &World) -> Option<Entity> {
    world
        .query::<(&Item,)>()
        .filter(|(_, (item,))| item.kind == "brass_key")
        .map(|(entity, _)| entity)
        .next()
}

/// Walks forward for `ticks` ticks.
fn walk(app: &mut App, ticks: u64) {
    for _ in 0..ticks {
        if let Some(state) = app.world.resource_mut::<InputState>() {
            state.set_axis(ActionId::new(MOVE_FORWARD), 1.0);
        }
        app.run_ticks(1).expect("a tick runs");
    }
}

/// Presses "use" for exactly one tick, then releases it.
///
/// # Through the scripted source, not `InputState`
///
/// `USE` is edge-triggered — `update_interactions` asks `just_pressed` — and `sample_input` rolls
/// current values into previous ones **before** applying the source. A value written straight onto
/// the resource therefore arrives already looking like a held key, so `just_pressed` is false and
/// the press is never seen at all.
///
/// `the_menu_pauses_it.rs` documents this trap in full and paid for it with five failing tests. The
/// axis in `walk` is written the other way precisely because an axis has no edge to miss.
fn tap_use(app: &mut App) {
    let now = app.tick();
    let release = Tick(now.0 + 1);
    app.world
        .with_service_taken::<InputDriver, ()>(|_world, driver| {
            if let Some(scripted) = driver.source.as_any_mut().downcast_mut::<ScriptedSource>() {
                scripted.press(now, USE, true);
                scripted.press(release, USE, false);
            }
        });
    app.run_ticks(2).expect("ticks run");
}

#[test]
fn the_key_starts_on_the_plinth_and_is_something_you_can_use() {
    let app = room();
    let key = key(&app.world).expect("the scene puts a brass key in the room");

    assert!(
        app.world.get::<Transform>(key).is_some(),
        "it is in the world to begin with"
    );
    assert_eq!(
        app.world
            .get::<Interactable>(key)
            .map(|i| i.prompt.as_str()),
        Some("Pick up the brass key"),
        "and it says what using it would do -- authored in the scene, not in code"
    );
    assert!(app.world.get::<StoredIn>(key).is_none());
}

#[test]
fn the_bag_is_on_the_player_and_the_reach_is_on_a_child() {
    // Both authored in `atrium.scene`, which is the claim worth checking: neither module needed a
    // line of setup code to be wired into a game (I1).
    //
    // They are on *different entities*, which is the shape the sweep's geometry forces — see
    // `hand` — and it is why this game has a `carrier_of` walk at all. A module that assumed the
    // interactor was also the container would have made this arrangement impossible.
    let app = room();
    let player = player(&app);

    assert!(app.world.get::<Interactor>(player).is_none());
    assert_ne!(hand(&app), player);
    assert_eq!(
        app.world.get::<Inventory>(player).map(|bag| bag.slots),
        Some(6)
    );
}

#[test]
fn walking_up_to_the_key_and_using_it_puts_it_in_your_pocket() {
    // The whole sentence, end to end, through the real solver.
    let mut app = room();
    let player = player(&app);
    let key = key(&app.world).expect("a key");

    // The plinth is at z = -6 and the player starts at z = 2, facing it.
    walk(&mut app, 120);

    let looking = app
        .world
        .get::<Looking>(hand(&app))
        .copied()
        .expect("the interactor writes one every tick");
    assert_eq!(
        looking.at,
        Some(key),
        "the sweep should find the key on the plinth. `None` here means one of three things, and \
         all three have happened: the walk did not get close enough, the key has no collider for \
         the cast to hit, or the sweep began inside the player's own capsule and reported that \
         instead -- which is what `body_of` in the module exists to prevent"
    );

    tap_use(&mut app);

    assert_eq!(contents(&app.world, player), vec![(0, key)]);
    assert_eq!(count_of(&app.world, player, "brass_key"), 1);
    assert!(
        app.world.get::<Transform>(key).is_none(),
        "and it left the world, which is the whole of ADR 0070's mechanism"
    );
}

#[test]
fn using_something_that_is_not_an_item_does_not_try_to_pocket_it() {
    // The Atrium has no doors, so this is asserted the other way round: the join only fires for
    // things carrying an `Item`, and nothing else in the room does. If `pick_up_what_you_used` ever
    // stopped checking, the first `Interactable` that is not an item would vanish into a bag.
    let app = room();
    let interactable_items = app.world.query::<(&Interactable, &Item)>().count();
    let interactable_total = app.world.query::<(&Interactable,)>().count();

    assert_eq!(
        interactable_items, interactable_total,
        "every interactable in this room is an item today, so this test is a tripwire for the day \
         one is not -- add a door and it should still pass"
    );
}

#[test]
fn a_carried_key_survives_a_save_and_resume() {
    // The reason ADR 0070 is happy for a stored item to stay an entity: it is ordinary world state,
    // so it snapshots with everything else and needed nothing built.
    let mut app = room();
    let player = player(&app);
    let key = key(&app.world).expect("a key");
    store(&mut app.world, key, player).expect("straight into the bag");

    let text = amadeo_snapshot::to_text(&app.capture_snapshot());

    let mut resumed = room();
    let snapshot = amadeo_snapshot::parse(&text).expect("parses");
    let report = resumed
        .restore_save(&snapshot, &amadeo_snapshot::Redirects::new())
        .expect("restores");

    assert!(report.exact, "{:?}", report.lines());
    assert_eq!(contents(&resumed.world, player), vec![(0, key)]);
    assert!(
        resumed.world.get::<Transform>(key).is_none(),
        "and it is still out of the world on the other side"
    );
}

#[test]
fn dropping_it_puts_it_back_in_the_room() {
    let mut app = room();
    let player = player(&app);
    let key = key(&app.world).expect("a key");

    store(&mut app.world, key, player).expect("into the bag");
    drop_at(&mut app.world, key, [1.0, 0.5, -2.0]).expect("it was stored");

    // Everything about it survived the round trip untouched, which is what makes an entity item
    // worth the entity: nothing was ever converted into a value and back.
    assert_eq!(
        app.world.get::<Transform>(key).map(|t| t.translation),
        Some([1.0, 0.5, -2.0])
    );
    assert!(
        app.world.get::<Interactable>(key).is_some(),
        "you can pick it up again"
    );
    assert!(contents(&app.world, player).is_empty());

    // And the room can still simulate it: a tick with the key back on the floor must not trip
    // anything up in physics or rendering collection.
    app.run_ticks(5).expect("the room runs on");
}
