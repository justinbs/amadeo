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

/// The geometry the null backend is currently holding under an id.
fn uploaded(world: &World, id: &str) -> Option<amadeo_render::MeshData> {
    world
        .service::<Renderer>()
        .expect("installed")
        .null_backend()
        .expect("a null backend")
        .mesh(id)
        .cloned()
}

#[test]
fn geometry_that_changes_under_a_fixed_id_reaches_the_backend_again() {
    // **The defect this was written against, and it has a name: digging.**
    //
    // A terrain chunk that is dug re-meshes under the *same* asset id. Before `MeshCache` carried a
    // version, the renderer asked `has_mesh` -- "do you have anything called this?" -- got yes, and
    // skipped the upload forever. The collider changed on the tick the edit was made and the picture
    // never did, so the player walked into a tunnel that still looked like solid rock.
    //
    // Nothing about it is visible in a simulation test: the world is right, the physics is right,
    // and only the pixels are wrong. It is exactly the class `games/vault` and `games/atrium` were
    // built to catch, found here by reading instead.
    let mut world = world_with_a_3d_camera();
    world.insert_service(cache_with_a_box());

    let entity = world.spawn();
    world.insert(entity, Transform::at(0.0, 0.0));
    world.insert(entity, Mesh::new("panel", ""));

    frame(&mut world);
    let first = uploaded(&world, "panel").expect("uploaded on the first frame");

    // Re-mesh `panel` as something visibly different, exactly as a dug chunk would.
    let dug = BoxMesh {
        size: [4.0, 4.0, 4.0],
    }
    .tessellate();
    world
        .service_mut::<MeshCache>()
        .expect("installed")
        .insert("panel", dug.clone());

    frame(&mut world);
    let second = uploaded(&world, "panel").expect("still uploaded");

    assert_ne!(
        first, second,
        "the backend kept the pre-edit geometry; a dug tunnel would still look like rock"
    );
    assert_eq!(second, dug);
}

#[test]
fn geometry_the_cache_let_go_of_is_freed_rather_than_kept_forever() {
    // The other half, and the one that is invisible until it is fatal. A streamed world drops chunks
    // behind the player; if the backend keeps every mesh it was ever handed, walking in one
    // direction grows video memory for as long as the game runs. There is no wrong picture to see --
    // the frame is correct right up until the allocation fails.
    let mut world = world_with_a_3d_camera();
    world.insert_service(cache_with_a_box());

    let entity = world.spawn();
    world.insert(entity, Transform::at(0.0, 0.0));
    world.insert(entity, Mesh::new("panel", ""));

    frame(&mut world);
    assert!(
        uploaded(&world, "panel").is_some(),
        "uploaded to begin with"
    );

    // The chunk streams out: the entity goes, and so does the cache entry.
    world.despawn(entity);
    world
        .service_mut::<MeshCache>()
        .expect("installed")
        .remove("panel");

    frame(&mut world);
    assert!(
        uploaded(&world, "panel").is_none(),
        "the backend is still holding geometry nothing refers to"
    );
}

#[test]
fn removing_a_mesh_that_was_never_there_is_not_an_error() {
    // Load-bearing rather than lenient. A terrain streamer reports every chunk leaving the drawn
    // region -- including ones whose mesh never arrived and ones that meshed to nothing -- because
    // filtering that list by "was the caller ever given this" would make it depend on what the job
    // pool had finished (docs/07). Removal being harmless is what pays for that honesty.
    let mut cache = MeshCache::new();
    cache.remove("never_existed");
    assert!(cache.get("never_existed").is_none());

    let mut backend = NullBackend::new(8, 8);
    amadeo_render::RenderBackend::remove_mesh(&mut backend, "never_existed");
}

#[test]
fn an_id_that_comes_back_is_not_mistaken_for_the_one_that_left() {
    // Why the version counter is global rather than per entry. A chunk streams out and is walked
    // back to; with a per-entry counter its version would restart at 1, and a backend still holding
    // version 1 would decide it was already up to date and draw the old geometry.
    let mut cache = MeshCache::new();
    cache.insert("chunk", BoxMesh::default().tessellate());
    let first = cache.version_of("chunk").expect("inserted");

    cache.remove("chunk");
    cache.insert("chunk", BoxMesh::default().tessellate());
    let second = cache.version_of("chunk").expect("re-inserted");

    assert_ne!(
        first, second,
        "an id that left and came back must not reuse its old version"
    );
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
