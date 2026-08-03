//! Rendering: turning simulation state into pixels.
//!
//! # Structure
//!
//! Everything here is split across a [`RenderBackend`] boundary. The engine builds a [`FrameData`]
//! by reading the world; a backend turns that into pixels, or into nothing at all.
//!
//! ```
//! use amadeo_ecs::World;
//! use amadeo_render::{Camera, NullBackend, Quad, Renderer, render_quads};
//! use amadeo_transform::Transform;
//!
//! let mut world = World::new();
//! world.insert_service(Renderer::new(Box::new(NullBackend::new(800, 600))));
//!
//! // A camera is an entity (ADR 0031). Nothing draws without one, deliberately: inventing a
//! // default view would be drawing something nobody authored.
//! let eye = world.spawn();
//! world.insert(eye, Transform::at(0.0, 0.0));
//! world.insert(eye, Camera::orthographic(10.0));
//!
//! let entity = world.spawn();
//! world.insert(entity, Transform::at(1.0, 0.0));
//! world.insert(entity, Quad::new(1.0, 1.0, [1.0, 0.0, 0.0, 1.0]));
//!
//! render_quads(&mut world);
//!
//! // The null backend records what would have been drawn -- assertable with no GPU.
//! let renderer = world.service::<Renderer>().expect("installed");
//! assert_eq!(renderer.null_backend().expect("null").last_quad_count(), 1);
//! ```
//!
//! # Rendering never writes simulation state
//!
//! The collection pass uses [`World::iter_pair`](amadeo_ecs::World::iter_pair), which is read-only —
//! deliberately, since the mutable query would mark every drawn entity as changed each frame and
//! make change detection worthless. Results go into a [`Renderer`] *service*, never a resource, so
//! nothing rendering does can move the state hash (ADR 0009).
//!
//! That is what makes invariant I7 hold: a headless run and a windowed run reach identical
//! simulation state, because rendering is incapable of affecting it.

mod backend;
mod components;
mod describe;
#[cfg(feature = "gpu")]
mod gpu;
mod sprites;
mod textures;

pub use backend::{
    FrameData, NullBackend, QuadInstance, RenderBackend, RenderError, SpriteBatch, SpriteInstance,
    View,
};
pub use components::{Camera, Projection, Quad, SortOrder, Sprite};
pub use describe::{
    DrawnEntity, DrawnKind, FrameDescription, describe_frame, describe_frame_through,
};
#[cfg(feature = "gpu")]
pub use gpu::WgpuBackend;
pub use sprites::{COLLECT_SPRITES, collect_sprites};
pub use textures::{PLACEHOLDER_TEXTURE_ID, TextureCache, TextureFailure, decode_frame_textures};
// Re-exported because a caller holding a `TextureCache` needs to talk about what is in it, and
// making them add `amadeo-image` to their own manifest for a type this crate hands them would be a
// dependency they did not choose.
pub use amadeo_image::{PixelFormat, TextureData};

use amadeo_ecs::{Service, World};
use std::collections::BTreeSet;
// Not re-exported: `Transform` belongs to `amadeo-transform` (ADR 0015), and two import paths to
// one type is exactly the sort of thing that makes people wonder whether they are the same type.
use amadeo_transform::{GlobalTransform, Mat4, Transform};

/// One entity's own transform as a matrix, for the fallback when propagation has not run.
fn local_matrix(transform: &Transform) -> Mat4 {
    Mat4::from_transform(transform.translation, transform.rotation, transform.scale)
}

/// The label the app layer registers [`render_quads`] under.
pub const RENDER_QUADS: &str = "render_quads";

/// Holds the active rendering backend.
///
/// A [`Service`]: rendering machinery, never simulation state.
#[derive(Debug)]
pub struct Renderer {
    backend: Box<dyn RenderBackend>,
    /// Background colour, linear RGBA.
    pub clear_color: [f32; 4],
    /// Set when the last frame could not be drawn. Cleared on the next success.
    last_error: Option<RenderError>,
    /// Ids whose uploaded pixels are a placeholder rather than the real texture.
    ///
    /// Needed because [`RenderBackend::has_texture`] answers "is *something* uploaded", which is the
    /// wrong question for an asset that arrives late: without this, a texture that fell back on
    /// frame one would keep its placeholder forever, since the backend would report it as present.
    /// Every id in here is re-checked each frame and re-uploaded the moment it really decodes.
    placeholders_uploaded: BTreeSet<String>,
}

