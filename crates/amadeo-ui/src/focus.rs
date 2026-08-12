//! Moving between things and choosing one — the half of a menu that is gameplay.
//!
//! # The problem this solves, which is not the obvious one
//!
//! The obvious way to build a menu is: find the button under the mouse, highlight it, and act when
//! it is clicked. That works and it **cannot be part of a deterministic simulation**, for a reason
//! worth stating precisely.
//!
//! Hit-testing reads a [`ComputedRect`](crate::ComputedRect), and layout depends on the **window
//! size** (ADR 0062). So "which button is under the pointer" has a different answer at 1920×1080 and
//! at 1280×720. If that answer reached the state hash, the same inputs would produce different
//! worlds on two machines, and invariant I3 would be gone — not subtly, but for every menu in every
//! game.
//!
//! # So focus is an authored order, not a spatial search
//!
//! [`Focusable::order`] is a number somebody writes in a scene file. Moving focus walks that order.
//! Nothing here reads a rectangle, a pointer, or a screen size, so:
//!
//! - it is identical at every resolution;
//! - it is driven by **named actions**, which `InputState` already hashes and `amadeo-input` already
//!   records and replays — so a menu replays with no new machinery and no change to the replay
//!   format;
//! - it works on a controller and a keyboard without a cursor, which is what a console-facing menu
//!   needs anyway.
//!
//! Spatial navigation — "the button visually below this one" — is what a mouse and a d-pad *feel*
//! like, and it can be added later as a **presentation-side** helper that sets focus. Setting focus
//! from a pointer is resolution-dependent by nature; the honest place for that is a system that runs
//! outside the deterministic zone and writes through the same [`Focus`] resource. See ADR 0063.
//!
//! # What a game does with this
//!
//! Reads [`UiActivated`] events. A button does not "do" anything by itself — it says it was chosen,
//! and the game decides what that means, which is invariant I4 one level up: the engine knows how a
//! menu moves, the game knows what its buttons are for.

use crate::components::UiNode;
use amadeo_core::StableHash;
use amadeo_ecs::{Component, Entity, Resource, World};
use amadeo_events::{Event, WorldEvents};
use amadeo_input::{ActionId, InputState};
use amadeo_reflect::Reflect;

/// The action that moves focus to the next item.
pub const UI_NEXT: &str = "ui_next";
/// The action that moves focus to the previous item.
pub const UI_PREVIOUS: &str = "ui_previous";
/// The action that chooses the focused item.
pub const UI_CONFIRM: &str = "ui_confirm";

/// The label the app layer registers [`navigate_focus`] under.
pub const NAVIGATE_FOCUS: &str = "navigate_focus";

/// Something that can be focused and chosen.
///
/// # Why the order is authored
///
/// See the module docs: the alternative is to derive it from where things ended up on screen, and
/// where things end up depends on the window size. An authored order is the same everywhere.
///
/// It is also what a designer wants. Reading order and tab order are not always the same thing —
/// a "Back" button at the top-left should usually be *last* — and a spatial rule cannot express that
/// without a special case.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, StableHash, Reflect)]
pub struct Focusable {
    /// Where this sits in the traversal order. Lower comes first.
    ///
    /// Ties break by entity, which is the order a scene file lists them — so two items left at the
    /// default still traverse in the order they were written rather than arbitrarily.
    pub order: i32,
    /// Whether it can currently be focused at all.
    ///
    /// A greyed-out option: still laid out, still drawn, skipped by navigation. A field rather than
    /// removing the component, for the reason `UiNode::visible` is one — toggling it must not move
    /// the entity between archetypes.
    pub enabled: bool,
}

impl Focusable {
    /// A focusable item at a given position in the order.
    #[must_use]
    pub fn at(order: i32) -> Self {
        Self {
            order,
            enabled: true,
        }
    }
}

impl Component for Focusable {}

/// Which item is focused.
///
/// # Hashed, and that is the point
///
/// A [`Resource`], so it **is** in the state hash — unlike everything else in this crate. That is
/// correct rather than inconsistent: where the focus sits is gameplay state, it changes only through
/// recorded input actions, and it does not depend on the window size. A replay that pressed "down,
/// down, confirm" reproduces exactly, at any resolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, StableHash, Reflect)]
pub struct Focus {
    /// The focused entity, or `None` when nothing is.
    pub entity: Option<Entity>,
}

