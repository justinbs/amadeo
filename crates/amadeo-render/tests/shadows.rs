//! Where a shadow map covers, and when one exists at all — the collection half of ADR 0038.
//!
//! Headless. Fitting a shadow box is arithmetic, and arithmetic is checkable with no GPU — which is
//! invariant I7 paying for itself again: the box being in the wrong place is by far the most likely
//! way shadows go wrong, and it is catchable here rather than by looking at a picture and squinting.
//!
//! The pixels are checked in `capture.rs`, which is the only thing that can prove the shader
//! actually samples what this computes.

use amadeo_ecs::World;
use amadeo_render::{
    Camera, DirectionalLight, FrameData, NullBackend, Projection, Renderer, ShadowMode,
    render_quads,
};
use amadeo_transform::Transform;

/// A world with a 3D camera at `eye` and one light pointing straight down.
///
/// An unrotated light travels along its own negative Z, so the light entity is *rotated* to point
/// down rather than moved — a directional light has no position, which is the whole reason its
/// shadow box has to be fitted to something else.
fn sunlit_world(eye: [f32; 3], shadows: ShadowMode) -> World {
    let mut world = World::new();
    world.insert_service(Renderer::new(Box::new(NullBackend::new(64, 64))));

    let camera = world.spawn();
    world.insert(
        camera,
        Transform {
            translation: eye,
            ..Transform::default()
        },
    );
    world.insert(camera, Camera::perspective(60.0));

    let sun = world.spawn();
    // Pitched down 90 degrees, so its negative Z points at the ground.
    world.insert(
        sun,
        Transform {
            rotation: [-90.0, 0.0, 0.0],
            ..Transform::default()
        },
    );
    world.insert(
        sun,
        DirectionalLight {
            shadows,
            ..DirectionalLight::default()
        },
    );
    world
}

fn frame(world: &mut World) -> FrameData {
    render_quads(world);
    world
        .service::<Renderer>()
        .expect("installed")
        .null_backend()
        .expect("a null backend")
        .last_frame()
        .expect("a frame was drawn")
        .clone()
}

/// Where a world point lands in the shadow map, in 0..1 texture coordinates.
///
/// The same arithmetic `mesh.wgsl` does: project into the light's clip space, then map -1..1 across
/// into 0..1 with v running downward. Duplicated here on purpose — a test that called the shader's
/// version would agree with it by construction and prove nothing.
fn shadow_uv(frame: &FrameData, point: [f32; 3]) -> Option<[f32; 2]> {
    let shadow = frame.primary()?.lights.first()?.shadow?;
    // Cascade zero: the nearest, and the only one an `Orthogonal` light has.
    let clip = shadow.cascades[0].view_projection.project_point(point)?;
    Some([clip[0] * 0.5 + 0.5, clip[1] * -0.5 + 0.5])
}

#[test]
fn a_light_with_shadows_off_fits_no_shadow_box() {
    // The default, and it must stay cheap: no matrix, and therefore no shadow pass and no shadow
    // map allocated. `ShadowMode::Off` being the default is what keeps a game that never asked for
    // shadows from paying for them.
    let mut world = sunlit_world([0.0, 2.0, 0.0], ShadowMode::Off);
    let frame = frame(&mut world);
    assert!(
        frame.primary().expect("one view").lights[0]
            .shadow
            .is_none()
    );
}

#[test]
fn a_light_with_shadows_on_fits_one() {
    let mut world = sunlit_world([0.0, 2.0, 0.0], ShadowMode::Orthogonal);
    let frame = frame(&mut world);
    let shadow = frame.primary().expect("one view").lights[0]
        .shadow
        .expect("a fitted shadow");
    assert_eq!(
        shadow.resolution,
        DirectionalLight::default().shadow_resolution
    );
    assert_eq!(
        shadow.count, 1,
        "Orthogonal is one map — cascades are what the other mode is for"
    );
    assert!(
        shadow.cascades[0].bias > 0.0,
        "some bias, or every lit surface gets acne"
    );
}

