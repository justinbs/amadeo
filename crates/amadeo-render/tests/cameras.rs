//! Cameras as entities — ADR 0031.
//!
//! The behaviours that are new rather than moved: several cameras in one world, the order they draw
//! in, a camera that follows its parent, and the two ways a camera declines to draw.

use amadeo_ecs::World;
use amadeo_render::{
    Camera, NullBackend, Projection, Quad, Renderer, SortOrder, describe_frame,
    describe_frame_through, render_quads,
};
use amadeo_transform::{GlobalTransform, Parent, Transform, propagate_transforms};

fn world_with_renderer() -> World {
    let mut world = World::new();
    world.insert_service(Renderer::new(Box::new(NullBackend::new(800, 600))));
    world
}

fn add_camera(world: &mut World, camera: Camera, at: [f32; 2]) -> amadeo_ecs::Entity {
    let entity = world.spawn();
    world.insert(entity, Transform::at(at[0], at[1]));
    world.insert(entity, camera);
    entity
}

fn add_quad(world: &mut World, x: f32) {
    let entity = world.spawn();
    world.insert(entity, Transform::at(x, 0.0));
    world.insert(entity, Quad::new(1.0, 1.0, [1.0, 1.0, 1.0, 1.0]));
    world.insert(entity, SortOrder::new(0));
}

fn last_frame(world: &World) -> amadeo_render::FrameData {
    world
        .service::<Renderer>()
        .expect("installed")
        .null_backend()
        .expect("null backend")
        .last_frame()
        .expect("rendered")
        .clone()
}

#[test]
fn one_world_can_hold_several_cameras() {
    // The whole point of the change. Two cameras, one quad: the quad is drawn twice, once per view.
    let mut world = world_with_renderer();
    add_camera(&mut world, Camera::orthographic(10.0), [0.0, 0.0]);
    add_camera(&mut world, Camera::orthographic(4.0), [3.0, 0.0]);
    add_quad(&mut world, 0.0);

    render_quads(&mut world);
    let frame = last_frame(&world);

    assert_eq!(frame.views.len(), 2);
    assert_eq!(frame.quad_count(), 2, "one quad, seen twice");
    // Each view records where its camera actually is, which is what makes them different pictures.
    assert_eq!(frame.views[0].eye, [0.0, 0.0]);
    assert_eq!(frame.views[1].eye, [3.0, 0.0]);
}

#[test]
fn cameras_draw_in_order_low_to_high() {
    // A HUD camera sits above a world camera, and the file says so rather than the spawn sequence.
    let mut world = world_with_renderer();
    let mut hud = Camera::orthographic(10.0);
    hud.order = 10;
    let mut background = Camera::orthographic(10.0);
    background.order = -10;

    // Spawned in the wrong order on purpose.
    add_camera(&mut world, hud, [1.0, 0.0]);
    add_camera(&mut world, background, [2.0, 0.0]);
    add_quad(&mut world, 0.0);

    render_quads(&mut world);
    let frame = last_frame(&world);

    assert_eq!(frame.views[0].camera.order, -10, "lowest order draws first");
    assert_eq!(frame.views[1].camera.order, 10);
}

#[test]
fn an_inactive_camera_does_not_draw() {
    let mut world = world_with_renderer();
    let mut idle = Camera::orthographic(10.0);
    idle.active = false;
    add_camera(&mut world, idle, [0.0, 0.0]);
    add_quad(&mut world, 0.0);

    render_quads(&mut world);
    assert!(last_frame(&world).views.is_empty());
}

#[test]
fn a_perspective_camera_draws_meshes_and_not_quads() {
    // **This test used to say a perspective camera drew nothing at all**, and it was written asking
    // that when it *did* draw, it be because someone made it rather than because it always secretly
    // did. Someone made it: ADR 0035's mesh collection.
    //
    // What is pinned now is the rule that replaced it — a camera's projection selects which pass it
    // feeds. An orthographic camera feeds the quad and sprite passes, a perspective one feeds the
    // mesh pass, and neither is built on the other (ADR 0031). A single camera drawing both would
    // mean the quad pass inventing a projection for a 3D view, which is exactly the guess the old
    // behaviour existed to avoid.
    let mut world = world_with_renderer();
    add_camera(&mut world, Camera::perspective(60.0), [0.0, 0.0]);
    add_quad(&mut world, 0.0);

    render_quads(&mut world);
    let frame = last_frame(&world);

    assert_eq!(frame.views.len(), 1, "a perspective camera is a view now");
    let view = frame.primary().expect("one view");
    assert!(
        view.quads.is_empty() && view.batches.is_empty(),
        "a perspective camera must not draw the 2D passes"
    );
    // Nothing here has a `Mesh`, so there is nothing for it to draw either — but the *reason* is
    // now "no meshes in the world" rather than "this camera is skipped".
    assert!(view.meshes.is_empty());
}