impl Resource for Focus {}

/// A focused item was chosen.
///
/// Carries the entity rather than a name, so a game matches on the button it spawned or authored.
/// **The engine does not know what a button means** — invariant I4 one level up.
#[derive(Debug, Clone, Copy, PartialEq, Eq, StableHash, Reflect)]
pub struct UiActivated {
    /// Which item was chosen.
    pub entity: Entity,
}

impl Event for UiActivated {}

/// Moves focus and raises [`UiActivated`], from named input actions.
///
/// Registered in a **simulation** stage, unlike everything else in this crate: it reads hashed input
/// and writes hashed state, so it belongs inside the deterministic zone rather than beside the draw
/// pass.
///
/// Does nothing without an [`InputState`] or a [`Focus`] resource, which is the headless case for a
/// game that has no menu.
pub fn navigate_focus(world: &mut World) {
    let items = focusable_in_order(world);
    if items.is_empty() {
        // Nothing focusable. Clear the focus rather than leaving it on an entity that has been
        // despawned or disabled — a stale focus is how "confirm" activates a button that is no
        // longer on screen.
        if let Some(focus) = world.resource_mut::<Focus>()
            && focus.entity.is_some()
        {
            focus.entity = None;
        }
        return;
    }

    let Some(input) = world.resource::<InputState>() else {
        return;
    };
    let next = input.just_pressed(ActionId::new(UI_NEXT));
    let previous = input.just_pressed(ActionId::new(UI_PREVIOUS));
    let confirm = input.just_pressed(ActionId::new(UI_CONFIRM));

    let Some(focus) = world.resource::<Focus>() else {
        return;
    };
    let current = focus.entity;

    // Where the focus is in the order, if it is still on something focusable. An entity that was
    // despawned, hidden or disabled since last tick simply is not found, and focus falls back to the
    // first item — which is what a menu should do when its selection disappears.
    let position = current.and_then(|entity| items.iter().position(|item| *item == entity));

    let moved = match (position, next, previous) {
        // **Both at once does nothing**, rather than one winning. Two directions in one tick is a
        // contradictory instruction, and picking a winner would make the result depend on which
        // branch was written first.
        (_, true, true) => position,
        (Some(index), true, false) => Some((index + 1) % items.len()),
        (Some(index), false, true) => Some((index + items.len() - 1) % items.len()),
        (Some(index), false, false) => Some(index),
        // Nothing focused yet. Any navigation lands on the first item, which is what pressing a
        // direction on a fresh menu should do.
        (None, true, false) | (None, false, true) => Some(0),
        (None, false, false) => None,
    };

    let focused = moved.map(|index| items[index]);

    if let Some(focus) = world.resource_mut::<Focus>() {
        focus.entity = focused;
    }

    // **Confirm applies to where the focus ended up**, so pressing a direction and confirm in one
    // tick chooses the item moved to. That is the reading a player expects and it costs nothing;
    // the alternative is an activation that depends on the order two systems happened to run in.
    if confirm && let Some(entity) = focused {
        world.send_event(UiActivated { entity });
    }
}