#[test]
fn the_shadow_box_is_centred_on_the_camera() {
    // The point of fitting it at all: a directional light has no position, so the only sensible
    // thing to centre its map on is what the viewer can actually see. A box centred on the world
    // origin instead would leave a player who walked away from it with no shadows at all.
    let mut world = sunlit_world([100.0, 2.0, -50.0], ShadowMode::Orthogonal);
    let frame = frame(&mut world);

    let under_camera = shadow_uv(&frame, [100.0, 0.0, -50.0]).expect("projects");
    assert!(
        (under_camera[0] - 0.5).abs() < 0.01 && (under_camera[1] - 0.5).abs() < 0.01,
        "the ground under the camera should be the middle of the map, got {under_camera:?}"
    );

    // And the world origin, a hundred units away, is outside the box entirely — which is what the
    // shader treats as "no information here, so nothing is shadowed".
    let origin = shadow_uv(&frame, [0.0, 0.0, 0.0]).expect("projects");
    assert!(
        !(0.0..=1.0).contains(&origin[0]),
        "a point 100 units from a 30-unit box should be outside it, got {origin:?}"
    );
}

#[test]
fn the_shadow_box_covers_the_distance_it_was_asked_for() {
    // `shadow_distance` is the field with the most direct effect on quality, so what it means has to
    // be exact: it is the half-extent, so the box is twice it across.
    let mut world = sunlit_world([0.0, 2.0, 0.0], ShadowMode::Orthogonal);
    let frame = frame(&mut world);
    let half = DirectionalLight::default().shadow_distance;

    let inside = shadow_uv(&frame, [half * 0.9, 0.0, 0.0]).expect("projects");
    assert!(
        (0.0..=1.0).contains(&inside[0]),
        "just inside the half-extent should be on the map, got {inside:?}"
    );
    let outside = shadow_uv(&frame, [half * 1.1, 0.0, 0.0]).expect("projects");
    assert!(
        !(0.0..=1.0).contains(&outside[0]),
        "just outside it should not be, got {outside:?}"
    );
}

#[test]
fn cascade_radii_grow_and_end_exactly_at_the_shadow_distance() {
    // The two properties every split scheme must have whatever its blend: each cascade reaches
    // further than the last, and the furthest covers exactly what the light asked for. A scheme that
    // overshot would draw shadows past where anything samples them; one that undershot would leave a
    // ring of unshadowed ground inside the declared distance.
    for blend in [0.0, 0.25, 0.5, 0.75, 1.0] {
        let radii = amadeo_render::cascade_radii(80.0, blend);

        for pair in radii.windows(2) {
            assert!(
                pair[1] > pair[0],
                "cascades must grow outward, got {radii:?} at blend {blend}"
            );
        }
        assert!(
            (radii[radii.len() - 1] - 80.0).abs() < 1e-3,
            "the last cascade must land on the shadow distance, got {radii:?} at blend {blend}"
        );
    }
}

#[test]
fn the_blend_moves_between_even_slices_and_perspective_ones() {
    // **What the blend is for**, and the reason neither extreme is the answer.
    //
    // Uniform splits cut distance evenly, which starves the near cascade — where detail is actually
    // looked at. Logarithmic splits match how perspective compresses distance and pull every cascade
    // in tight, which starves the far one. The blend interpolates, so this checks it really does move
    // between the two rather than favouring one and ignoring the parameter.
    let uniform = amadeo_render::cascade_radii(80.0, 0.0);
    let middle = amadeo_render::cascade_radii(80.0, 0.5);
    let logarithmic = amadeo_render::cascade_radii(80.0, 1.0);

    // The first cascade is where the schemes disagree most.
    assert!(
        logarithmic[0] < uniform[0],
        "logarithmic splits pull the near cascade in tighter: {logarithmic:?} against {uniform:?}"
    );
    assert!(
        middle[0] > logarithmic[0] && middle[0] < uniform[0],
        "a half blend must land between the two, got {middle:?}"
    );

    // Uniform means literally even, which is the one value that can be checked against arithmetic
    // rather than against the other scheme.
    let step = uniform[1] - uniform[0];
    assert!(
        (uniform[2] - uniform[1] - step).abs() < 1e-3,
        "blend 0.0 must give even slices, got {uniform:?}"
    );
}

