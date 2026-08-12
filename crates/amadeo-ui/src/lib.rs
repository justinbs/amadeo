//! Retained-mode game UI: anchors, flow layout, and text — ADR 0062.
//!
//! # What this is, and what it deliberately is not
//!
//! This is the **game's** interface: menus, HUD, dialogue, inventory screens. It is *not* `egui`,
//! which is the editor's UI and a different problem with different requirements. Conflating them is
//! a mistake `docs/04` §13 names twice.
//!
//! The difference that decides everything else is **retained versus immediate mode**. An
//! immediate-mode widget exists only for the duration of a function call, which means:
//!
//! - an agent cannot inspect it, breaking invariant I5 and the whole observability story;
//! - a scene file cannot author it, breaking invariant I1.
//!
//! So a widget here is an **entity with components**, like everything else in this engine — which is
//! ADR 0031's argument for the camera, applied again. `world.query` sees a menu, `describe` reports
//! it, a snapshot restores it, and none of that needed building.
//!
//! # Layout in one paragraph
//!
//! A node is placed inside its parent by an [`Anchor`] — one [`Align`] per axis, so sixteen useful
//! placements come from four names. A parent with a [`Flow`] instead arranges its children in a row
//! or a column, sharing out leftover space by [`UiNode::grow`]. That pair is what every mature engine
//! converges on, because a HUD is a placement problem and a menu is a flow problem and one model does
//! one of them badly.
//!
//! ```
//! use amadeo_ecs::World;
//! use amadeo_transform::Parent;
//! use amadeo_ui::{Align, Anchor, ComputedRect, UiEdges, UiNode, layout_ui};
//!
//! let mut world = World::new();
//!
//! // A panel pinned to the top-right corner, twenty pixels in.
//! let screen = world.spawn();
//! world.insert(screen, UiNode::full());
//!
//! let badge = world.spawn();
//! world.insert(badge, Parent(screen));
//! world.insert(
//!     badge,
//!     UiNode {
//!         anchor: Anchor::new(Align::End, Align::Start),
//!         margin: UiEdges::all(20.0),
//!         ..UiNode::sized(100.0, 40.0)
//!     },
//! );
//!
//! layout_ui(&mut world, 1280.0, 720.0);
//!
//! let rect = world.get::<ComputedRect>(badge).expect("laid out");
//! assert_eq!(rect.right(), 1260.0); // 1280 - 20
//! assert_eq!(rect.top, 20.0);
//! ```
//!
//! # Screen space, and the flip that comes with it
//!
//! UI is authored in the space it is drawn in: the origin is the **top-left** and +Y points
//! **down**, which is the opposite of the world convention in ADR 0018. That is deliberate — "twenty
//! pixels from the top" is what a person means — and it is the seam most likely to be got wrong,
//! because a layout with the flip backwards is plausible and upside down.

mod atlas;
mod components;
mod draw;
mod focus;
mod layout;
mod text;

/// A valid TrueType font built in code, so every text test needs no fixture and no system fonts.
///
/// **A fixture, not API.** Compiled for this crate's own tests, and also when `gpu` is on so that
/// `tests/it_draws.rs` — a separate crate, which cannot see `#[cfg(test)]` items — can draw a real
/// glyph and look at the pixels. Hidden from the documentation because nothing outside should build
/// a game on a one-letter box.
#[cfg(any(test, feature = "gpu"))]
#[doc(hidden)]
pub mod test_font;

pub use atlas::{GLYPH_ATLAS_ID, GlyphAtlas, GlyphImage};
pub use components::{Align, Anchor, ComputedRect, Flow, Panel, Text, UiEdges, UiNode};
pub use draw::{UI_ORDER, collect_ui};
pub use focus::{
    Focus, Focusable, NAVIGATE_FOCUS, UI_CONFIRM, UI_NEXT, UI_PREVIOUS, UiActivated, navigate_focus,
};
pub use layout::layout_ui;
pub use text::{FontCache, FontFailure, PositionedGlyph, ShapedText};

/// The label the app layer registers [`collect_ui`] under.
pub const COLLECT_UI: &str = "collect_ui";

/// The label the app layer registers [`layout_ui_system`] under.
pub const LAYOUT_UI: &str = "layout_ui";