impl Service for Renderer {}

impl Renderer {
    /// Wraps a backend.
    #[must_use]
    pub fn new(backend: Box<dyn RenderBackend>) -> Self {
        Self {
            backend,
            clear_color: FrameData::default().clear_color,
            last_error: None,
            placeholders_uploaded: BTreeSet::new(),
        }
    }

    /// Creates a renderer that draws nothing. The default for headless runs.
    #[must_use]
    pub fn headless() -> Self {
        Self::new(Box::new(NullBackend::default()))
    }

    /// The backend's name, for diagnostics.
    #[must_use]
    pub fn backend_name(&self) -> &'static str {
        self.backend.name()
    }

    /// The current drawable size in physical pixels.
    #[must_use]
    pub fn viewport(&self) -> (u32, u32) {
        self.backend.viewport()
    }

    /// Tells the backend the drawable size changed.
    pub fn resize(&mut self, width: u32, height: u32) {
        self.backend.resize(width, height);
    }

    /// The error from the last failed frame, if the last frame failed.
    ///
    /// Surfaced rather than logged-and-forgotten, so a game that is silently drawing nothing can be
    /// diagnosed by asking rather than by guessing.
    #[must_use]
    pub fn last_error(&self) -> Option<&RenderError> {
        self.last_error.as_ref()
    }

    /// The backend as a [`NullBackend`], if that is what it is.
    ///
    /// Lets headless tests and CI assert on what *would* have been drawn without a GPU. Returns
    /// `None` for a real backend, which has nothing equivalent to offer.
    #[must_use]
    pub fn null_backend(&self) -> Option<&NullBackend> {
        self.backend.as_any().downcast_ref::<NullBackend>()
    }

    /// Uploads any texture this frame needs that the backend does not already hold.
    ///
    /// Runs before every draw. The common case is that nothing happens: after the first frame, each
    /// batch's texture is already on the GPU and this is one map lookup per *batch* — of which
    /// ADR 0023 keeps there being very few.
    ///
    /// Two cases do upload:
    ///
    /// - **First sight.** The backend has nothing under this id.
    /// - **A late arrival.** The id was uploaded as a placeholder and has since decoded for real,
    ///   which is the streaming case ADR 0021 permits. Without this check the placeholder would be
    ///   permanent, because the backend would keep answering "yes, I have that texture".
    ///
    /// Failures are recorded, not propagated: a texture that will not fit in video memory should
    /// leave the game running and visibly wrong, not stop it.
    fn upload_frame_textures(&mut self, frame: &FrameData, cache: &TextureCache) {
        // Across every view. Two cameras seeing one texture upload it once, because the check below
        // is against what the backend already holds rather than against what this frame asked for.
        for batch in frame.batches() {
            let id = batch.texture.as_str();
            let decoded_for_real = cache.is_decoded(id);
            let needs_upload = !self.backend.has_texture(id)
                || (decoded_for_real && self.placeholders_uploaded.contains(id));
            if !needs_upload {
                continue;
            }

            match self.backend.upload_texture(id, cache.get(id)) {
                Ok(()) => {
                    if decoded_for_real {
                        self.placeholders_uploaded.remove(id);
                    } else {
                        self.placeholders_uploaded.insert(id.to_string());
                    }
                }
                Err(error) => self.last_error = Some(error),
            }
        }
    }

    /// Draws a frame.
    fn render(&mut self, frame: &FrameData) {
        match self.backend.render(frame) {
            Ok(()) => self.last_error = None,
            // A failed frame is normal during a resize or while minimised, so it is recorded rather
            // than propagated. A game that stops drawing should not stop simulating.
            Err(error) => self.last_error = Some(error),
        }
    }
}

