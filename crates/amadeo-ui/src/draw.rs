//! Turning laid-out nodes into something the renderer draws — ADR 0062's last mile.
//!
//! # Screen space meets world space, and this is the only place they touch
//!
//! Layout works in **screen pixels**: origin top-left, +Y down. The sprite path draws in **world
//! units** through a camera: +Y up. Somewhere those have to meet, and the whole point of confining it
//! to this file is that a flip applied twice, or in half the cases, produces a layout that is
//! plausible and upside down.
//!
//! The conversion is one line — `world = [screen_x, -screen_y]` — made to work by giving the UI its
//! own orthographic camera whose visible region is exactly `x ∈ 0..width`, `y ∈ -height..0`. A pixel
//! is a unit, so no scaling is involved anywhere and a node's size *is* its size.
//!
//! # The UI camera is machinery, not content
//!
//! It is synthesised here rather than authored in a scene file. A game does not choose whether its
//! interface is drawn in screen space, and an authored UI camera would be a thing every game had to
//! remember, could get wrong, and would have to update on every resize. ADR 0031 made a camera an
//! entity so that *games* could have several; this is the engine using the same `View` shape for its
//! own pass, which costs the renderer nothing new.
//!
//! It draws at [`UI_ORDER`], well above anything a game is likely to use, and ADR 0018's rule still
//! decides what is on top *within* the interface.
//!
//! # The focus is drawn here, and that placement is the decision
//!
//! [`Focus`] is a hashed resource — where the highlight sits is gameplay (ADR 0063). **What the
//! highlight looks like is not**, and the two must not be confused: repainting a [`Panel`] component
//! would write the theme into the state hash and make two players with different looks simulate
//! differently. So the substitution happens at collection, on the way to a `View`, where nothing it
//! touches can be hashed.
//!
//! The rule is [`FOCUS_PANEL`] and [`FOCUS_TEXT`], and it comes from the palette rather than from
//! taste: ADR 0064's `Accent` is documented as "focus, selection, and the one thing on screen asking
//! to be looked at", so a theme has already said what a focused thing should be painted. A menu
//! authored with no knowledge of focus therefore highlights correctly, which is the property worth
//! having — the alternative is a per-widget opt-in that is silent when forgotten.

use crate::components::{ComputedRect, UiNode};
use crate::focus::Focus;
use crate::text::FontCache;
use crate::theme::{Paint, Theme};
use crate::{GLYPH_ATLAS_ID, Panel, Text};
use amadeo_ecs::{Entity, World};
use amadeo_render::{
    Camera, Overlay, Projection, QuadInstance, SpriteBatch, SpriteInstance, TextureCache, View,
};
use amadeo_transform::Parent;

/// The camera order the interface draws at.
///
/// High enough to sit over anything a game would author without having to coordinate, and an
/// ordinary number rather than `i32::MAX` so a game *can* deliberately put something above it — a
/// transition wipe, a debug overlay — without reaching for a special case.
pub const UI_ORDER: i32 = 1000;

/// What a [`Panel`] is painted with while it, or something above it, has the focus.
///
/// Not configurable, and deliberately so for now: the palette already assigns this meaning to
/// `Accent`, so a second knob would be a second place for the answer to live. A per-widget override
/// is additive — a component the draw pass consults instead of this constant — and nothing authored
/// today would change if one were added.
pub const FOCUS_PANEL: Paint = Paint::Accent;

/// What [`Text`] is painted with while it sits inside the focused node.
///
/// The pair of [`FOCUS_PANEL`], because ink that stayed `Ink` on an accent fill would be the one
/// unreadable thing on the screen.
pub const FOCUS_TEXT: Paint = Paint::OnAccent;