/// The layout pass as a system, taking the screen size from the installed renderer.
///
/// [`layout_ui`] takes an explicit size because a test wants to name one, and because layout has no
/// business knowing what a renderer is. This is the thin wrapper that makes it registrable, and it
/// is the only place the two meet.
///
/// **Does nothing when no renderer is installed.** A headless run has no window and therefore no
/// screen size to lay out against; guessing one would produce rectangles that describe nothing. A
/// test that wants layout without a renderer calls [`layout_ui`] directly, which is what every test
/// in this crate does.
pub fn layout_ui_system(world: &mut amadeo_ecs::World) {
    let Some((width, height)) = world
        .service::<amadeo_render::Renderer>()
        .map(amadeo_render::Renderer::viewport)
    else {
        return;
    };
    layout_ui(world, width as f32, height as f32);
}

#[cfg(test)]
mod tests {
    use super::*;
    use amadeo_ecs::{Entity, World};
    use amadeo_transform::Parent;

    /// A world with a full-screen root, and that root.
    fn screen() -> (World, Entity) {
        let mut world = World::new();
        let root = world.spawn();
        world.insert(root, UiNode::full());
        (world, root)
    }

    fn child(world: &mut World, parent: Entity, node: UiNode) -> Entity {
        let entity = world.spawn();
        world.insert(entity, Parent(parent));
        world.insert(entity, node);
        entity
    }

    fn rect(world: &World, entity: Entity) -> ComputedRect {
        *world
            .get::<ComputedRect>(entity)
            .expect("this node should have been laid out")
    }

    #[test]
    fn a_full_root_becomes_the_whole_screen() {
        let (mut world, root) = screen();
        layout_ui(&mut world, 1280.0, 720.0);

        let screen = rect(&world, root);
        assert_eq!(screen.left, 0.0);
        assert_eq!(screen.top, 0.0);
        assert_eq!(screen.width, 1280.0);
        assert_eq!(screen.height, 720.0);
    }

    #[test]
    fn each_corner_anchors_where_its_name_says() {
        // Four assertions that would all pass if the axes were swapped, and one that would not:
        // top-right and bottom-left are mirror images, so both are checked.
        let (mut world, root) = screen();
        let corner = |align| UiNode {
            anchor: align,
            margin: UiEdges::all(10.0),
            ..UiNode::sized(100.0, 50.0)
        };

        let top_left = child(
            &mut world,
            root,
            corner(Anchor::new(Align::Start, Align::Start)),
        );
        let top_right = child(
            &mut world,
            root,
            corner(Anchor::new(Align::End, Align::Start)),
        );
        let bottom_left = child(
            &mut world,
            root,
            corner(Anchor::new(Align::Start, Align::End)),
        );

        layout_ui(&mut world, 1000.0, 600.0);

        assert_eq!(
            (rect(&world, top_left).left, rect(&world, top_left).top),
            (10.0, 10.0)
        );
        // Pinned to the right edge: its *right* side is 10 from the screen's, not its left.
        assert_eq!(rect(&world, top_right).right(), 990.0);
        assert_eq!(rect(&world, top_right).top, 10.0);
        // +Y is **down**, so "bottom" is the larger y. This is the assertion that fails if the
        // screen-space flip is ever got backwards.
        assert_eq!(rect(&world, bottom_left).bottom(), 590.0);
        assert_eq!(rect(&world, bottom_left).left, 10.0);
    }

    #[test]
    fn a_stretched_node_uses_its_margins_as_insets() {
        // The spelling for "a panel twenty pixels from every edge", with no knowledge of the screen
        // size — which is the entire point of an anchor over a position.
        let (mut world, root) = screen();
        let panel = child(
            &mut world,
            root,
            UiNode {
                anchor: Anchor::fill(),
                margin: UiEdges::all(20.0),
                // Deliberately absurd, to prove `Stretch` ignores it.
                ..UiNode::sized(5.0, 5.0)
            },
        );

        layout_ui(&mut world, 800.0, 600.0);

        let panel = rect(&world, panel);
        assert_eq!(panel.left, 20.0);
        assert_eq!(panel.top, 20.0);
        assert_eq!(panel.width, 760.0);
        assert_eq!(panel.height, 560.0);
    }