/// Every camera that should draw this frame, with its world position, in draw order.
///
/// Since ADR 0031 a camera is an entity, so this is a query rather than a resource lookup. Three
/// rules, all of them things a reader should be able to check against the code:
///
/// - **Inactive cameras are skipped**, so `active false` in a scene file means what it says.
/// - **A perspective camera is skipped too**, because nothing draws through one yet — the mesh pass
///   arrives later in M2 and guessing at a projection would be worse than drawing nothing.
/// - **Sorted by `Camera::order`, then by entity**, so the order is total and reproducible. Two
///   cameras at the same order would otherwise draw in whichever sequence iteration happened to
///   produce, and I3 wants reproducible-*and*-meaningful rather than merely reproducible.
///
/// The position comes from `GlobalTransform` when it is there, so a camera parented to a character
/// follows it — which is what ADR 0031 means by "parenting a camera to a character *is* a follow
/// camera". It falls back to the local `Transform` for an unparented camera, or one in a game that
/// has not run propagation.
/// One camera entity's world position, from `GlobalTransform` if propagation has run.
///
/// `[0, 0]` for an entity with no transform at all, which is a camera nobody finished setting up.
#[must_use]
pub fn camera_eye(world: &World, entity: amadeo_ecs::Entity) -> [f32; 2] {
    let matrix = match world.get::<GlobalTransform>(entity) {
        Some(global) => global.to_mat4(),
        None => match world.get::<Transform>(entity) {
            Some(transform) => local_matrix(transform),
            None => return [0.0, 0.0],
        },
    };
    let at = matrix.translation();
    [at[0], at[1]]
}

/// The camera that draws first to the window, with its world position.
///
/// What `render.describe` answers for by default, and the nearest thing to "the camera" now that a
/// world may hold several. `None` when a world has none — a state worth distinguishing from a
/// default camera, which is why this returns an `Option` rather than falling back here.
#[must_use]
pub fn primary_camera(world: &World) -> Option<(Camera, [f32; 2])> {
    active_cameras(world)
        .into_iter()
        .find(|(camera, _)| camera.target.is_empty())
}

fn active_cameras(world: &World) -> Vec<(Camera, [f32; 2])> {
    let mut found: Vec<(i32, amadeo_ecs::Entity, Camera, [f32; 2])> = world
        .query::<(&Camera, &Transform, Option<&GlobalTransform>)>()
        .filter(|(_, (camera, _, _))| {
            camera.active && matches!(camera.projection, Projection::Orthographic)
        })
        .map(|(entity, (camera, transform, global))| {
            let matrix = match global {
                Some(global) => global.to_mat4(),
                None => local_matrix(transform),
            };
            let at = matrix.translation();
            (camera.order, entity, camera.clone(), [at[0], at[1]])
        })
        .collect();

    found.sort_by_key(|(order, entity, _, _)| (*order, entity.index(), entity.generation()));
    found
        .into_iter()
        .map(|(_, _, camera, eye)| (camera, eye))
        .collect()
}