/// Reads laid-out nodes and hands the renderer a view of the interface.
///
/// Registered in the `Render` stage after [`layout_ui_system`](crate::layout_ui_system), which is
/// what puts a [`ComputedRect`] on everything. Does nothing when no [`Overlay`] service is installed,
/// which is the headless case.
///
/// # Nothing here spawns an entity, deliberately
///
/// The obvious implementation gives each glyph an entity with a `Sprite` and lets the existing
/// collection pass find it. That would be **catastrophic**: entities are simulation state, so a
/// paragraph of text would move the state hash, and it would move it differently at two window
/// sizes. Draw data goes straight into a `View` instead, which is a `Service` and outside the hash
/// by trait bound (ADR 0009).
pub fn collect_ui(world: &mut World) {
    if !world.has_service::<Overlay>() {
        return;
    }

    let (width, height) = match world.service::<amadeo_render::Renderer>() {
        Some(renderer) => renderer.viewport(),
        // No renderer means no screen, so there is nothing to lay an interface out against. The same
        // rule `layout_ui_system` follows, and for the same reason.
        None => return,
    };
    let (width, height) = (width as f32, height as f32);

    // The theme, or the built-in default. Copied once for the same reason layout copies it: it is
    // small, and holding a borrow would fight the world borrows below.
    let theme = world.service::<Theme>().cloned().unwrap_or_default();

    // Where the highlight sits. `None` in a game with no menu, and in every headless run — the
    // resource is one a game installs, not one this crate requires.
    let focused = world.resource::<Focus>().and_then(|focus| focus.entity);

    let quads = collect_panels(world, &theme, focused);
    let batches = collect_text(world, &theme, focused);

    if quads.is_empty() && batches.is_empty() {
        // Nothing to draw. An empty view would still cost the backend a pass and a clear.
        return;
    }

    let view = View {
        camera: Camera {
            active: true,
            order: UI_ORDER,
            // A pixel is a unit: the visible height *is* the screen height, so nothing scales.
            projection: Projection::Orthographic { height },
            target: String::new(),
            viewport: [0.0, 0.0, 1.0, 1.0],
            environment: String::new(),
        },
        // Centred so the visible region is exactly `x ∈ 0..width`, `y ∈ -height..0`.
        eye: [width * 0.5, -height * 0.5],
        eye_matrix: amadeo_transform::Mat4::from_transform(
            [width * 0.5, -height * 0.5, 0.0],
            [0.0; 3],
            [1.0; 3],
        ),
        environment: amadeo_render::Environment::default(),
        quads,
        batches,
        meshes: Vec::new(),
        shadow_casters: Vec::new(),
        lights: Vec::new(),
        punctual: Vec::new(),
    };

    if let Some(overlay) = world.service_mut::<Overlay>() {
        overlay.views.push(view);
    }
}

/// Whether a node is really on screen, and whether the focus is on it or above it.
///
/// # Two answers, because they come from the same walk
///
/// Both questions are about a node's **ancestors**, so asking them separately would walk the tree
/// twice to learn one thing.
///
/// # The visibility half is not the same as `node.visible`
///
/// Layout skips a hidden node *and its descendants*, which means it never overwrites the
/// [`ComputedRect`] those descendants were given the last time they were shown. Hiding a menu root
/// therefore leaves stale rectangles all the way down, and a draw pass that only checked each node's
/// own `visible` flag would happily draw the buttons of a closed menu — which is precisely what a
/// pause menu does on every keypress.
///
/// Removing the rectangle instead would be a structural change on every toggle, and avoiding exactly
/// that is why `visible` is a field rather than a despawn. So the check belongs here.
fn ancestry(world: &World, entity: Entity, focused: Option<Entity>) -> (bool, bool) {
    let mut current = entity;
    let mut highlighted = false;

    for _ in 0..crate::layout::MAX_DEPTH {
        let Some(node) = world.get::<UiNode>(current) else {
            // Off the top of the interface. A UI node whose parent is not a UI node is a root —
            // layout says so — and a gameplay entity cannot hide one.
            return (true, highlighted);
        };
        if !node.visible {
            return (false, highlighted);
        }
        if Some(current) == focused {
            highlighted = true;
        }
        match world.get::<Parent>(current) {
            Some(parent) => current = parent.0,
            None => return (true, highlighted),
        }
    }

    // Deeper than layout is willing to walk, so this node has no rectangle worth believing either.
    (false, highlighted)
}