    #[test]
    fn padding_is_inside_and_margin_is_outside() {
        // The distinction that makes nested layouts reasonable, and the one most likely to be
        // conflated. The parent's padding shrinks where children may go; the child's margin shifts
        // it within that.
        let (mut world, root) = screen();
        let panel = child(
            &mut world,
            root,
            UiNode {
                anchor: Anchor::fill(),
                padding: UiEdges::all(30.0),
                ..UiNode::default()
            },
        );
        let inner = child(
            &mut world,
            panel,
            UiNode {
                margin: UiEdges::all(5.0),
                ..UiNode::sized(10.0, 10.0)
            },
        );

        layout_ui(&mut world, 400.0, 400.0);

        // The panel itself is the whole screen — padding does not shrink the node, only its content.
        assert_eq!(rect(&world, panel).width, 400.0);
        // 30 of padding then 5 of margin.
        assert_eq!(rect(&world, inner).left, 35.0);
        assert_eq!(rect(&world, inner).top, 35.0);
    }

    #[test]
    fn a_column_stacks_children_with_gaps_between_them() {
        let (mut world, root) = screen();
        let menu = child(&mut world, root, UiNode::column(10.0));
        let buttons: Vec<Entity> = (0..3)
            .map(|_| child(&mut world, menu, UiNode::sized(200.0, 40.0)))
            .collect();

        layout_ui(&mut world, 800.0, 600.0);

        // Three 40-high buttons with two 10-gaps between them.
        assert_eq!(rect(&world, buttons[0]).top, 0.0);
        assert_eq!(rect(&world, buttons[1]).top, 50.0);
        assert_eq!(rect(&world, buttons[2]).top, 100.0);
        // `column` centres its children across the flow, so each is centred horizontally.
        assert_eq!(rect(&world, buttons[0]).left, 300.0);
        assert_eq!(rect(&world, buttons[0]).width, 200.0);
    }

    #[test]
    fn a_column_keeps_the_order_the_scene_file_wrote() {
        // **Load-bearing rather than incidental.** A flow layout whose child order came from a hash
        // map would shuffle a menu between runs, and the symptom is buttons swapping places on
        // restart. Entity order is spawn order is the order a scene file lists them.
        let (mut world, root) = screen();
        let menu = child(&mut world, root, UiNode::column(0.0));
        let first = child(&mut world, menu, UiNode::sized(100.0, 10.0));
        let second = child(&mut world, menu, UiNode::sized(100.0, 20.0));
        let third = child(&mut world, menu, UiNode::sized(100.0, 30.0));

        layout_ui(&mut world, 400.0, 400.0);

        assert!(rect(&world, first).top < rect(&world, second).top);
        assert!(rect(&world, second).top < rect(&world, third).top);
        // And the heights are theirs, so this is not passing by accident of equal sizes.
        assert_eq!(rect(&world, third).height, 30.0);
    }

    #[test]
    fn growing_children_share_what_is_left_in_proportion() {
        let (mut world, root) = screen();
        let bar = child(
            &mut world,
            root,
            UiNode {
                anchor: Anchor::fill(),
                ..UiNode::row(0.0)
            },
        );
        let fixed = child(&mut world, bar, UiNode::sized(100.0, 20.0));
        let one = child(
            &mut world,
            bar,
            UiNode {
                grow: 1.0,
                ..UiNode::sized(0.0, 20.0)
            },
        );
        let two = child(
            &mut world,
            bar,
            UiNode {
                grow: 2.0,
                ..UiNode::sized(0.0, 20.0)
            },
        );

        layout_ui(&mut world, 700.0, 100.0);

        // 700 total, 100 taken by the fixed child, 600 to share one-to-two.
        assert_eq!(rect(&world, fixed).width, 100.0);
        assert_eq!(rect(&world, one).width, 200.0);
        assert_eq!(rect(&world, two).width, 400.0);
        // And they are laid end to end, in order.
        assert_eq!(rect(&world, one).left, 100.0);
        assert_eq!(rect(&world, two).left, 300.0);
    }

    #[test]
    fn children_that_ask_for_more_than_there_is_overflow_rather_than_reversing() {
        // A negative share would place children in reverse, which looks like a layout bug in the
        // *authoring* rather than an overflow. Overflowing is visible and diagnosable.
        let (mut world, root) = screen();
        let row = child(
            &mut world,
            root,
            UiNode {
                anchor: Anchor::fill(),
                ..UiNode::row(0.0)
            },
        );
        let first = child(&mut world, row, UiNode::sized(400.0, 20.0));
        let second = child(&mut world, row, UiNode::sized(400.0, 20.0));

        layout_ui(&mut world, 500.0, 100.0);

        assert_eq!(rect(&world, first).left, 0.0);
        assert!(
            rect(&world, second).left > rect(&world, first).left,
            "order must survive an overflow"
        );
        assert_eq!(rect(&world, second).width, 400.0);
    }