#[test]
fn a_camera_parented_to_something_follows_it() {
    // ADR 0031 claims parenting a camera to a character *is* a follow camera, with no special case.
    // This is that claim, tested.
    let mut world = world_with_renderer();

    let character = world.spawn();
    world.insert(character, Transform::at(5.0, 2.0));
    world.insert(character, GlobalTransform::default());

    let eye = add_camera(&mut world, Camera::orthographic(10.0), [0.0, 0.0]);
    world.insert(eye, Parent(character));
    world.insert(eye, GlobalTransform::default());

    propagate_transforms(&mut world);
    render_quads(&mut world);

    assert_eq!(
        last_frame(&world).views[0].eye,
        [5.0, 2.0],
        "the camera should be wherever its parent is"
    );
}

#[test]
fn describe_answers_for_the_camera_that_draws_first() {
    let mut world = world_with_renderer();
    let mut second = Camera::orthographic(4.0);
    second.order = 5;
    add_camera(&mut world, second, [9.0, 9.0]);
    add_camera(&mut world, Camera::orthographic(10.0), [0.0, 0.0]);

    let description = describe_frame(&world);
    assert_eq!(description.eye, [0.0, 0.0, 0.0]);
    assert_eq!(description.camera.projection.height(), Some(10.0));
}

#[test]
fn describe_can_be_asked_about_a_different_camera() {
    let mut world = world_with_renderer();
    add_camera(&mut world, Camera::orthographic(10.0), [0.0, 0.0]);
    let minimap = add_camera(&mut world, Camera::orthographic(50.0), [7.0, 0.0]);

    let description = describe_frame_through(&world, minimap).expect("that entity is a camera");
    assert_eq!(description.eye, [7.0, 0.0, 0.0]);
    assert_eq!(description.camera.projection.height(), Some(50.0));

    // Asking about something that is not a camera answers `None` rather than quietly falling back
    // to a different one, which would answer a question nobody asked.
    let quad = world.spawn();
    world.insert(quad, Transform::at(0.0, 0.0));
    assert!(describe_frame_through(&world, quad).is_none());
}

#[test]
fn a_camera_can_target_a_texture_instead_of_the_window() {
    // Render-to-texture is a *setting*, which is one of the three reasons ADR 0031 moved the camera
    // out of a resource. Nothing draws into the texture yet — that needs `render.capture` — but the
    // camera is excluded from `describe`'s answer, which is what "not the window" has to mean.
    let mut world = world_with_renderer();
    let mut monitor = Camera::orthographic(6.0);
    monitor.target = "security_feed".to_string();
    monitor.order = -100;
    add_camera(&mut world, monitor, [4.0, 4.0]);
    add_camera(&mut world, Camera::orthographic(10.0), [0.0, 0.0]);

    render_quads(&mut world);
    // Both are collected: the frame is what would be drawn, and a texture target is still drawn.
    assert_eq!(last_frame(&world).views.len(), 2);

    // But `render.describe` answers for the window, even though the monitor sorts first.
    assert_eq!(describe_frame(&world).eye, [0.0, 0.0, 0.0]);
}

#[test]
fn a_camera_reports_its_projection_in_the_schema() {
    // `Projection` is neither a component nor a resource, so before ADR 0030 nothing could say what
    // its legal values were. It is a dependency of `Camera`, so registering one registers the other.
    let mut registry = amadeo_reflect::TypeRegistry::new();
    registry.register::<Camera>().expect("registers");

    let info = registry
        .get("Projection")
        .expect("registered as a dependency");
    let rendered = format!("{info:?}");
    assert!(rendered.contains("Orthographic"), "{rendered}");
    assert!(rendered.contains("Perspective"), "{rendered}");
    assert!(matches!(
        Camera::default().projection,
        Projection::Orthographic { .. }
    ));
    // The point of the payload: a perspective camera has no height to report, rather than a
    // meaningless one sitting beside it.
    assert_eq!(Camera::perspective(60.0).projection.height(), None);
}