/// Every visible panel, as a quad, in draw order.
fn collect_panels(world: &World, theme: &Theme, focused: Option<Entity>) -> Vec<QuadInstance> {
    let mut panels: Vec<(i32, Entity, QuadInstance)> = world
        .query::<(&Panel, &UiNode, &ComputedRect)>()
        .filter_map(|(entity, (panel, _, rect))| {
            let (shown, highlighted) = ancestry(world, entity, focused);
            let paint = if highlighted {
                FOCUS_PANEL
            } else {
                panel.paint
            };
            shown.then(|| (entity, theme.paint(paint), panel.order, *rect))
        })
        // Resolved *before* the transparency check, because whether a panel is invisible is a
        // property of the colour the theme gave it, not of the token that asked for one.
        .filter(|(_, colour, _, _)| colour[3] > 0.0)
        .map(|(entity, colour, order, rect)| {
            let centre = rect.centre();
            (
                order,
                entity,
                QuadInstance {
                    center: [centre[0], -centre[1]],
                    size: [rect.width, rect.height],
                    rotation: 0.0,
                    color: colour,
                },
            )
        })
        .collect();

    // By order, then by entity — which is authored order, so two panels at the same order stack the
    // way the scene file lists them rather than the way the storage happens to iterate.
    panels.sort_by_key(|(order, entity, _)| (*order, entity.index(), entity.generation()));
    panels.into_iter().map(|(_, _, quad)| quad).collect()
}

