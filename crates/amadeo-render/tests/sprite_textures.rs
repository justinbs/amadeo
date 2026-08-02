//! The whole path from a file on disk to a texture the backend holds, with no GPU involved.
//!
//! # What this file is for
//!
//! `amadeo-image` tests decoding. `textures.rs` tests the cache. This tests the thing neither of
//! them can see on its own: that running the **real render system**, against a **real world**, with
//! a **real asset directory**, ends with the right pixels in the backend under the right id.
//!
//! That is possible with no GPU because [`NullBackend`] records uploads the same way it records
//! draw calls (invariant I7). It is a much sharper claim than "no error was returned" — a renderer
//! that uploaded the placeholder for everything would pass the latter and fail every test here.

use amadeo_assets::{AssetCatalogue, Assets, Sidecar};
use amadeo_ecs::World;
use amadeo_render::{
    Camera2d, NullBackend, PLACEHOLDER_TEXTURE_ID, Renderer, Sprite, TextureCache, render_quads,
};
use amadeo_transform::Transform;
use std::path::{Path, PathBuf};

/// One red pixel, as ASCII PPM. Hand-readable on purpose — the value asserted below is visible here.
const RED: &[u8] = b"P3 1 1 255 255 0 0";
/// One blue pixel.
const BLUE: &[u8] = b"P3 1 1 255 0 0 255";

/// A temporary asset directory with real files in it, and the `Assets` that catalogues them.
struct Project {
    root: PathBuf,
    assets: Assets,
}

