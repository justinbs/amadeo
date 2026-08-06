//! Meshes and lights reach the frame — the collection half of ADR 0035.
//!
//! Headless, because a frame is data before it is pixels: what the renderer *would* draw is
//! computable with no GPU, and that is what invariant I7 buys. The pixels are checked in
//! `capture.rs` once the mesh pass exists.

use amadeo_ecs::World;
use amadeo_render::{
    BoxMesh, Camera, DirectionalLight, FrameData, Material, MaterialCache, Mesh, MeshCache,
    NullBackend, Renderer, SortOrder, render_quads,
};
use amadeo_transform::Transform;

fn world_with_a_3d_camera() -> World {
    let mut world = World::new();
    let eye = world.spawn();
    world.insert(eye, Transform::at(0.0, 0.0));
    world.insert(eye, Camera::perspective(60.0));
    world.insert_service(Renderer::new(Box::new(NullBackend::new(64, 64))));
    world
}

/// A cache holding one tessellated box under `panel`.
fn cache_with_a_box() -> MeshCache {
    let mut cache = MeshCache::new();
    cache.insert("panel", BoxMesh::default().tessellate());
    cache
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

#[test]
fn a_mesh_with_loaded_geometry_reaches_the_frame() {
    let mut world = world_with_a_3d_camera();
    world.insert_service(cache_with_a_box());

    let entity = world.spawn();
    world.insert(entity, Transform::at(2.0, 3.0));
    world.insert(entity, Mesh::new("panel", ""));

    let frame = frame(&mut world);
    let view = frame.primary().expect("one view");
    assert_eq!(view.meshes.len(), 1);
    assert_eq!(view.meshes[0].mesh, "panel");
    // The model matrix carries the transform, so the backend never needs the world.
    assert_eq!(view.meshes[0].model.translation(), [2.0, 3.0, 0.0]);
    // No material named, so the plain default rather than nothing at all.
    assert_eq!(view.meshes[0].material, Material::default());
}

#[test]
fn a_mesh_whose_geometry_never_loaded_is_skipped_rather_than_substituted() {
    // A missing texture has an honest stand-in; a missing *mesh* does not, because a substitute
    // cube would be a shape nobody authored sitting in the world. So the frame carries only what
    // can actually be drawn, which is also what keeps `render.describe` agreeing with it.
    let mut world = world_with_a_3d_camera();
    world.insert_service(MeshCache::new());

    let entity = world.spawn();
    world.insert(entity, Transform::at(0.0, 0.0));
    world.insert(entity, Mesh::new("no_such_mesh", ""));

    assert!(
        frame(&mut world)
            .primary()
            .expect("one view")
            .meshes
            .is_empty()
    );
}

#[test]
fn a_material_is_resolved_by_the_time_the_frame_is_built() {
    // Carried by value rather than by id, unlike a texture: a material is five numbers and a
    // string, so resolving it once here means the backend never reaches back into the world.
    let mut world = world_with_a_3d_camera();
    world.insert_service(cache_with_a_box());

    let mut materials = MaterialCache::new();
    materials.insert(
        "brass",
        Material {
            base_colour: [0.9, 0.7, 0.3, 1.0],
            metallic: 1.0,
            roughness: 0.2,
            ..Material::default()
        },
    );
    world.insert_service(materials);

    let entity = world.spawn();
    world.insert(entity, Transform::at(0.0, 0.0));
    world.insert(entity, Mesh::new("panel", "brass"));

    let frame = frame(&mut world);
    let material = &frame.primary().expect("one view").meshes[0].material;
    assert_eq!(material.metallic, 1.0);
    assert!((material.roughness - 0.2).abs() < 1e-6);
}

#[test]
fn meshes_come_back_in_sort_order() {
    let mut world = world_with_a_3d_camera();
    world.insert_service(cache_with_a_box());

    for order in [5_i32, -3, 0] {
        let entity = world.spawn();
        world.insert(entity, Transform::at(order as f32, 0.0));
        world.insert(entity, Mesh::new("panel", ""));
        world.insert(entity, SortOrder::new(order));
    }

    let frame = frame(&mut world);
    let orders: Vec<i32> = frame
        .primary()
        .expect("one view")
        .meshes
        .iter()
        .map(|instance| instance.order)
        .collect();
    assert_eq!(orders, [-3, 0, 5]);
}

#[test]
fn a_light_points_along_its_own_negative_z() {
    // The same convention a camera looks along, so aiming a light is aiming a camera and a scene
    // file needs no separate vocabulary for it. An unrotated light shines straight ahead, into -Z.
    let mut world = world_with_a_3d_camera();

    let sun = world.spawn();
    world.insert(sun, Transform::at(0.0, 10.0));
    world.insert(sun, DirectionalLight::default());

    let frame = frame(&mut world);
    let lights = &frame.primary().expect("one view").lights;
    assert_eq!(lights.len(), 1);

    let direction = lights[0].direction;
    assert!(
        (direction[2] + 1.0).abs() < 1e-5,
        "an unrotated light travels into -Z, got {direction:?}"
    );
    // Position is irrelevant to a directional light — only the angle matters, which is what makes
    // it cheap and what distinguishes it from the point lights that arrive with shadows.
    assert!(direction[0].abs() < 1e-5 && direction[1].abs() < 1e-5);
}

#[test]
fn intensity_is_folded_into_the_colour() {
    // So a backend multiplies nothing: what arrives is the light's actual contribution.
    let mut world = world_with_a_3d_camera();

    let sun = world.spawn();
    world.insert(sun, Transform::at(0.0, 0.0));
    world.insert(
        sun,
        DirectionalLight {
            colour: [1.0, 0.5, 0.25],
            intensity: 4.0,
            ..DirectionalLight::default()
        },
    );

    let frame = frame(&mut world);
    assert_eq!(
        frame.primary().expect("one view").lights[0].colour,
        [4.0, 2.0, 1.0]
    );
}

#[test]
fn a_light_with_no_intensity_is_not_carried() {
    let mut world = world_with_a_3d_camera();
    let sun = world.spawn();
    world.insert(sun, Transform::at(0.0, 0.0));
    world.insert(
        sun,
        DirectionalLight {
            intensity: 0.0,
            ..DirectionalLight::default()
        },
    );

    assert!(
        frame(&mut world)
            .primary()
            .expect("one view")
            .lights
            .is_empty()
    );
}

#[test]
fn collecting_meshes_does_not_change_the_state_hash() {
    // Rendering reads simulation state and never writes it (ADR 0005), and the caches it reads are
    // `Service`s so they cannot be in the hash either (ADR 0009). Asserted by running a frame rather
    // than by reading trait bounds.
    let mut world = world_with_a_3d_camera();
    world.insert_service(cache_with_a_box());
    let entity = world.spawn();
    world.insert(entity, Transform::at(1.0, 2.0));
    world.insert(entity, Mesh::new("panel", ""));

    let before = world.state_hash();
    render_quads(&mut world);
    render_quads(&mut world);
    assert_eq!(before, world.state_hash());
}
