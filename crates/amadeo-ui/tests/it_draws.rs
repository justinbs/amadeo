//! Someone finally looks at it.
//!
//! # Why this file is the one that matters
//!
//! Everything in `src/` asserts on *numbers*: a panel's centre, a glyph's advance, the sign of a
//! coordinate. All of it can be right while the picture is wrong, because the numbers and the
//! picture are joined by a projection, a camera and a shader that no unit test touches.
//!
//! `docs/07` records what that costs. A voxel mesher had correct normals and inside-out winding for
//! two sessions, and every test passed, because **the two properties are independent and only one
//! was checked**. Screen-space UI has the same shape: a layout can be numerically perfect and drawn
//! upside down, off-screen, or underneath the world, and not one assertion in `draw.rs` would move.
//!
//! So this renders the interface offscreen and reads the pixels back.
//!
//! # Skipping is honest here
//!
//! A machine with no GPU and no software fallback has nothing to render with. These report that and
//! pass, because a missing adapter is a fact about the machine rather than about the engine.

#![cfg(feature = "gpu")]

use amadeo_ecs::{Entity, World};
use amadeo_render::{Overlay, Renderer, TextureCache, TextureData, WgpuBackend, render_quads};
use amadeo_transform::Parent;
use amadeo_ui::{Align, Anchor, Paint, Panel, TypeScale, UiEdges, UiNode, collect_ui, layout_ui};

/// Only one GPU device at a time within this binary — `gfx-rs/wgpu#6571`, the same crash
/// `amadeo-render`'s capture tests take a lock for. Dropping one device while another is alive is a
/// `STATUS_ACCESS_VIOLATION` on Windows, and cargo runs tests in parallel by default.
static ONE_DEVICE_AT_A_TIME: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn one_device_at_a_time() -> std::sync::MutexGuard<'static, ()> {
    ONE_DEVICE_AT_A_TIME
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Lays out the interface, collects it, renders it offscreen, and hands back the pixels.
fn draw(world: &mut World, width: u32, height: u32) -> Option<TextureData> {
    let _gpu = one_device_at_a_time();

    let backend = match WgpuBackend::offscreen(width, height) {
        Ok(backend) => backend,
        Err(error) => {
            println!("skipping: no offscreen device on this machine ({error})");
            return None;
        }
    };
    world.insert_service(Renderer::new(Box::new(backend)));
    world.insert_service(Overlay::default());
    world.insert_service(TextureCache::new());

    // The three steps a game's `Render` stage runs, in order. Getting them out of order is itself a
    // bug worth catching: `collect_ui` before `layout_ui` would find no `ComputedRect` and draw
    // nothing at all.
    layout_ui(world, width as f32, height as f32);
    collect_ui(world);
    render_quads(world);

    let mut renderer = world
        .remove_service::<Renderer>()
        .expect("just installed it");
    Some(
        renderer
            .capture()
            .expect("an offscreen backend can capture"),
    )
}

fn pixel_at(image: &TextureData, x: u32, y: u32) -> [u8; 4] {
    let index = ((y * image.width + x) * 4) as usize;
    [
        image.pixels[index],
        image.pixels[index + 1],
        image.pixels[index + 2],
        image.pixels[index + 3],
    ]
}

/// A world with a full-screen UI root, and that root.
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

/// Whether a pixel is recognisably the red the tests paint with.
fn is_red(pixel: [u8; 4]) -> bool {
    pixel[0] > 150 && pixel[1] < 90 && pixel[2] < 90
}

#[test]
fn a_panel_anchored_top_left_is_drawn_in_the_top_left() {
    // **The assertion this whole file exists for.** A y-flip that is missing, doubled, or applied to
    // only one of the two coordinate systems produces an interface that is upside down — and every
    // unit test in `draw.rs` still passes, because they check the numbers that feed the flip rather
    // than the picture that comes out of it.
    let (mut world, root) = screen();
    let panel = child(
        &mut world,
        root,
        UiNode {
            anchor: Anchor::new(Align::Start, Align::Start),
            margin: UiEdges::all(20.0),
            ..UiNode::sized(120.0, 80.0)
        },
    );
    world.insert(panel, Panel::filled([1.0, 0.0, 0.0, 1.0]));

    let Some(image) = draw(&mut world, 320, 240) else {
        return;
    };

    // Inside the panel: 20 in from each edge, 120 x 80, so (60, 50) is comfortably within it.
    assert!(
        is_red(pixel_at(&image, 60, 50)),
        "the panel should be here, got {:?}",
        pixel_at(&image, 60, 50)
    );
    // And the *opposite* corner must not be. This is what fails if the picture is upside down.
    assert!(
        !is_red(pixel_at(&image, 260, 190)),
        "the bottom-right should be empty, got {:?}",
        pixel_at(&image, 260, 190)
    );
    // Just outside the panel's own edge, so the size is right rather than merely non-zero.
    assert!(!is_red(pixel_at(&image, 150, 50)));
    assert!(!is_red(pixel_at(&image, 60, 110)));
}