    #[test]
    fn an_invisible_node_takes_no_space_and_lays_out_nothing_below_it() {
        // Both halves matter. A hidden menu that still reserved its space leaves a hole; a hidden
        // menu whose children were still laid out costs a subtree of work on every frame it is
        // hidden, which is most of them.
        let (mut world, root) = screen();
        let menu = child(&mut world, root, UiNode::column(0.0));
        let first = child(&mut world, menu, UiNode::sized(100.0, 40.0));
        let hidden = child(
            &mut world,
            menu,
            UiNode {
                visible: false,
                ..UiNode::sized(100.0, 40.0)
            },
        );
        let buried = child(&mut world, hidden, UiNode::sized(10.0, 10.0));
        let last = child(&mut world, menu, UiNode::sized(100.0, 40.0));

        layout_ui(&mut world, 400.0, 400.0);

        assert!(world.get::<ComputedRect>(hidden).is_none());
        assert!(
            world.get::<ComputedRect>(buried).is_none(),
            "a hidden node's children must not be laid out"
        );
        // The visible ones close up: 0 and 40, not 0 and 80.
        assert_eq!(rect(&world, first).top, 0.0);
        assert_eq!(rect(&world, last).top, 40.0);
    }

    #[test]
    fn stretching_across_a_column_is_what_a_menu_of_full_width_buttons_wants() {
        // The parent's `align_children` normally wins, because a column exists to line its children
        // up — but `Stretch` is a request about *size*, not position, so it is honoured.
        let (mut world, root) = screen();
        let menu = child(
            &mut world,
            root,
            UiNode {
                anchor: Anchor::fill(),
                padding: UiEdges::all(20.0),
                ..UiNode::column(0.0)
            },
        );
        let button = child(
            &mut world,
            menu,
            UiNode {
                anchor: Anchor::new(Align::Stretch, Align::Start),
                ..UiNode::sized(0.0, 40.0)
            },
        );

        layout_ui(&mut world, 500.0, 500.0);

        let button = rect(&world, button);
        assert_eq!(button.left, 20.0);
        assert_eq!(button.width, 460.0, "full width inside the padding");
        assert_eq!(button.height, 40.0, "the main axis still uses its size");
    }

    #[test]
    fn a_node_parented_to_something_that_is_not_ui_is_its_own_root() {
        // Attaching a nameplate to a gameplay entity is not a layout relationship. Treating it as
        // one would lay the nameplate out inside a rectangle that does not exist.
        let mut world = World::new();
        let creature = world.spawn();
        world.insert(creature, amadeo_transform::Transform::at(3.0, 4.0));

        let plate = world.spawn();
        world.insert(plate, Parent(creature));
        world.insert(plate, UiNode::full());

        layout_ui(&mut world, 640.0, 480.0);

        assert_eq!(rect(&world, plate).width, 640.0);
    }

    #[test]
    fn the_system_takes_its_size_from_the_renderer_and_skips_without_one() {
        // A headless run has no window and therefore no screen to lay out against. Guessing a size
        // would produce rectangles that describe nothing, and something downstream would believe
        // them.
        let (mut world, root) = screen();
        layout_ui_system(&mut world);
        assert!(
            world.get::<ComputedRect>(root).is_none(),
            "no renderer means no screen size, so nothing should be laid out"
        );

        world.insert_service(amadeo_render::Renderer::new(Box::new(
            amadeo_render::NullBackend::new(800, 600),
        )));
        layout_ui_system(&mut world);

        assert_eq!(rect(&world, root).width, 800.0);
        assert_eq!(rect(&world, root).height, 600.0);
    }

    #[test]
    fn layout_cannot_move_the_state_hash() {
        // **Sharper here than for most derived data**, because layout depends on the *window size*.
        // A game played at 1920x1080 and the same game at 1280x720 must be the same world, and they
        // would not be if where a button landed were state.
        let (mut world, root) = screen();
        child(&mut world, root, UiNode::sized(100.0, 40.0));

        let before = world.state_hash();
        layout_ui(&mut world, 1920.0, 1080.0);
        let wide = world.state_hash();
        layout_ui(&mut world, 1280.0, 720.0);

        assert_eq!(before, wide);
        assert_eq!(before, world.state_hash());
    }
}