#[test]
fn every_cascade_snaps_to_its_own_texel_grid_rather_than_a_shared_one() {
    // **The property most likely to be got wrong when this reaches the GPU**, and the reason
    // `fit_cascade` takes a radius rather than sharing one snap.
    //
    // The grid a box snaps to is one shadow-map texel wide, and a cascade covering a quarter of the
    // distance at the same resolution has texels a quarter the size. Snapping them all to the largest
    // cascade's grid would compile, look right in a still, and leave the near cascades crawling as
    // the camera moves — which is the exact artefact snapping exists to remove.
    //
    // Checked through the radii rather than the matrices: a cascade's texel size is its radius over
    // its resolution, so four different radii is four different grids by construction.
    let radii = amadeo_render::cascade_radii(80.0, 0.5);
    let resolution = 2048.0;

    let texels: Vec<f32> = radii.iter().map(|r| 2.0 * r / resolution).collect();
    for pair in texels.windows(2) {
        assert!(
            pair[1] > pair[0] * 1.2,
            "each cascade's texels must be meaningfully larger than the last, got {texels:?} — \
             if these were equal the cascades would all be covering the same ground"
        );
    }
}

#[test]
fn a_shadow_box_moves_in_whole_texels() {
    // **The anti-shimmer property, and the reason the snap grid is anchored at the world origin
    // rather than at the camera.**
    //
    // If the box slid continuously with the camera, every shadow-map pixel would cover a slightly
    // different patch of world each frame and every shadow edge would crawl and fizz while the
    // player walked — with nothing in the scene actually moving. Snapping means a fixed world point
    // keeps landing on exactly the same texel until the box jumps a whole one.
    //
    // So: nudge the camera by a fraction of a texel and a fixed point must not move at all.
    let half = DirectionalLight::default().shadow_distance;
    let resolution = DirectionalLight::default().shadow_resolution;
    let texel = (2.0 * half) / resolution as f32;

    let mut first = sunlit_world([0.0, 2.0, 0.0], ShadowMode::Orthogonal);
    let before = shadow_uv(&frame(&mut first), [1.0, 0.0, 1.0]).expect("projects");

    let mut nudged = sunlit_world([texel * 0.25, 2.0, 0.0], ShadowMode::Orthogonal);
    let after = shadow_uv(&frame(&mut nudged), [1.0, 0.0, 1.0]).expect("projects");

    assert_eq!(
        before, after,
        "a sub-texel camera move must not move the shadow map under a fixed point"
    );

    // And the control: move a long way and it *must* change, or the box is not following the camera
    // at all and the test above would pass for the wrong reason.
    let mut moved = sunlit_world([texel * 400.0, 2.0, 0.0], ShadowMode::Orthogonal);
    let far = shadow_uv(&frame(&mut moved), [1.0, 0.0, 1.0]).expect("projects");
    assert_ne!(before, far, "a large camera move should move the box");
}

#[test]
fn a_2d_camera_fits_no_shadow_box() {
    // An orthographic camera draws no meshes (ADR 0031), so a shadow map for it would be a whole
    // extra pass over the scene rendering nothing. Every one of the target games that is 2D would
    // pay for it.
    let mut world = sunlit_world([0.0, 2.0, 0.0], ShadowMode::Orthogonal);
    // Replace the camera with a flat one.
    let flat = world.spawn();
    world.insert(flat, Transform::at(0.0, 0.0));
    world.insert(flat, Camera::orthographic(10.0));

    let frame = frame(&mut world);
    // Selected by *projection*, not by having no meshes: this world has no meshes at all, so
    // "meshes is empty" would have matched the 3D camera too and the test would have passed while
    // looking at the wrong view.
    let flat_view = frame
        .views
        .iter()
        .find(|view| matches!(view.camera.projection, Projection::Orthographic { .. }))
        .expect("a 2D view");
    assert!(flat_view.lights.iter().all(|light| light.shadow.is_none()));

    // And the 3D camera in the same world still gets one, so this is testing the projection rather
    // than something having gone wrong for both.
    let deep_view = frame
        .views
        .iter()
        .find(|view| matches!(view.camera.projection, Projection::Perspective { .. }))
        .expect("a 3D view");
    assert!(deep_view.lights.iter().any(|light| light.shadow.is_some()));
}

#[test]
fn only_one_light_casts_a_shadow() {
    // Every extra shadow-casting light is another full pass over the scene. Choosing between a loop
    // in the shader and a pass per light is the same open question this crate already has about
    // lighting in general, and answering it here — for shadows only — would answer it in the wrong
    // place.
    let mut world = sunlit_world([0.0, 2.0, 0.0], ShadowMode::Orthogonal);
    let second = world.spawn();
    world.insert(
        second,
        Transform {
            rotation: [-45.0, 30.0, 0.0],
            ..Transform::default()
        },
    );
    world.insert(second, DirectionalLight::casting_shadows());

    let frame = frame(&mut world);
    let casting = frame
        .primary()
        .expect("one view")
        .lights
        .iter()
        .filter(|light| light.shadow.is_some())
        .count();
    assert_eq!(casting, 1, "two lights asked; exactly one should cast");
}