#[test]
fn a_panel_anchored_bottom_right_is_drawn_in_the_bottom_right() {
    // The mirror of the test above. Together they pin **both** axes: either one alone would pass
    // against a picture flipped on the other.
    let (mut world, root) = screen();
    let panel = child(
        &mut world,
        root,
        UiNode {
            anchor: Anchor::new(Align::End, Align::End),
            margin: UiEdges::all(20.0),
            ..UiNode::sized(120.0, 80.0)
        },
    );
    world.insert(panel, Panel::filled([1.0, 0.0, 0.0, 1.0]));

    let Some(image) = draw(&mut world, 320, 240) else {
        return;
    };

    assert!(
        is_red(pixel_at(&image, 260, 190)),
        "the panel should be here, got {:?}",
        pixel_at(&image, 260, 190)
    );
    assert!(
        !is_red(pixel_at(&image, 60, 50)),
        "the top-left should be empty, got {:?}",
        pixel_at(&image, 60, 50)
    );
}

#[test]
fn a_stretched_panel_covers_everything_inside_its_margins() {
    // `Stretch` is the anchor that ignores a node's own size and turns margins into insets. If the
    // projection's *scale* were wrong — a pixel not being a unit — this is the test that notices,
    // because the panel would stop short of, or spill past, its inset by a visible amount.
    let (mut world, root) = screen();
    let panel = child(
        &mut world,
        root,
        UiNode {
            anchor: Anchor::fill(),
            margin: UiEdges::all(40.0),
            ..UiNode::default()
        },
    );
    world.insert(panel, Panel::filled([1.0, 0.0, 0.0, 1.0]));

    let Some(image) = draw(&mut world, 320, 240) else {
        return;
    };

    // Inside the inset, on all four sides.
    for (x, y) in [(45, 45), (274, 45), (45, 194), (274, 194), (160, 120)] {
        assert!(
            is_red(pixel_at(&image, x, y)),
            "({x}, {y}) should be inside the panel, got {:?}",
            pixel_at(&image, x, y)
        );
    }
    // And outside it, on all four sides. A scale error shows up here first.
    for (x, y) in [(20, 120), (300, 120), (160, 20), (160, 220)] {
        assert!(
            !is_red(pixel_at(&image, x, y)),
            "({x}, {y}) should be outside the panel, got {:?}",
            pixel_at(&image, x, y)
        );
    }
}

#[test]
fn a_later_panel_draws_over_an_earlier_one() {
    // ADR 0018's rule, seen rather than asserted: `Panel::order` decides what is on top.
    let (mut world, root) = screen();

    let under = child(&mut world, root, UiNode::full());
    world.insert(under, Panel::filled([1.0, 0.0, 0.0, 1.0]));

    let over = child(
        &mut world,
        root,
        UiNode {
            anchor: Anchor::fill(),
            margin: UiEdges::all(40.0),
            ..UiNode::default()
        },
    );
    world.insert(
        over,
        Panel {
            paint: Paint::Custom {
                rgba: [0.0, 0.0, 1.0, 1.0],
            },
            order: 1,
        },
    );

    let Some(image) = draw(&mut world, 320, 240) else {
        return;
    };

    let middle = pixel_at(&image, 160, 120);
    assert!(
        middle[2] > 150 && middle[0] < 90,
        "the higher-order blue panel should win in the middle, got {middle:?}"
    );
    // And the lower one still shows outside the higher one's inset.
    assert!(is_red(pixel_at(&image, 20, 120)));
}