/// Every focusable item that can currently take focus, in traversal order.
///
/// Skips anything hidden or disabled. Sorted by authored order, then by entity — which is the order
/// a scene file lists them, so items left at the default order still traverse the way they were
/// written rather than arbitrarily.
fn focusable_in_order(world: &World) -> Vec<Entity> {
    let mut items: Vec<(i32, Entity)> = world
        .query::<(&Focusable, &UiNode)>()
        .filter(|(_, (focusable, node))| focusable.enabled && node.visible)
        .map(|(entity, (focusable, _))| (focusable.order, entity))
        .collect();

    items.sort_by_key(|(order, entity)| (*order, entity.index(), entity.generation()));
    items.into_iter().map(|(_, entity)| entity).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use amadeo_ecs::World;

    /// A world with a focus resource, an input state, and `count` focusable items in order.
    fn menu(count: i32) -> (World, Vec<Entity>) {
        let mut world = World::new();
        world.insert_resource(Focus::default());
        world.insert_resource(InputState::new());
        world.register_event::<UiActivated>();

        let items = (0..count)
            .map(|index| {
                let entity = world.spawn();
                world.insert(entity, UiNode::sized(100.0, 30.0));
                world.insert(entity, Focusable::at(index));
                entity
            })
            .collect();
        (world, items)
    }

    /// Releases, then presses an action for one tick, then runs the system.
    ///
    /// **The release is not padding.** `just_pressed` is edge-triggered, so a helper that only ever
    /// set the button true would report a *held* key from the second call onwards and every test
    /// using it would silently stop moving the focus. Written without it first, and six tests failed
    /// at once — which was the code being right and the helper being wrong.
    fn press(world: &mut World, action: &str) {
        press_together(world, &[action]);
    }

    /// The same, for several actions pressed on one tick.
    fn press_together(world: &mut World, actions: &[&str]) {
        if let Some(input) = world.resource_mut::<InputState>() {
            input.begin_tick();
            for action in actions {
                input.set_button(ActionId::new(action), false);
            }
            input.begin_tick();
            for action in actions {
                input.set_button(ActionId::new(action), true);
            }
        }
        navigate_focus(world);
    }

    fn focused(world: &World) -> Option<Entity> {
        world.resource::<Focus>().expect("installed").entity
    }

    fn activations(world: &mut World) -> Vec<Entity> {
        world.swap_events::<UiActivated>();
        world
            .read_events::<UiActivated>()
            .iter()
            .map(|record| record.event.entity)
            .collect()
    }

    #[test]
    fn nothing_is_focused_until_something_moves() {
        // A menu that focused its first item the moment it appeared would steal the highlight from
        // whatever the game wanted focused, and would do it one tick after the scene loaded.
        let (mut world, _) = menu(3);
        navigate_focus(&mut world);
        assert_eq!(focused(&world), None);
    }

    #[test]
    fn the_first_press_lands_on_the_first_item() {
        let (mut world, items) = menu(3);
        press(&mut world, UI_NEXT);
        assert_eq!(focused(&world), Some(items[0]));
    }

    #[test]
    fn moving_walks_the_authored_order_and_wraps() {
        let (mut world, items) = menu(3);
        press(&mut world, UI_NEXT);
        press(&mut world, UI_NEXT);
        assert_eq!(focused(&world), Some(items[1]));

        press(&mut world, UI_NEXT);
        press(&mut world, UI_NEXT);
        // Past the end and round to the start, which is what a menu does.
        assert_eq!(focused(&world), Some(items[0]));
    }

    #[test]
    fn holding_a_direction_moves_once_rather_than_scrolling() {
        // **Found by the helper above being wrong**, which is a better way to find it than by a
        // player holding a key and watching a menu blur past. Navigation is edge-triggered on
        // purpose: a held direction is one instruction, not sixty a second.
        //
        // Key repeat — move, pause, then accelerate — is a real feature and a *timing* one, which
        // makes it a poor fit for a fixed-tick system that must replay identically. It is not here,
        // and this is what says that is deliberate.
        let (mut world, items) = menu(3);
        press(&mut world, UI_NEXT);
        assert_eq!(focused(&world), Some(items[0]));

        // Held: the tick rolls on and the button stays down.
        for _ in 0..10 {
            if let Some(input) = world.resource_mut::<InputState>() {
                input.begin_tick();
                input.set_button(ActionId::new(UI_NEXT), true);
            }
            navigate_focus(&mut world);
        }

        assert_eq!(
            focused(&world),
            Some(items[0]),
            "holding a direction for ten ticks should not have moved the focus"
        );
    }

    #[test]
    fn moving_backwards_from_the_first_item_wraps_to_the_last() {
        // The wrap most likely to be got wrong, because `index - 1` underflows on a `usize` and the
        // obvious fix is a branch somebody forgets.
        let (mut world, items) = menu(3);
        press(&mut world, UI_NEXT);
        press(&mut world, UI_PREVIOUS);
        assert_eq!(focused(&world), Some(items[2]));
    }

    #[test]
    fn the_order_is_authored_rather_than_the_order_things_were_spawned() {
        // **The property the whole design rests on.** If traversal came from spawn order, or from
        // where things landed on screen, it would depend on the scene's shape or the window size.
        let mut world = World::new();
        world.insert_resource(Focus::default());
        world.insert_resource(InputState::new());
        world.register_event::<UiActivated>();

        // Spawned last, ordered first.
        let second = world.spawn();
        world.insert(second, UiNode::sized(10.0, 10.0));
        world.insert(second, Focusable::at(10));

        let first = world.spawn();
        world.insert(first, UiNode::sized(10.0, 10.0));
        world.insert(first, Focusable::at(-5));

        press(&mut world, UI_NEXT);
        assert_eq!(focused(&world), Some(first));
        press(&mut world, UI_NEXT);
        assert_eq!(focused(&world), Some(second));
    }

    #[test]
    fn a_disabled_or_hidden_item_is_skipped() {
        let (mut world, items) = menu(3);
        world.insert(
            items[1],
            Focusable {
                enabled: false,
                ..Focusable::at(1)
            },
        );

        press(&mut world, UI_NEXT);
        press(&mut world, UI_NEXT);
        assert_eq!(focused(&world), Some(items[2]), "the middle one is skipped");

        // And hiding has the same effect, through a different field.
        world.insert(
            items[2],
            UiNode {
                visible: false,
                ..UiNode::sized(100.0, 30.0)
            },
        );
        press(&mut world, UI_NEXT);
        assert_eq!(focused(&world), Some(items[0]));
    }

    #[test]
    fn confirm_raises_an_event_naming_the_focused_item() {
        let (mut world, items) = menu(3);
        press(&mut world, UI_NEXT);
        press(&mut world, UI_CONFIRM);

        assert_eq!(activations(&mut world), vec![items[0]]);
    }

    #[test]
    fn confirming_with_nothing_focused_raises_nothing() {
        // The failure this prevents is a menu that activates its first item on the first keypress,
        // before the player has seen what is highlighted.
        let (mut world, _) = menu(3);
        press(&mut world, UI_CONFIRM);
        assert!(activations(&mut world).is_empty());
    }

    #[test]
    fn moving_and_confirming_in_one_tick_chooses_where_it_moved_to() {
        // Otherwise the result depends on which of two systems ran first, which is exactly the class
        // of ordering bug ADR 0005 exists to make impossible.
        let (mut world, items) = menu(3);
        press(&mut world, UI_NEXT);
        press_together(&mut world, &[UI_NEXT, UI_CONFIRM]);

        assert_eq!(focused(&world), Some(items[1]));
        assert_eq!(activations(&mut world), vec![items[1]]);
    }

    #[test]
    fn pressing_both_directions_at_once_does_nothing() {
        // Contradictory input. Picking a winner would make the answer depend on which branch was
        // written first, which is not a thing a player could ever predict.
        let (mut world, items) = menu(3);
        press(&mut world, UI_NEXT);
        press_together(&mut world, &[UI_NEXT, UI_PREVIOUS]);

        assert_eq!(focused(&world), Some(items[0]));
    }

    #[test]
    fn focus_falls_off_an_item_that_stops_being_focusable() {
        // A stale focus is how "confirm" activates a button that is no longer on screen — the
        // pause-menu bug where closing a menu and pressing a key triggers whatever was highlighted.
        let (mut world, items) = menu(3);
        press(&mut world, UI_NEXT);
        press(&mut world, UI_NEXT);
        assert_eq!(focused(&world), Some(items[1]));

        world.despawn(items[1]);
        navigate_focus(&mut world);
        assert_ne!(focused(&world), Some(items[1]));
    }

    #[test]
    fn a_menu_with_nothing_focusable_clears_the_focus() {
        let (mut world, items) = menu(1);
        press(&mut world, UI_NEXT);
        assert!(focused(&world).is_some());

        world.despawn(items[0]);
        navigate_focus(&mut world);
        assert_eq!(focused(&world), None);
    }

    #[test]
    fn navigating_is_identical_at_any_window_size() {
        // **The claim the module docs make, checked rather than argued.** Nothing here reads a
        // rectangle, so laying the same menu out at two resolutions and navigating it must produce
        // the same focus and the same state hash. If focus ever became spatial, this is what fails.
        let hash_after = |width: f32, height: f32| {
            let (mut world, _) = menu(4);
            crate::layout_ui(&mut world, width, height);
            press(&mut world, UI_NEXT);
            press(&mut world, UI_NEXT);
            press(&mut world, UI_NEXT);
            (world.state_hash(), focused(&world))
        };

        assert_eq!(hash_after(1920.0, 1080.0), hash_after(1280.0, 720.0));
    }
}