#[test]
fn a_cascaded_light_fits_four_boxes_that_grow_outward() {
    // The mode existing end to end, which the `cascade_radii` tests above cannot show: they check
    // the split arithmetic in isolation, and this checks that asking for cascades in a component
    // actually produces four fitted boxes on the frame a backend receives.
    let mut world = sunlit_world([0.0, 2.0, 0.0], ShadowMode::Cascaded { blend: 0.5 });
    let frame = frame(&mut world);
    let shadow = frame.primary().expect("one view").lights[0]
        .shadow
        .expect("a fitted shadow");

    assert_eq!(shadow.count, amadeo_render::CASCADE_COUNT);
    for index in 1..shadow.count {
        assert!(
            shadow.cascades[index].far > shadow.cascades[index - 1].far,
            "cascade {index} must reach further than the one before it, got {:?}",
            shadow.cascades.map(|c| c.far)
        );
    }
    assert!(
        (shadow.cascades[shadow.count - 1].far - DirectionalLight::default().shadow_distance).abs()
            < 0.01,
        "the last cascade must reach exactly the light's shadow distance"
    );
}

#[test]
fn a_near_cascade_gets_a_larger_bias_than_a_far_one() {
    // **The trap the plan named**, and the one that is invisible until a scene is big: a bias is
    // expressed in the light's *clip* depth, which spans that cascade's own box. A near cascade
    // covering ten metres and a far one covering seventy turn the same authored world-unit offset
    // into very different clip-space numbers.
    //
    // Sharing one bias across all four therefore means picking which end to break — too little for
    // the far cascades, which stipple themselves dark with acne, or too much for the near one, which
    // detaches shadows from the things casting them. The per-cascade division in `fit_cascade` is
    // what avoids the choice, and this is what says it is happening.
    let mut world = sunlit_world([0.0, 2.0, 0.0], ShadowMode::Cascaded { blend: 0.5 });
    let frame = frame(&mut world);
    let shadow = frame.primary().expect("one view").lights[0]
        .shadow
        .expect("a fitted shadow");

    for index in 1..shadow.count {
        assert!(
            shadow.cascades[index].bias < shadow.cascades[index - 1].bias,
            "cascade {index} covers more depth than the one before it, so the same authored offset \
             must come out as a smaller share of its clip range — got {:?}",
            shadow.cascades.map(|c| c.bias)
        );
    }
}

#[test]
fn every_mode_agrees_about_how_many_maps_it_needs() {
    // One place decides this, so a texture's layer count, the number of shadow passes and the
    // shader's loop bound cannot disagree. Duplicating the match is exactly how those three drift.
    assert_eq!(ShadowMode::Off.map_count(), 0);
    assert_eq!(ShadowMode::Orthogonal.map_count(), 1);
    assert_eq!(
        ShadowMode::Cascaded { blend: 0.5 }.map_count(),
        amadeo_render::CASCADE_COUNT
    );
}

#[test]
fn cascades_make_the_near_shadow_map_far_finer_than_a_single_one() {
    // **The actual reason cascades exist**, stated as a number rather than as "edges look better".
    //
    // A shadow-map texel covers `2 × radius / resolution` world units. One map over the Scarp's
    // 70-metre box at 2048 pixels gives about seven centimetres of ground per texel, which is what
    // makes its edges visibly blocky. The near cascade covers a fraction of that distance at the
    // same resolution, so its texels are proportionally smaller — and near the camera is exactly
    // where shadow edges are looked at.
    //
    // Asserting a ratio rather than an absolute size, so this stays true if the resolution or the
    // distance is retuned. What it is really pinning is that the near cascade is *meaningfully*
    // tighter: a split scheme that produced four nearly-equal radii would satisfy
    // `cascade_radii_grow_and_end_exactly_at_the_shadow_distance` and buy almost nothing.
    let distance = 70.0;
    let radii = amadeo_render::cascade_radii(distance, 0.5);

    let single_texel = 2.0 * distance;
    let near_texel = 2.0 * radii[0];

    assert!(
        near_texel * 4.0 < single_texel,
        "the near cascade should give at least four times the resolution of one map over the whole \
         distance — {distance} against a near radius of {}, which is only {:.1}x",
        radii[0],
        single_texel / near_texel
    );
}