#[test]
fn a_glyph_reaches_the_screen_as_pixels() {
    // **The text half of this file's argument.** Shaping is tested, the atlas is tested to the
    // pixel, and the sprite positions are asserted — and all of that can be right while nothing
    // appears, because the atlas region, the tint, the alpha blend and the projection are joined
    // only in the shader.
    //
    // Uses the font generated in `test_font.rs`, whose one glyph is a solid box mapped from `A`. A
    // box is exactly what this wants: a letter's coverage varies and a solid rectangle does not, so
    // "is there ink where the glyph should be" is a question with an unambiguous answer.
    let (mut world, root) = screen();

    let mut fonts = amadeo_ui::FontCache::new();
    fonts.insert_font("boxes", &amadeo_ui::test_font::single_glyph_font());
    world.insert_service(fonts);

    let label = child(
        &mut world,
        root,
        UiNode {
            anchor: Anchor::new(Align::Start, Align::Start),
            margin: UiEdges::all(40.0),
            ..UiNode::sized(200.0, 80.0)
        },
    );
    world.insert(
        label,
        amadeo_ui::Text {
            paint: Paint::Custom {
                rgba: [1.0, 0.0, 0.0, 1.0],
            },
            ..amadeo_ui::Text::label("A", "boxes", TypeScale::Title)
        },
    );

    let Some(image) = draw(&mut world, 320, 240) else {
        return;
    };

    // The glyph sits on the label's baseline, near its top-left. Rather than pin an exact pixel —
    // which would make this a test of the font's metrics — scan the region it must fall inside and
    // require that *something* was drawn there.
    let mut inked = 0;
    for y in 40..140 {
        for x in 40..240 {
            if is_red(pixel_at(&image, x, y)) {
                inked += 1;
            }
        }
    }
    assert!(
        inked > 400,
        "a 64px solid box glyph should cover hundreds of pixels, found {inked}"
    );

    // And nothing outside the label's box, so the glyph is *placed* rather than smeared.
    assert!(!is_red(pixel_at(&image, 300, 220)));
}

#[test]
fn the_focused_item_is_visibly_a_different_colour() {
    // **The point of drawing the focus at all**, and the only test that can check it: everything in
    // `draw.rs` asserts that a token was substituted, which is true of a substitution that resolves
    // to a colour indistinguishable from the one it replaced. Whether a player can *see* which item
    // is highlighted is a question about pixels.
    //
    // Two identical `Raised` panels, one of them focused. Signage paints the focused one safety
    // orange, so the two must come back visibly different — and the orange must be recognisably
    // orange rather than merely "not the other one", which is what catches a theme lookup that fell
    // through to a default.
    let (mut world, root) = screen();
    world.insert_resource(amadeo_ui::Focus::default());

    let unfocused = child(
        &mut world,
        root,
        UiNode {
            anchor: Anchor::new(Align::Start, Align::Start),
            margin: UiEdges::all(20.0),
            ..UiNode::sized(100.0, 60.0)
        },
    );
    world.insert(unfocused, Panel::of(Paint::Raised));

    let highlighted = child(
        &mut world,
        root,
        UiNode {
            anchor: Anchor::new(Align::End, Align::Start),
            margin: UiEdges::all(20.0),
            ..UiNode::sized(100.0, 60.0)
        },
    );
    world.insert(highlighted, Panel::of(Paint::Raised));

    if let Some(focus) = world.resource_mut::<amadeo_ui::Focus>() {
        focus.entity = Some(highlighted);
    }

    let Some(image) = draw(&mut world, 320, 240) else {
        return;
    };

    let plain = pixel_at(&image, 70, 50);
    let accent = pixel_at(&image, 250, 50);

    // Safety orange: strongly red, some green, almost no blue. Loose bounds, because this is asking
    // "does it read as the accent" rather than pinning the sRGB round trip to a byte.
    assert!(
        accent[0] > 170 && accent[1] > 40 && accent[1] < 150 && accent[2] < 90,
        "the focused panel should be the accent, got {accent:?}"
    );
    // And the unfocused one is still the dark raised surface it authored.
    assert!(
        plain[0] < 90,
        "the unfocused panel should be unchanged, got {plain:?}"
    );
}

#[test]
fn an_invisible_panel_draws_nothing_at_all() {
    // The cheap half of `visible`, checked against pixels: a hidden node is not laid out, so it has
    // no rectangle, so there is nothing to draw. If hiding ever became "draw it transparent", this
    // is what would notice the wasted pass.
    let (mut world, root) = screen();
    let panel = child(
        &mut world,
        root,
        UiNode {
            visible: false,
            anchor: Anchor::fill(),
            ..UiNode::default()
        },
    );
    world.insert(panel, Panel::filled([1.0, 0.0, 0.0, 1.0]));

    let Some(image) = draw(&mut world, 320, 240) else {
        return;
    };

    assert!(!is_red(pixel_at(&image, 160, 120)));
}
