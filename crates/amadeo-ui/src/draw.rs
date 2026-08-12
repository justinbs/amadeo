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

use crate::components::{ComputedRect, UiNode};
use crate::text::FontCache;
use crate::{GLYPH_ATLAS_ID, Panel, Text};
use amadeo_ecs::{Entity, World};
use amadeo_render::{
    Camera, Overlay, Projection, QuadInstance, SpriteBatch, SpriteInstance, TextureCache, View,
};

/// The camera order the interface draws at.
///
/// High enough to sit over anything a game would author without having to coordinate, and an
/// ordinary number rather than `i32::MAX` so a game *can* deliberately put something above it — a
/// transition wipe, a debug overlay — without reaching for a special case.
pub const UI_ORDER: i32 = 1000;

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

    let quads = collect_panels(world);
    let batches = collect_text(world);

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

/// Every visible panel, as a quad, in draw order.
fn collect_panels(world: &World) -> Vec<QuadInstance> {
    let mut panels: Vec<(i32, Entity, QuadInstance)> = world
        .query::<(&Panel, &UiNode, &ComputedRect)>()
        .filter(|(_, (_, node, _))| node.visible)
        .filter(|(_, (panel, _, _))| panel.colour[3] > 0.0)
        .map(|(entity, (panel, _, rect))| {
            let centre = rect.centre();
            (
                panel.order,
                entity,
                QuadInstance {
                    center: [centre[0], -centre[1]],
                    size: [rect.width, rect.height],
                    rotation: 0.0,
                    color: panel.colour,
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
fn collect_text(world: &mut World) -> Vec<SpriteBatch> {
    if !world.has_service::<FontCache>() {
        return Vec::new();
    }

    let mut instances: Vec<(i32, Entity, SpriteInstance)> = Vec::new();

    world.with_service_taken::<FontCache, ()>(|world, fonts| {
        let labels: Vec<(Entity, Text, ComputedRect)> = world
            .query::<(&Text, &UiNode, &ComputedRect)>()
            .filter(|(_, (_, node, _))| node.visible)
            .filter(|(_, (text, _, _))| !text.content.is_empty() && text.colour[3] > 0.0)
            .map(|(entity, (text, _, rect))| (entity, text.clone(), *rect))
            .collect();

        for (entity, text, rect) in labels {
            // Wrapped to the node's width, so a label in a narrow panel breaks rather than spilling.
            let wrap = if text.wrap { Some(rect.width) } else { None };
            let shaped = fonts.shape(&text.content, &text.font, text.size, text.line_height, wrap);

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
                        color: text.colour,
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
    use crate::{Anchor, UiEdges, layout_ui};
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
                colour: [1.0, 0.0, 0.0, 1.0],
            },
        );
        let early = child(&mut world, root, UiNode::sized(10.0, 10.0));
        world.insert(
            early,
            Panel {
                order: 0,
                colour: [0.0, 1.0, 0.0, 1.0],
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
        fonts.insert_font_for_test("test", &crate::test_font::single_glyph_font());
        world.insert_service(fonts);

        let label = child(&mut world, root, UiNode::sized(200.0, 40.0));
        world.insert(label, Text::label("AAA", "test", 24.0));

        let view = drawn(&mut world, 800.0, 600.0).expect("something to draw");

        assert_eq!(view.batches.len(), 1, "one atlas means one batch");
        assert_eq!(view.batches[0].texture, GLYPH_ATLAS_ID);
        assert_eq!(view.batches[0].instances.len(), 3);

        // Glyphs advance to the right, and each is a sensible size rather than zero.
        let first = &view.batches[0].instances[0];
        let second = &view.batches[0].instances[1];
        assert!(second.center[0] > first.center[0]);
        assert!(first.axes[0][0] > 1.0 && first.axes[1][1] > 1.0);
        // The colour is the tint, since the atlas holds white coverage.
        assert_eq!(first.color, [1.0, 1.0, 1.0, 1.0]);
    }

    #[test]
    fn drawing_text_publishes_the_atlas_so_the_batch_has_something_to_sample() {
        // **The batch names `GLYPH_ATLAS_ID` whether or not anything ever put a texture there**, so
        // without this the text would draw as the magenta placeholder — visible, wrong, and easy to
        // misread as a broken font rather than a missing publish.
        let (mut world, root) = world_with_screen(800, 600);
        let mut fonts = FontCache::new();
        fonts.insert_font_for_test("test", &crate::test_font::single_glyph_font());
        world.insert_service(fonts);

        let label = child(&mut world, root, UiNode::sized(200.0, 40.0));
        world.insert(label, Text::label("A", "test", 24.0));

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
        world.insert(label, Text::label("hello", "not_installed", 24.0));

        assert!(drawn(&mut world, 800.0, 600.0).is_none());
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