/// Collects every drawable entity and hands the frame to the backend.
///
/// Registered in the app layer's `Render` stage, outside the deterministic zone. Does nothing if no
/// [`Renderer`] service is installed.
///
/// Quads are sorted by [`SortOrder`] with a **stable** sort, so entities sharing an order keep
/// their iteration order — which is itself deterministic (invariant I3). Draw order is therefore
/// reproducible without being arbitrary. An entity with no [`SortOrder`] draws at zero.
pub fn render_quads(world: &mut World) {
    if !world.has_service::<Renderer>() {
        return;
    }

    let clear_color = world
        .service::<Renderer>()
        .map_or(FrameData::default().clear_color, |r| r.clear_color);

    // One query, four components, two of them optional. `SortOrder` and `GlobalTransform` are both
    // optional on purpose — an entity missing either still draws, at order zero and at its local
    // transform respectively, because requiring them would mean forgetting a system makes quads
    // silently invisible, which is a much worse first failure than a slightly wrong one.
    //
    // This used to collect into a `Vec` and then look those two up per entity, because the ECS had
    // no way to express an optional term. Q17 gave it one, and the columns are now resolved once per
    // archetype instead of once per quad.
    let mut collected: Vec<(i32, QuadInstance)> = world
        .query::<(
            &Transform,
            &Quad,
            Option<&SortOrder>,
            Option<&GlobalTransform>,
        )>()
        .map(|(_entity, (transform, quad, order, global))| {
            // `GlobalTransform` is what the entity's parents have made of its transform, so this is
            // where hierarchy finally reaches the screen. Falls back to the local `Transform` when
            // propagation has not run — correct for an unparented entity, and better than drawing
            // nothing at all for a game that forgot the system.
            let placement = match global {
                Some(global) => *global,
                None => GlobalTransform::from(local_matrix(transform)),
            };

            let matrix = placement.to_mat4();
            let translation = matrix.translation();

            // Scale and rotation are read back out of the composed matrix rather than off the local
            // transform, so a parent's scale and turn apply too. The columns of a transform matrix
            // are its scaled axes, so a column's length is that axis's total scale.
            let axis_x = [matrix.columns[0][0], matrix.columns[0][1]];
            let axis_y = [matrix.columns[1][0], matrix.columns[1][1]];
            let scale_x = axis_x[0].hypot(axis_x[1]);
            let scale_y = axis_y[0].hypot(axis_y[1]);

            (
                order.copied().unwrap_or_default().order,
                QuadInstance {
                    // The renderer is 2D; a transform is 3D (ADR 0018). Depth within a sort order is
                    // the pipeline decision Q3 deliberately left open, so z is dropped here rather
                    // than guessed at.
                    center: [translation[0], translation[1]],
                    size: [quad.size[0] * scale_x, quad.size[1] * scale_y],
                    // The angle of the composed x axis. Already in radians — the degrees an author
                    // wrote were converted when the matrix was built.
                    rotation: axis_x[1].atan2(axis_x[0]),
                    color: quad.color,
                },
            )
        })
        .collect();

    collected.sort_by_key(|(order, _)| *order);
    let quads: Vec<QuadInstance> = collected.into_iter().map(|(_, quad)| quad).collect();
    // Sprites are collected in the same pass rather than a separate system, so one frame is one
    // consistent read of the world. Two passes could see different worlds if anything ran between
    // them, and "the sprites are one frame behind the quads" is a miserable bug to find.
    let batches = collect_sprites(world);

    // The drawables are gathered once and then handed to each camera, rather than re-queried per
    // camera: what is in the world does not depend on who is looking at it, and re-collecting would
    // both cost more and open the door to two cameras disagreeing about one frame.
    let frame = FrameData {
        clear_color,
        views: active_cameras(world)
            .into_iter()
            .map(|(camera, eye)| View {
                camera,
                eye,
                quads: quads.clone(),
                batches: batches.clone(),
            })
            .collect(),
    };

    // Turn every texture id this frame names into pixels, before anything tries to draw with one.
    // Reads `Assets` and fills `TextureCache`; both are services, so neither can move the state
    // hash (ADR 0009).
    decode_frame_textures(world, &frame);

    world.with_service_taken::<Renderer, ()>(|world, renderer| {
        // The cache is read *inside* the taken-service closure because the renderer and the cache
        // are two entries in the same service map, and only one of them can be borrowed at a time.
        if let Some(cache) = world.service::<TextureCache>() {
            renderer.upload_frame_textures(&frame, cache);
        }
        renderer.render(&frame);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A world with a renderer and one camera entity.
    ///
    /// The camera is spawned rather than inserted as a resource since ADR 0031 -- which is exactly
    /// the migration these tests are here to keep honest.
    fn world_with_renderer() -> World {
        let mut world = World::new();
        world.insert_service(Renderer::new(Box::new(NullBackend::new(800, 600))));
        add_camera(&mut world, Camera::default(), [0.0, 0.0]);
        world
    }

    fn add_camera(world: &mut World, camera: Camera, at: [f32; 2]) -> amadeo_ecs::Entity {
        let entity = world.spawn();
        world.insert(entity, Transform::at(at[0], at[1]));
        world.insert(entity, camera);
        entity
    }

    fn add_quad(world: &mut World, x: f32, order: i32) -> amadeo_ecs::Entity {
        let entity = world.spawn();
        world.insert(entity, Transform::at(x, 0.0));
        world.insert(entity, Quad::new(1.0, 1.0, [1.0, 1.0, 1.0, 1.0]));
        world.insert(entity, SortOrder::new(order));
        entity
    }

    fn last_frame(world: &World) -> FrameData {
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
    fn collects_drawable_entities() {
        let mut world = world_with_renderer();
        add_quad(&mut world, 1.0, 0);
        add_quad(&mut world, 2.0, 0);

        render_quads(&mut world);
        assert_eq!(
            last_frame(&world)
                .primary()
                .expect("one camera")
                .quads
                .len(),
            2
        );
    }

    #[test]
    fn ignores_entities_missing_either_component() {
        let mut world = world_with_renderer();
        add_quad(&mut world, 1.0, 0);

        // A transform with no quad, and a quad with no transform: neither is drawable.
        let no_quad = world.spawn();
        world.insert(no_quad, Transform::at(9.0, 9.0));
        let no_transform = world.spawn();
        world.insert(no_transform, Quad::default());

        render_quads(&mut world);
        assert_eq!(
            last_frame(&world)
                .primary()
                .expect("one camera")
                .quads
                .len(),
            1
        );
    }

    #[test]
    fn a_child_is_drawn_where_its_parent_puts_it() {
        // The reason propagation exists, seen from the screen. Without reading GlobalTransform the
        // child would draw at its local (2, 0) rather than the (0, 2) its parent's quarter turn
        // puts it at.
        use amadeo_transform::{Parent, propagate_transforms};

        let mut world = world_with_renderer();

        let mut turned = Transform::default();
        turned.rotation[2] = 90.0;
        let parent = world.spawn();
        world.insert(parent, turned);

        let child = world.spawn();
        world.insert(child, Transform::at(2.0, 0.0));
        world.insert(child, Parent(parent));
        world.insert(child, Quad::new(1.0, 1.0, [1.0, 1.0, 1.0, 1.0]));

        propagate_transforms(&mut world);
        render_quads(&mut world);

        let drawn = last_frame(&world).primary().expect("one camera").quads[0];
        assert!(
            (drawn.center[0] - 0.0).abs() < 1e-5,
            "got {:?}",
            drawn.center
        );
        assert!(
            (drawn.center[1] - 2.0).abs() < 1e-5,
            "got {:?}",
            drawn.center
        );
    }

    #[test]
    fn a_parents_scale_reaches_the_quad_size() {
        use amadeo_transform::{Parent, propagate_transforms};

        let mut world = world_with_renderer();

        let parent = world.spawn();
        world.insert(
            parent,
            Transform {
                scale: [3.0, 3.0, 1.0],
                ..Transform::default()
            },
        );

        let child = world.spawn();
        world.insert(child, Transform::at(0.0, 0.0));
        world.insert(child, Parent(parent));
        world.insert(child, Quad::new(2.0, 2.0, [1.0, 1.0, 1.0, 1.0]));

        propagate_transforms(&mut world);
        render_quads(&mut world);

        let drawn = last_frame(&world).primary().expect("one camera").quads[0];
        assert!((drawn.size[0] - 6.0).abs() < 1e-5, "got {:?}", drawn.size);
    }

    #[test]
    fn a_quad_still_draws_without_propagation_having_run() {
        // The fallback. A game that never registers `propagate_transforms` should still see its
        // unparented entities, rather than a blank screen with no explanation.
        let mut world = world_with_renderer();
        let entity = world.spawn();
        world.insert(entity, Transform::at(4.0, -1.0));
        world.insert(entity, Quad::new(1.0, 1.0, [1.0, 1.0, 1.0, 1.0]));

        render_quads(&mut world);

        assert_eq!(
            last_frame(&world).primary().expect("one camera").quads[0].center,
            [4.0, -1.0]
        );
    }

    #[test]
    fn applies_transform_scale_to_quad_size() {
        let mut world = world_with_renderer();
        let entity = world.spawn();
        world.insert(
            entity,
            Transform {
                translation: [0.0, 0.0, 0.0],
                rotation: [0.0, 0.0, 90.0],
                scale: [2.0, 3.0, 1.0],
            },
        );
        world.insert(entity, Quad::new(1.0, 1.0, [1.0, 1.0, 1.0, 1.0]));

        render_quads(&mut world);
        let quad = last_frame(&world).primary().expect("one camera").quads[0];
        assert_eq!(quad.size, [2.0, 3.0]);
        // Authored in degrees (ADR 0018), handed to the backend in radians. The conversion happening
        // exactly once, here, is the thing this pins.
        assert!(
            (quad.rotation - std::f32::consts::FRAC_PI_2).abs() < 1e-6,
            "90 degrees should reach the backend as pi/2, got {}",
            quad.rotation
        );
    }

    #[test]
    fn sorts_by_sort_order() {
        let mut world = world_with_renderer();
        // Added out of layer order on purpose.
        add_quad(&mut world, 1.0, 5);
        add_quad(&mut world, 2.0, -3);
        add_quad(&mut world, 3.0, 0);

        render_quads(&mut world);
        let centers: Vec<f32> = last_frame(&world)
            .quads()
            .map(|quad| quad.center[0])
            .collect();
        assert_eq!(centers, vec![2.0, 3.0, 1.0]);
    }

    #[test]
    fn sort_is_stable_within_a_layer() {
        let mut world = world_with_renderer();
        for i in 0..5 {
            add_quad(&mut world, i as f32, 0);
        }

        render_quads(&mut world);
        let first: Vec<f32> = last_frame(&world).quads().map(|q| q.center[0]).collect();

        render_quads(&mut world);
        let second: Vec<f32> = last_frame(&world).quads().map(|q| q.center[0]).collect();

        assert_eq!(first, second, "draw order must be reproducible");
        assert_eq!(first, vec![0.0, 1.0, 2.0, 3.0, 4.0]);
    }

    #[test]
    fn rendering_does_not_change_simulation_state() {
        // Invariant I7 at the smallest scale: drawing must be invisible to the state hash.
        let mut world = world_with_renderer();
        add_quad(&mut world, 1.0, 0);

        let before = world.state_hash();
        for _ in 0..10 {
            render_quads(&mut world);
        }
        assert_eq!(world.state_hash(), before);
    }

    #[test]
    fn rendering_does_not_mark_components_changed() {
        // The reason the read-only pair query exists. A mutable query here would flag every drawn
        // entity as modified every frame, making change detection useless.
        let mut world = world_with_renderer();
        let entity = add_quad(&mut world, 1.0, 0);
        world.advance_tick();
        world.advance_tick();

        let before = world.changed_tick::<Transform>(entity);
        render_quads(&mut world);
        assert_eq!(world.changed_tick::<Transform>(entity), before);
    }

    #[test]
    fn a_world_with_no_camera_draws_nothing_rather_than_guessing() {
        // Before ADR 0031 this fell back to a default camera, which was the only sensible answer
        // when there could only ever be one. With cameras as entities, inventing one would draw a
        // view nobody authored -- so the frame is empty and the screen is merely cleared.
        let mut world = World::new();
        world.insert_service(Renderer::new(Box::new(NullBackend::new(640, 480))));
        add_quad(&mut world, 0.0, 0);

        render_quads(&mut world);
        assert!(last_frame(&world).views.is_empty());
        assert_eq!(last_frame(&world).quad_count(), 0);
    }

    #[test]
    fn rendering_without_a_renderer_is_harmless() {
        let mut world = World::new();
        add_quad(&mut world, 0.0, 0);
        render_quads(&mut world);
        assert!(!world.has_service::<Renderer>());
    }

    #[test]
    fn empty_world_still_renders_a_cleared_frame() {
        // "Nothing to draw" must still produce a frame, or the screen keeps the previous image.
        let mut world = world_with_renderer();
        render_quads(&mut world);

        let frame = last_frame(&world);
        assert_eq!(frame.quad_count(), 0);
        assert_eq!(
            frame.views.len(),
            1,
            "the camera is still there with nothing to see"
        );
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

    #[test]
    fn renderer_reports_its_backend_and_viewport() {
        let mut world = world_with_renderer();
        {
            let renderer = world.service::<Renderer>().expect("installed");
            assert_eq!(renderer.backend_name(), "null");
            assert_eq!(renderer.viewport(), (800, 600));
            assert!(renderer.last_error().is_none());
        }

        world
            .service_mut::<Renderer>()
            .expect("installed")
            .resize(1920, 1080);
        assert_eq!(
            world.service::<Renderer>().expect("installed").viewport(),
            (1920, 1080)
        );
    }

    #[test]
    fn headless_renderer_is_a_null_backend() {
        let renderer = Renderer::headless();
        assert_eq!(renderer.backend_name(), "null");
        assert!(renderer.null_backend().is_some());
    }
}