/// Every visible label, as glyph sprites batched onto the atlas.
///
/// Needs the [`FontCache`] mutably — shaping rasterises — and the world shared, so the cache is taken
/// out for the duration. The same shape `decode_frame_textures` uses one crate down.
fn collect_text(world: &mut World, theme: &Theme, focused: Option<Entity>) -> Vec<SpriteBatch> {
    if !world.has_service::<FontCache>() {
        return Vec::new();
    }

    let mut instances: Vec<(i32, Entity, SpriteInstance)> = Vec::new();

    world.with_service_taken::<FontCache, ()>(|world, fonts| {
        // The paint is resolved here rather than read off the component, so a label inside the
        // focused node draws in `OnAccent` without anything having written that anywhere hashable.
        let labels: Vec<(Entity, Text, Paint, ComputedRect)> = world
            .query::<(&Text, &UiNode, &ComputedRect)>()
            .filter(|(_, (text, _, _))| !text.content.is_empty())
            .filter_map(|(entity, (text, _, rect))| {
                let (shown, highlighted) = ancestry(world, entity, focused);
                let paint = if highlighted { FOCUS_TEXT } else { text.paint };
                shown.then(|| (entity, text.clone(), paint, *rect))
            })
            .filter(|(_, _, paint, _)| theme.paint(*paint)[3] > 0.0)
            .collect();

        // Load every font a label names, before anything tries to shape with one. The same step
        // `decode_frame_textures` and `ensure_sounds` perform for their own asset kind, and cheap to
        // repeat: an id already loaded, or already failed, returns immediately.
        if let Some(assets) = world.service::<amadeo_assets::Assets>() {
            for (_, text, _, _) in &labels {
                fonts.ensure(&text.font, assets);
            }
        }

        for (entity, text, paint, rect) in labels {
            // Wrapped to the node's width, so a label in a narrow panel breaks rather than spilling.
            let wrap = if text.wrap { Some(rect.width) } else { None };
            // The tokens become numbers here, and nowhere else.
            let step = theme.scale(text.scale);
            let colour = theme.paint(paint);
            let shaped = fonts.shape(&text.content, &text.font, step.size, step.line_height, wrap);

            for glyph in &shaped.glyphs {
                let Some(image) = glyph.image else {
                    continue;
                };

                // The glyph's top-left in screen pixels. `image.top` measures **up** from the
                // baseline — fonts do — which is why it is subtracted rather than added.
                let left = rect.left + glyph.left + image.left;
                let top = rect.top + glyph.baseline - image.top;

                instances.push((
                    text.order,
                    entity,
                    SpriteInstance {
                        center: [left + image.width * 0.5, -(top + image.height * 0.5)],
                        axes: [[image.width, 0.0], [0.0, image.height]],
                        color: colour,
                        region: image.region,
                    },
                ));
            }
        }

        // The atlas may have gained glyphs just now, so it is republished before anything draws with
        // it. Cheap when nothing changed: `insert_decoded` replaces one map entry.
        if let Some(textures) = world.service_mut::<TextureCache>() {
            textures.insert_decoded(GLYPH_ATLAS_ID, fonts.atlas().texture().clone());
        }
    });

    instances.sort_by_key(|(order, entity, _)| (*order, entity.index(), entity.generation()));

    // **One batch**, because every glyph in the game shares one atlas — which is the whole argument
    // for having an atlas (ADR 0023 batches on texture). A page of text is one draw call.
    let mut batches: Vec<SpriteBatch> = Vec::new();
    for (order, _, instance) in instances {
        match batches.last_mut() {
            Some(batch) if batch.order == order => batch.instances.push(instance),
            _ => batches.push(SpriteBatch {
                texture: GLYPH_ATLAS_ID.to_string(),
                order,
                instances: vec![instance],
            }),
        }
    }
    batches
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Anchor, Paint, TypeScale, UiEdges, layout_ui};
    use amadeo_render::{NullBackend, Renderer};
    use amadeo_transform::Parent;

    /// A world with a renderer, an overlay slot, and a full-screen root.
    fn world_with_screen(width: u32, height: u32) -> (World, Entity) {
        let mut world = World::new();
        world.insert_service(Renderer::new(Box::new(NullBackend::new(width, height))));
        world.insert_service(Overlay::default());
        world.insert_service(TextureCache::new());

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

    /// Lays out and collects, returning the interface's view.
    fn drawn(world: &mut World, width: f32, height: f32) -> Option<View> {
        layout_ui(world, width, height);
        collect_ui(world);
        world
            .service_mut::<Overlay>()
            .expect("installed")
            .views
            .pop()
    }

    #[test]
    fn a_panel_lands_where_layout_put_it_with_y_flipped_once() {
        // **The assertion this whole file exists for.** Layout is +Y down from the top-left; the
        // sprite path is +Y up. A flip applied twice, or in half the cases, gives a layout that is
        // plausible and upside down — which looks like a layout bug rather than a conversion one.
        let (mut world, root) = world_with_screen(800, 600);
        let panel = child(
            &mut world,
            root,
            UiNode {
                anchor: Anchor::new(crate::Align::Start, crate::Align::Start),
                margin: UiEdges::all(10.0),
                ..UiNode::sized(100.0, 50.0)
            },
        );
        world.insert(panel, Panel::filled([1.0, 0.0, 0.0, 1.0]));

        let view = drawn(&mut world, 800.0, 600.0).expect("something to draw");
        assert_eq!(view.quads.len(), 1);

        // Screen centre is (10 + 50, 10 + 25) = (60, 35). World y is the negation.
        assert_eq!(view.quads[0].center, [60.0, -35.0]);
        assert_eq!(view.quads[0].size, [100.0, 50.0]);
    }

    #[test]
    fn the_ui_camera_shows_exactly_the_screen() {
        // A pixel is a unit. If the projection height or the eye were wrong, every position above
        // would still be "right" and everything would be drawn at the wrong scale or off-screen.
        let (mut world, root) = world_with_screen(800, 600);
        let panel = child(&mut world, root, UiNode::sized(10.0, 10.0));
        world.insert(panel, Panel::default());

        let view = drawn(&mut world, 800.0, 600.0).expect("something to draw");

        assert_eq!(view.camera.order, UI_ORDER);
        assert!(view.camera.active);
        match view.camera.projection {
            Projection::Orthographic { height } => assert_eq!(height, 600.0),
            other => panic!("the interface needs an orthographic camera, got {other:?}"),
        }
        // Centred so `x ∈ 0..800` and `y ∈ -600..0` are exactly visible.
        assert_eq!(view.eye, [400.0, -300.0]);
    }

    #[test]
    fn a_bottom_right_panel_has_a_more_negative_y_than_a_top_left_one() {
        // The direction check, independent of the arithmetic above: further down the screen must be
        // further *negative* in world space. This is what catches a flip that was forgotten rather
        // than one that was miscomputed.
        let (mut world, root) = world_with_screen(400, 400);
        let top = child(&mut world, root, UiNode::sized(20.0, 20.0));
        world.insert(top, Panel::default());
        let bottom = child(
            &mut world,
            root,
            UiNode {
                anchor: Anchor::new(crate::Align::End, crate::Align::End),
                ..UiNode::sized(20.0, 20.0)
            },
        );
        world.insert(
            bottom,
            Panel {
                order: 1,
                ..Panel::default()
            },
        );

        let view = drawn(&mut world, 400.0, 400.0).expect("something to draw");
        assert_eq!(view.quads.len(), 2);
        assert!(
            view.quads[1].center[1] < view.quads[0].center[1],
            "the lower panel should be further negative: {:?}",
            view.quads
        );
    }

    #[test]
    fn an_invisible_or_transparent_panel_is_not_drawn() {
        // Both skipped before they reach a batch. A fully transparent quad costs a draw and changes
        // nothing, and a hidden node has no `ComputedRect` at all.
        let (mut world, root) = world_with_screen(400, 400);

        let hidden = child(
            &mut world,
            root,
            UiNode {
                visible: false,
                ..UiNode::sized(20.0, 20.0)
            },
        );
        world.insert(hidden, Panel::default());

        let clear = child(&mut world, root, UiNode::sized(20.0, 20.0));
        world.insert(clear, Panel::filled([1.0, 1.0, 1.0, 0.0]));

        assert!(
            drawn(&mut world, 400.0, 400.0).is_none(),
            "nothing visible means no view at all, not an empty one"
        );
    }

    #[test]
    fn panels_draw_in_order_then_in_authored_order() {
        let (mut world, root) = world_with_screen(400, 400);
        let late = child(&mut world, root, UiNode::sized(10.0, 10.0));
        world.insert(
            late,
            Panel {
                order: 5,
                paint: Paint::Custom {
                    rgba: [1.0, 0.0, 0.0, 1.0],
                },
            },
        );
        let early = child(&mut world, root, UiNode::sized(10.0, 10.0));
        world.insert(
            early,
            Panel {
                order: 0,
                paint: Paint::Custom {
                    rgba: [0.0, 1.0, 0.0, 1.0],
                },
            },
        );

        let view = drawn(&mut world, 400.0, 400.0).expect("something to draw");
        // Spawned late-then-early, drawn early-then-late.
        assert_eq!(view.quads[0].color, [0.0, 1.0, 0.0, 1.0]);
        assert_eq!(view.quads[1].color, [1.0, 0.0, 0.0, 1.0]);
    }

    #[test]
    fn text_becomes_one_batch_of_glyph_sprites_on_the_atlas() {
        let (mut world, root) = world_with_screen(800, 600);
        let mut fonts = FontCache::new();
        fonts.insert_font("test", &crate::test_font::single_glyph_font());
        world.insert_service(fonts);

        let label = child(&mut world, root, UiNode::sized(200.0, 40.0));
        world.insert(label, Text::label("AAA", "test", TypeScale::Heading));

        let view = drawn(&mut world, 800.0, 600.0).expect("something to draw");

        assert_eq!(view.batches.len(), 1, "one atlas means one batch");
        assert_eq!(view.batches[0].texture, GLYPH_ATLAS_ID);
        assert_eq!(view.batches[0].instances.len(), 3);

        // Glyphs advance to the right, and each is a sensible size rather than zero.
        let first = &view.batches[0].instances[0];
        let second = &view.batches[0].instances[1];
        assert!(second.center[0] > first.center[0]);
        assert!(first.axes[0][0] > 1.0 && first.axes[1][1] > 1.0);
        // The colour is the tint, since the atlas holds white coverage — and it comes from the
        // theme's `Ink`, which is what `Text` defaults to, rather than from a literal.
        assert_eq!(first.color, Theme::default().paint(Paint::Ink));
    }

    #[test]
    fn drawing_text_publishes_the_atlas_so_the_batch_has_something_to_sample() {
        // **The batch names `GLYPH_ATLAS_ID` whether or not anything ever put a texture there**, so
        // without this the text would draw as the magenta placeholder — visible, wrong, and easy to
        // misread as a broken font rather than a missing publish.
        let (mut world, root) = world_with_screen(800, 600);
        let mut fonts = FontCache::new();
        fonts.insert_font("test", &crate::test_font::single_glyph_font());
        world.insert_service(fonts);

        let label = child(&mut world, root, UiNode::sized(200.0, 40.0));
        world.insert(label, Text::label("A", "test", TypeScale::Heading));

        drawn(&mut world, 800.0, 600.0).expect("something to draw");

        assert!(
            world
                .service::<TextureCache>()
                .expect("installed")
                .is_decoded(GLYPH_ATLAS_ID),
            "the glyph atlas has to reach the texture cache"
        );
    }

    #[test]
    fn a_label_in_a_missing_font_draws_nothing_rather_than_something_wrong() {
        let (mut world, root) = world_with_screen(800, 600);
        world.insert_service(FontCache::new());

        let label = child(&mut world, root, UiNode::sized(200.0, 40.0));
        world.insert(
            label,
            Text::label("hello", "not_installed", TypeScale::Heading),
        );

        assert!(drawn(&mut world, 800.0, 600.0).is_none());
    }

    #[test]
    fn hiding_a_menu_hides_the_buttons_inside_it() {
        // **The bug this was written to catch, found while wiring the focus up.** Layout skips a
        // hidden node *and its descendants*, so it never overwrites the rectangles those descendants
        // were given while the menu was open. A draw pass that only checked each node's own
        // `visible` flag would keep drawing them off stale rectangles — a closed pause menu still on
        // screen, which is the first thing a pause menu does.
        let (mut world, root) = world_with_screen(400, 400);
        let menu = child(&mut world, root, UiNode::full());
        let button = child(&mut world, menu, UiNode::sized(100.0, 40.0));
        world.insert(button, Panel::default());

        assert!(
            drawn(&mut world, 400.0, 400.0).is_some(),
            "the menu is open, so the button should draw"
        );

        // Close it. The button's own node is untouched, and its rectangle is still there.
        world.insert(
            menu,
            UiNode {
                visible: false,
                ..UiNode::full()
            },
        );
        assert!(
            world.get::<ComputedRect>(button).is_some(),
            "the stale rectangle is the whole hazard — if this ever goes away, so does the test"
        );

        assert!(
            drawn(&mut world, 400.0, 400.0).is_none(),
            "a button inside a hidden menu must not draw"
        );
    }

    #[test]
    fn the_focused_node_and_its_text_are_repainted() {
        // A menu item is a panel with a label inside it, so the highlight has to reach a *child*.
        // Repainting only the focused entity gives an orange button with ink-coloured text on it,
        // which is the one unreadable combination in the palette.
        let (mut world, root) = world_with_screen(800, 600);
        let mut fonts = FontCache::new();
        fonts.insert_font("test", &crate::test_font::single_glyph_font());
        world.insert_service(fonts);
        world.insert_resource(crate::Focus::default());

        let button = child(&mut world, root, UiNode::sized(200.0, 40.0));
        world.insert(button, Panel::of(Paint::Raised));
        let label = child(&mut world, button, UiNode::sized(180.0, 30.0));
        world.insert(label, Text::label("A", "test", TypeScale::Body));

        let theme = Theme::default();

        // Nothing focused: both draw what they authored.
        let view = drawn(&mut world, 800.0, 600.0).expect("something to draw");
        assert_eq!(view.quads[0].color, theme.paint(Paint::Raised));
        assert_eq!(view.batches[0].instances[0].color, theme.paint(Paint::Ink));

        if let Some(focus) = world.resource_mut::<crate::Focus>() {
            focus.entity = Some(button);
        }

        let view = drawn(&mut world, 800.0, 600.0).expect("something to draw");
        assert_eq!(view.quads[0].color, theme.paint(FOCUS_PANEL));
        assert_eq!(
            view.batches[0].instances[0].color,
            theme.paint(FOCUS_TEXT),
            "the label is a child of the focused node, so it is inside the highlight"
        );
    }

    #[test]
    fn only_the_focused_item_is_repainted() {
        // The other half: a highlight that reached everything would be a menu with no selection at
        // all, which looks like the focus never moving.
        let (mut world, root) = world_with_screen(400, 400);
        world.insert_resource(crate::Focus::default());

        let menu = child(&mut world, root, UiNode::column(crate::Spacing::None));
        let first = child(&mut world, menu, UiNode::sized(100.0, 40.0));
        world.insert(first, Panel::of(Paint::Raised));
        let second = child(&mut world, menu, UiNode::sized(100.0, 40.0));
        world.insert(second, Panel::of(Paint::Raised));

        if let Some(focus) = world.resource_mut::<crate::Focus>() {
            focus.entity = Some(second);
        }

        let view = drawn(&mut world, 400.0, 400.0).expect("something to draw");
        let theme = Theme::default();
        assert_eq!(view.quads[0].color, theme.paint(Paint::Raised));
        assert_eq!(view.quads[1].color, theme.paint(FOCUS_PANEL));
        // And the parent of the focused node is not swept up in it — the walk goes one way.
        assert_ne!(
            theme.paint(Paint::Raised),
            theme.paint(FOCUS_PANEL),
            "the test is vacuous if these two tokens ever resolve to the same colour"
        );
    }

    #[test]
    fn the_highlight_cannot_move_the_state_hash() {
        // **The reason the substitution lives in the draw pass at all** (ADR 0063). `Focus` is
        // hashed and *where* the highlight sits is gameplay; what it looks like is not, and writing
        // an appearance into a `Panel` would put the theme into the state hash — so two players with
        // different looks would simulate differently.
        let (mut world, root) = world_with_screen(400, 400);
        world.insert_resource(crate::Focus::default());
        let button = child(&mut world, root, UiNode::sized(100.0, 40.0));
        world.insert(button, Panel::of(Paint::Raised));

        layout_ui(&mut world, 400.0, 400.0);
        if let Some(focus) = world.resource_mut::<crate::Focus>() {
            focus.entity = Some(button);
        }
        let before = world.state_hash();
        collect_ui(&mut world);
        collect_ui(&mut world);
        assert_eq!(before, world.state_hash());
    }

    #[test]
    fn collecting_the_interface_cannot_move_the_state_hash() {
        // Sharper here than usual: what this produces depends on the **window size**, and it would
        // be catastrophic for a game to hash differently at two resolutions. Nothing is spawned and
        // everything written is a service.
        let (mut world, root) = world_with_screen(800, 600);
        let panel = child(&mut world, root, UiNode::sized(10.0, 10.0));
        world.insert(panel, Panel::default());

        layout_ui(&mut world, 800.0, 600.0);
        let before = world.state_hash();
        collect_ui(&mut world);
        collect_ui(&mut world);
        assert_eq!(before, world.state_hash());
    }
}