impl Project {
    fn new(name: &str, files: &[(&str, &[u8])]) -> Project {
        let root = std::env::temp_dir().join(format!(
            "amadeo-sprite-tex-{name}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("temp dir");

        let mut project = Project {
            root,
            assets: Assets::from_catalogue(AssetCatalogue::new()),
        };
        for (id, bytes) in files {
            project.write(id, bytes);
        }
        project.reload();
        project
    }

    /// Writes an asset file and catalogues it, without loading its bytes.
    fn write(&mut self, id: &str, bytes: &[u8]) {
        let file = format!("{id}.ppm");
        std::fs::write(self.root.join(&file), bytes).expect("write");
        if !self.assets.catalogue.contains(id) {
            self.assets
                .catalogue
                .insert(Sidecar::new(id), Path::new(&file))
                .expect("distinct id");
        }
    }

    /// Loads every catalogued asset's bytes, the way ADR 0021's barrier does before the first tick.
    fn reload(&mut self) {
        let ids: Vec<String> = self.assets.catalogue.ids().map(str::to_string).collect();
        let Assets {
            catalogue, store, ..
        } = &mut self.assets;
        store.load_all(catalogue, &self.root, ids.iter().map(String::as_str));
    }

    /// A world with this project's assets, a texture cache, and a recording backend installed.
    ///
    /// Takes `&mut self` rather than `self` so the `Project` stays alive alongside the world it
    /// produced — it owns the temporary directory, and dropping it would delete the files out from
    /// under a test that loads them a second time.
    fn world(&mut self) -> World {
        let mut world = World::new();
        world.insert_resource(Camera2d::default());
        world.insert_service(Renderer::new(Box::new(NullBackend::new(800, 600))));
        world.insert_service(TextureCache::new());
        world.insert_service(std::mem::take(&mut self.assets));
        world
    }
}

impl Drop for Project {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// Adds an entity that draws `texture`.
fn add_sprite(world: &mut World, texture: &str) {
    let entity = world.spawn();
    world.insert(entity, Transform::at(0.0, 0.0));
    world.insert(entity, Sprite::new(texture, 1.0, 1.0));
}

/// The pixels the backend holds under an id, if any.
fn uploaded(world: &World, id: &str) -> Option<Vec<u8>> {
    world
        .service::<Renderer>()
        .expect("installed")
        .null_backend()
        .expect("null backend")
        .texture(id)
        .map(|texture| texture.pixels.clone())
}

#[test]
fn a_sprites_texture_reaches_the_backend_as_the_pixels_the_file_holds() {
    // The claim this whole session is about, minus the GPU: a file on disk becomes an uploaded
    // texture, addressed by the id the sprite named.
    let mut project = Project::new("basic", &[("wall", RED)]);
    let mut world = project.world();
    add_sprite(&mut world, "wall");

    render_quads(&mut world);

    assert_eq!(uploaded(&world, "wall"), Some(vec![255, 0, 0, 255]));
}

#[test]
fn only_the_textures_this_frame_uses_are_uploaded() {
    // A project with a gigabyte of art must not upload all of it to draw one wall. The frame's
    // batches decide what is needed, which is also why the batcher runs first.
    let mut project = Project::new("subset", &[("wall", RED), ("floor", BLUE)]);
    let mut world = project.world();
    add_sprite(&mut world, "wall");

    render_quads(&mut world);

    assert!(uploaded(&world, "wall").is_some());
    assert!(uploaded(&world, "floor").is_none());
}

#[test]
fn a_missing_texture_uploads_a_placeholder_and_reports_itself() {
    // ADR 0021: visible stand-in *plus* a structured report. Both halves are asserted, because a
    // frame that silently draws magenta is a frame an agent cannot diagnose.
    let mut project = Project::new("missing", &[]);
    let mut world = project.world();
    add_sprite(&mut world, "wall");

    render_quads(&mut world);

    // Something was drawn -- the built-in 2x2 check, since no placeholder asset exists either.
    let pixels = uploaded(&world, "wall").expect("a placeholder was uploaded");
    assert_eq!(pixels.len(), 2 * 2 * 4);
    assert_eq!(&pixels[..4], &[230, 0, 230, 255]);

    // And it said so.
    let cache = world.service::<TextureCache>().expect("installed");
    assert!(!cache.is_decoded("wall"));
    let reported: Vec<&str> = cache.failures().map(|(id, _)| id).collect();
    assert!(reported.contains(&"wall"), "got {reported:?}");
}

#[test]
fn a_games_own_placeholder_is_used_when_it_ships_one() {
    let mut project = Project::new("custom", &[(PLACEHOLDER_TEXTURE_ID, BLUE)]);
    let mut world = project.world();
    add_sprite(&mut world, "absent");

    render_quads(&mut world);

    assert_eq!(uploaded(&world, "absent"), Some(vec![0, 0, 255, 255]));
}

#[test]
fn a_texture_is_uploaded_once_and_not_again_every_frame() {
    // Uploading is the expensive part of drawing a sprite, and doing it per frame would make the
    // batching in front of it pointless.
    let mut project = Project::new("once", &[("wall", RED)]);
    let mut world = project.world();
    add_sprite(&mut world, "wall");

    for _ in 0..10 {
        render_quads(&mut world);
    }

    let backend = world
        .service::<Renderer>()
        .expect("installed")
        .null_backend()
        .expect("null")
        .clone();
    assert_eq!(backend.frames_rendered(), 10);
    // Ten frames, one texture. If uploads were repeating, this would still be 1 -- so the sharper
    // check is that the cache decoded once, which `ensure` short-circuits on.
    assert_eq!(backend.texture_ids().collect::<Vec<_>>(), vec!["wall"]);
    assert_eq!(world.service::<TextureCache>().expect("installed").len(), 1);
}

#[test]
fn a_texture_that_arrives_late_replaces_its_placeholder() {
    // The case `Renderer::has_texture` alone gets wrong. Frame one draws a placeholder because the
    // file is not there; the file appears; a later frame must show the real thing rather than the
    // placeholder forever. ADR 0021 explicitly permits loading past the barrier, so this is a
    // supported path and not a curiosity.
    let mut project = Project::new("late", &[]);
    project.write("wall", RED);
    // Catalogued but deliberately not loaded yet, so the first frame has no bytes to decode.
    let root = project.root.clone();
    let mut world = project.world();
    add_sprite(&mut world, "wall");

    render_quads(&mut world);
    let placeholder = uploaded(&world, "wall").expect("placeholder");
    assert_eq!(placeholder.len(), 2 * 2 * 4, "the built-in check is 2x2");

    // The asset arrives: bytes are loaded, and the cache is told to stop remembering the failure.
    world.with_service_taken::<Assets, ()>(|_world, assets| {
        let Assets {
            catalogue, store, ..
        } = assets;
        store.load_all(catalogue, &root, ["wall"]);
    });
    world
        .service_mut::<TextureCache>()
        .expect("installed")
        .forget("wall");

    render_quads(&mut world);

    assert_eq!(uploaded(&world, "wall"), Some(vec![255, 0, 0, 255]));
}

#[test]
fn one_texture_shared_by_many_sprites_is_uploaded_once() {
    // The tilesheet case, seen from the upload side rather than the batching side.
    let mut project = Project::new("sheet", &[("tiles", RED)]);
    let mut world = project.world();
    for _ in 0..50 {
        add_sprite(&mut world, "tiles");
    }

    render_quads(&mut world);

    let frame_batches = world
        .service::<Renderer>()
        .expect("installed")
        .null_backend()
        .expect("null")
        .last_frame()
        .expect("rendered")
        .batch_count();
    assert_eq!(frame_batches, 1, "50 sprites, one sheet, one draw call");
    assert_eq!(
        world
            .service::<Renderer>()
            .expect("installed")
            .null_backend()
            .expect("null")
            .texture_ids()
            .count(),
        1
    );
}

#[test]
fn rendering_textures_does_not_move_the_state_hash() {
    // Invariant I3 across the new code. Decoding reads files and allocates megabytes, and none of
    // it may reach the simulation -- which holds structurally, because both `Assets` and
    // `TextureCache` are services and ADR 0009 excludes those by trait bound.
    let mut project = Project::new("hash", &[("wall", RED)]);
    let mut world = project.world();
    add_sprite(&mut world, "wall");

    let before = world.state_hash();
    for _ in 0..5 {
        render_quads(&mut world);
    }
    assert_eq!(world.state_hash(), before);

    // And the same run with the texture *missing* must reach the same hash, or a failed decode
    // would be observable to gameplay.
    let mut broken = Project::new("hash-missing", &[]);
    let mut other = broken.world();
    add_sprite(&mut other, "wall");
    for _ in 0..5 {
        render_quads(&mut other);
    }
    assert_eq!(other.state_hash(), before);
}

#[test]
fn a_world_with_no_asset_system_still_draws_placeholders() {
    // Invariant I7 at its bluntest: a headless test that installed nothing but a renderer must
    // still produce a frame rather than panicking on a missing service.
    let mut world = World::new();
    world.insert_service(Renderer::new(Box::new(NullBackend::new(320, 240))));
    world.insert_service(TextureCache::new());
    add_sprite(&mut world, "whatever");

    render_quads(&mut world);

    let pixels = uploaded(&world, "whatever").expect("the built-in placeholder needs no files");
    assert_eq!(pixels.len(), 2 * 2 * 4);
}

#[test]
fn a_world_with_no_texture_cache_renders_without_textures() {
    // The cache is optional machinery, not a requirement. A game that never installs one should
    // draw its quads and simply not draw sprites, rather than failing.
    let mut world = World::new();
    world.insert_service(Renderer::new(Box::new(NullBackend::new(320, 240))));
    add_sprite(&mut world, "whatever");

    render_quads(&mut world);

    assert!(uploaded(&world, "whatever").is_none());
    assert_eq!(
        world
            .service::<Renderer>()
            .expect("installed")
            .null_backend()
            .expect("null")
            .frames_rendered(),
        1
    );
}
