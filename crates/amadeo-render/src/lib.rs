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
mod environment;
mod frustum;
#[cfg(feature = "gpu")]
mod gpu;
mod graph;
mod ibl;
mod mesh;
mod sprites;
mod textures;

pub use backend::{
    FrameData, LightData, MeshInstance, NullBackend, PunctualLight, QuadInstance, RenderBackend,
    RenderError, ShadowCascade, ShadowData, SpotShadow, SpriteBatch, SpriteInstance, View,
};
pub use components::{Camera, Projection, Quad, SortOrder, Sprite};
pub use describe::{
    DrawnEntity, DrawnKind, FrameDescription, describe_frame, describe_frame_through,
};
pub use environment::{Bloom, Environment, EnvironmentCache, Fog, Grade, Tonemap, Vignette};
pub use frustum::{Frustum, transformed_bounds};
#[cfg(feature = "gpu")]
pub use gpu::{AdapterDescription, WgpuBackend};
pub use ibl::{
    Cubemap, DEFAULT_SKY, EnvironmentMap, FACE_COUNT, IRRADIANCE_SIZE, SPECULAR_LEVELS,
    SPECULAR_SIZE, SkyCache, irradiance, prefilter_specular,
};
pub use mesh::{
    BoxMesh, DirectionalLight, GltfPart, MAX_PUNCTUAL_LIGHTS, MAX_SHADOW_LAYERS, MAX_SHADOW_SPOTS,
    Material, MaterialCache, Mesh, MeshCache, MeshData, PlaneMesh, PointLight, ShadowMode,
    SpotLight, Vertex,
};
pub use sprites::{COLLECT_SPRITES, collect_sprites};
pub use textures::{
    COLOR_SPACE_SETTING, LINEAR_COLOR_SPACE, PLACEHOLDER_TEXTURE_ID, TextureCache, TextureFailure,
    decode_frame_textures,
};
// Re-exported because a caller holding a `TextureCache` needs to talk about what is in it, and
// making them add `amadeo-image` to their own manifest for a type this crate hands them would be a
// dependency they did not choose.
pub use amadeo_image::{EncodeError, PixelFormat, TextureData, encode_png};

use amadeo_ecs::{Service, World};
use std::collections::{BTreeMap, BTreeSet};
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
    /// Which version of each mesh the backend is holding — the renderer's mirror of what is resident
    /// in video memory.
    ///
    /// Two jobs, and streaming needs both: spotting geometry that **changed** under a fixed id (a
    /// chunk that was dug), and spotting geometry the cache has **let go of**, so its buffers can be
    /// freed rather than accumulating for as long as the game runs.
    uploaded_meshes: BTreeMap<String, u64>,
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
            uploaded_meshes: BTreeMap::new(),
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

    /// The pixels of the most recently drawn frame.
    ///
    /// **The agent's eyes** (ADR 0021), and the only thing that checks what the GPU actually
    /// produced rather than what the world says should be produced. Backed by
    /// [`RenderBackend::capture`], so what it can answer depends on which backend is installed —
    /// only an offscreen wgpu one can, which is the one agent mode uses.
    ///
    /// # Errors
    ///
    /// [`RenderError::CaptureUnsupported`] when the installed backend cannot read its own output.
    /// The message names the backend and what answers the same question instead.
    pub fn capture(&mut self) -> Result<TextureData, RenderError> {
        self.backend.capture()
    }

    /// The backend as a [`NullBackend`], if that is what it is.
    ///
    /// Lets headless tests and CI assert on what *would* have been drawn without a GPU. Returns
    /// `None` for a real backend, which has nothing equivalent to offer.
    #[must_use]
    pub fn null_backend(&self) -> Option<&NullBackend> {
        self.backend_as::<NullBackend>()
    }

    /// The backend as a concrete type, if that is what is installed.
    ///
    /// The general form of [`Renderer::null_backend`], for the cases where a caller needs something
    /// only one backend can offer. GPU pass timing is the first: a null backend has no GPU to time,
    /// so the numbers live on `WgpuBackend` rather than on the trait, and a measurement harness has
    /// to be able to ask the thing it installed.
    ///
    /// `None` for any other backend, which is a caller asking a question that backend cannot answer
    /// rather than an error.
    #[must_use]
    pub fn backend_as<T: RenderBackend + 'static>(&self) -> Option<&T> {
        self.backend.as_any().downcast_ref::<T>()
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
        // Across every view, and across sprites *and* surfaces. Two cameras seeing one texture
        // upload it once, because the check below is against what the backend already holds rather
        // than against what this frame asked for — and a texture shared between a sprite and a
        // material is one upload for the same reason.
        let sprite_textures = frame.batches().map(|batch| batch.texture.as_str());
        // Both of a material's texture slots, for the same reason `decode_frame_textures` walks
        // both: an id that decodes and is never uploaded is as invisible as one that never decoded.
        let mesh_textures = frame
            .views
            .iter()
            .flat_map(|view| view.meshes.iter())
            .flat_map(|instance| {
                [
                    instance.material.base_colour_texture.as_str(),
                    instance.material.normal_texture.as_str(),
                    instance.material.metallic_roughness_texture.as_str(),
                ]
            })
            .filter(|id| !id.is_empty());

        for id in sprite_textures.chain(mesh_textures).collect::<Vec<&str>>() {
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

    /// Uploads the prefiltered sky each camera in this frame names, if the backend lacks it.
    ///
    /// No version and no placeholder tracking, unlike the two paths either side of this one. An
    /// environment map is built once from a file that does not change, so "does the backend have
    /// this id" is a complete question — where a texture can arrive late and geometry can be re-meshed
    /// under a fixed id.
    fn upload_frame_skies(&mut self, frame: &FrameData, cache: &SkyCache) {
        for view in &frame.views {
            let id = view.environment.sky.as_str();
            if id.is_empty() || self.backend.has_environment(id) {
                continue;
            }
            // Not ready yet is not an error: prefiltering happens on first sight and the frame that
            // asked draws with the neutral fallback. ADR 0021 applied to a sky.
            let Some(prefiltered) = cache.get(id) else {
                continue;
            };
            if let Err(error) = self.backend.upload_environment(id, prefiltered) {
                self.last_error = Some(error);
            }
        }
    }

    /// Uploads any geometry this frame needs that the backend does not already hold *at the current
    /// version*, and drops geometry the cache no longer has.
    ///
    /// # Why a version, when textures get by with a boolean
    ///
    /// Because geometry can now change under a fixed name. Every mesh in this engine used to be an
    /// asset loaded once at startup, so [`RenderBackend::has_mesh`] was a complete question. Terrain
    /// streaming broke that: **digging re-meshes a chunk under the same id**, and `has_mesh` would
    /// answer "yes, I have that" and keep the pre-dig geometry on screen — over a collider that had
    /// already changed. The player would walk into a tunnel that still looked like solid rock.
    ///
    /// [`MeshCache`] bumps a version on every write, so "is my copy stale" is two integers rather
    /// than a comparison of megabytes.
    fn upload_frame_meshes(&mut self, frame: &FrameData, cache: &MeshCache) {
        for view in &frame.views {
            for instance in &view.meshes {
                let Some(version) = cache.version_of(&instance.mesh) else {
                    continue;
                };
                if self.uploaded_meshes.get(&instance.mesh) == Some(&version) {
                    continue;
                }
                let Some(data) = cache.get(&instance.mesh) else {
                    continue;
                };
                match self.backend.upload_mesh(&instance.mesh, data) {
                    Ok(()) => {
                        self.uploaded_meshes.insert(instance.mesh.clone(), version);
                    }
                    Err(error) => self.last_error = Some(error),
                }
            }
        }

        self.evict_departed_meshes(cache);
    }

    /// Frees geometry the backend holds that the cache has let go of.
    ///
    /// # Why this is driven from what the *renderer* uploaded
    ///
    /// This looks like the pattern `docs/07` forbids — filtering by "what does the caller already
    /// have" — and it is worth saying why it is not the same thing. That rule governs the
    /// **deterministic outputs of a background system**: a list gameplay reads must not depend on
    /// what a thread pool finished. This is the renderer's private record of what is resident in
    /// video memory, it reaches nothing but drawing, and ADR 0041 §2 explicitly permits drawing to
    /// lag. A frame late in freeing a buffer is invisible; a frame late in a collider is not.
    ///
    /// The alternative — asking the backend to enumerate everything it holds — would allocate a list
    /// of ids every frame to answer a question the renderer already knows the answer to.
    fn evict_departed_meshes(&mut self, cache: &MeshCache) {
        let departed: Vec<String> = self
            .uploaded_meshes
            .keys()
            .filter(|id| cache.version_of(id).is_none())
            .cloned()
            .collect();

        for id in departed {
            self.backend.remove_mesh(&id);
            self.uploaded_meshes.remove(&id);
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
    let at = camera_matrix(world, entity).translation();
    [at[0], at[1]]
}

/// One camera entity's full world matrix, from `GlobalTransform` if propagation has run.
///
/// The identity for an entity with no transform at all, which is a camera nobody finished setting
/// up. Where [`camera_eye`] answers "where is it", this also answers "where is it *pointing*" —
/// which for a perspective camera decides what is on screen at least as much as its position does.
#[must_use]
pub fn camera_matrix(world: &World, entity: amadeo_ecs::Entity) -> Mat4 {
    match world.get::<GlobalTransform>(entity) {
        Some(global) => global.to_mat4(),
        None => match world.get::<Transform>(entity) {
            Some(transform) => local_matrix(transform),
            None => Mat4::IDENTITY,
        },
    }
}

/// The camera that draws first to the window, with its world position.
///
/// What `render.describe` answers for by default, and the nearest thing to "the camera" now that a
/// world may hold several. `None` when a world has none — a state worth distinguishing from a
/// default camera, which is why this returns an `Option` rather than falling back here.
/// **Orthographic only.** [`primary_view`] is the one that answers for either projection; this one
/// remains because a caller wanting a 2D camera specifically should not have to filter for it.
#[must_use]
pub fn primary_camera(world: &World) -> Option<(Camera, [f32; 2])> {
    active_cameras(world)
        .into_iter()
        .find(|(camera, ..)| {
            camera.target.is_empty() && matches!(camera.projection, Projection::Orthographic { .. })
        })
        .map(|(camera, eye, _)| (camera, eye))
}

/// The camera that draws first to the window, whatever its projection, with its world matrix.
///
/// # Why the whole matrix rather than a position
///
/// A 2D camera's answer is a translation, and reporting where it *is* was enough while
/// `render.describe` only knew quads and sprites. A perspective camera also has an **orientation** —
/// where it is pointing decides what is on screen at least as much as where it stands — and a
/// position alone cannot express that.
///
/// This is what closed **Q26**. Filtering by projection, as [`primary_camera`] does, is what made
/// `render.describe` answer about a *default orthographic camera that did not exist* when asked
/// about a 3D world: it reported a plausible camera and zero entities, which is worse than reporting
/// nothing at all, and it cost a session-13 debugging detour.
#[must_use]
pub fn primary_view(world: &World) -> Option<(Camera, Mat4)> {
    active_cameras(world)
        .into_iter()
        .find(|(camera, ..)| camera.target.is_empty())
        .map(|(camera, _, matrix)| (camera, matrix))
}

fn active_cameras(world: &World) -> Vec<(Camera, [f32; 2], Mat4)> {
    // **No longer filtered by projection.** Until the mesh pass existed, a perspective camera was
    // skipped because nothing could draw through one and guessing at a projection would have been
    // worse than drawing nothing. Now both kinds draw: an orthographic camera feeds the quad and
    // sprite passes, a perspective one feeds the mesh pass.
    let mut found: Vec<(i32, amadeo_ecs::Entity, Camera, [f32; 2], Mat4)> = world
        .query::<(&Camera, &Transform, Option<&GlobalTransform>)>()
        .filter(|(_, (camera, _, _))| camera.active)
        .map(|(entity, (camera, transform, global))| {
            let matrix = match global {
                Some(global) => global.to_mat4(),
                None => local_matrix(transform),
            };
            let at = matrix.translation();
            (camera.order, entity, camera.clone(), [at[0], at[1]], matrix)
        })
        .collect();

    found.sort_by_key(|(order, entity, ..)| (*order, entity.index(), entity.generation()));
    found
        .into_iter()
        .map(|(_, _, camera, eye, matrix)| (camera, eye, matrix))
        .collect()
}

/// Every point and spot light worth drawing for a camera at `eye`, nearest first.
///
/// # Nearest first, and capped
///
/// Every pixel evaluates every light in the list, so the list has a hard limit
/// ([`MAX_PUNCTUAL_LIGHTS`]). When a scene has more, *something* has to be dropped, and distance to
/// the camera is the honest cheap answer: a light across the level contributes least and is least
/// missed. Sorting is by distance from the eye to the light's **surface** — its position minus its
/// range — so a big lamp fifty metres away outranks a candle at thirty, which is what "affects this
/// view most" actually means.
///
/// **The cut is silent, deliberately.** A frame that quietly drops the ninth light is a lit scene
/// with a light missing, which an author can see; refusing to draw or logging every frame is worse.
/// The count is visible through `render.describe`, which is where a question about it gets answered.
///
/// # Determinism
///
/// The sort breaks ties by entity, so two lights at identical distances always come out in the same
/// order — an unstable sort over floats is exactly the kind of thing that makes one machine's frame
/// differ from another's. Rendering is outside the state hash (ADR 0031), so this is about a
/// reproducible *picture* rather than a reproducible simulation, and `render.describe` and a capture
/// are both worthless if it wobbles.
fn collect_punctual(world: &World, eye: [f32; 3], first_free_layer: u32) -> Vec<PunctualLight> {
    // Carried alongside each light so the sort below can keep it: a spot's *authored* settings are
    // needed to fit its shadow, and fitting has to happen after the sort, because a layer is only
    // assigned to the lights that survive the cut.
    let mut shadow_wanted: BTreeMap<amadeo_ecs::Entity, SpotLight> = BTreeMap::new();
    let mut found: Vec<(f32, amadeo_ecs::Entity, PunctualLight)> = Vec::new();

    let placement = |transform: &Transform, global: Option<&GlobalTransform>| match global {
        Some(global) => global.to_mat4(),
        None => local_matrix(transform),
    };
    let position_of = |matrix: &Mat4| {
        [
            matrix.columns[3][0],
            matrix.columns[3][1],
            matrix.columns[3][2],
        ]
    };

    for (entity, (light, transform, global)) in
        world.query::<(&PointLight, &Transform, Option<&GlobalTransform>)>()
    {
        if light.intensity <= 0.0 || light.range <= 0.0 {
            continue;
        }
        let matrix = placement(transform, global);
        found.push((
            0.0,
            entity,
            PunctualLight {
                position: position_of(&matrix),
                // Never read: the cone below admits the whole sphere.
                direction: [0.0, -1.0, 0.0],
                colour: [
                    light.colour[0] * light.intensity,
                    light.colour[1] * light.intensity,
                    light.colour[2] * light.intensity,
                ],
                range: light.range,
                cone_inner_cos: -1.0,
                cone_outer_cos: -1.0,
                // A point light's shadow is a cube — six faces and six passes — which ADR 0058
                // leaves out. Nothing here can cast.
                shadow: None,
            },
        ));
    }

    for (entity, (light, transform, global)) in
        world.query::<(&SpotLight, &Transform, Option<&GlobalTransform>)>()
    {
        if light.intensity <= 0.0 || light.range <= 0.0 {
            continue;
        }
        let matrix = placement(transform, global);
        // The third column is the entity's Z axis; light travels along its negative — the same
        // convention a camera looks along, so aiming a light is aiming a camera.
        let axis = matrix.columns[2];
        let length = (axis[0] * axis[0] + axis[1] * axis[1] + axis[2] * axis[2]).sqrt();
        let direction = if length < 1e-6 {
            [0.0, -1.0, 0.0]
        } else {
            [-axis[0] / length, -axis[1] / length, -axis[2] / length]
        };

        // Clamped so the outer cone is never tighter than the inner one. An author who swaps the two
        // gets a hard-edged beam rather than a divide by a negative width, which would invert the
        // falloff and light everything *outside* the cone.
        let outer = light.outer_angle.max(light.inner_angle);
        if light.shadows {
            shadow_wanted.insert(entity, *light);
        }
        found.push((
            0.0,
            entity,
            PunctualLight {
                position: position_of(&matrix),
                direction,
                colour: [
                    light.colour[0] * light.intensity,
                    light.colour[1] * light.intensity,
                    light.colour[2] * light.intensity,
                ],
                range: light.range,
                // Cosines, computed once here rather than per pixel per light in the shader — and
                // with the engine's own trigonometry (ADR 0053), so two machines agree.
                cone_inner_cos: amadeo_core::cos_degrees(light.inner_angle),
                cone_outer_cos: amadeo_core::cos_degrees(outer),
                // Filled in after the sort, for the ones near enough to survive the cut and to be
                // among the first `MAX_SHADOW_SPOTS` of those.
                shadow: None,
            },
        ));
    }

    for entry in &mut found {
        let light = &entry.2;
        let offset = [
            light.position[0] - eye[0],
            light.position[1] - eye[1],
            light.position[2] - eye[2],
        ];
        let distance =
            (offset[0] * offset[0] + offset[1] * offset[1] + offset[2] * offset[2]).sqrt();
        // To the light's reach rather than to its centre, so a large distant lamp beats a small near
        // one. Negative for a camera inside the light, which sorts it first, which is right.
        entry.0 = distance - light.range;
    }

    // `total_cmp` rather than `partial_cmp`: it is a total order over every float including NaN, so
    // there is no unwrap and no arm that cannot happen. The entity breaks ties.
    found.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.index().cmp(&b.1.index())));
    found.truncate(MAX_PUNCTUAL_LIGHTS);

    // Shadows are fitted **after** the sort and the cut, and layers hand out in that order — so the
    // nearest shadow-casting spot gets one and a distant one does not. Fitting before would waste
    // work on lights about to be dropped, and would assign layers to lights that never reach a
    // backend.
    let mut layer = first_free_layer;
    for (_, entity, light) in &mut found {
        if layer >= first_free_layer + MAX_SHADOW_SPOTS as u32 {
            break;
        }
        let Some(authored) = shadow_wanted.get(entity) else {
            continue;
        };
        light.shadow = fit_spot_shadow(authored, light.position, light.direction, layer);
        if light.shadow.is_some() {
            layer += 1;
        }
    }

    found.into_iter().map(|(_, _, light)| light).collect()
}

/// Fits one spot light's shadow map — ADR 0058.
///
/// # Why this is so much simpler than a cascade
///
/// A directional light has no position and no bound, so ADR 0055 has to invent one: a box centred on
/// the camera, snapped to a texel grid, split four ways because no single box covers both a
/// footprint and a horizon. A spot light bounds itself — it stands somewhere, it points somewhere,
/// and it stops at its range. So its shadow is exactly the view *from the light*, which is one
/// perspective matrix and no fitting at all.
///
/// The cone's outer angle is the field of view, doubled because a field of view is measured across
/// where a cone's angle is measured from its axis.
fn fit_spot_shadow(
    light: &SpotLight,
    position: [f32; 3],
    direction: [f32; 3],
    layer: u32,
) -> Option<SpotShadow> {
    // A light aimed straight up or down is parallel to the usual up axis, which collapses the basis
    // and rolls the map. Switching to a different reference axis there is the same fix a camera
    // looking straight down needs, and it is invisible in the result because a shadow map has no
    // orientation anyone can see.
    let up = if direction[1].abs() > 0.999 {
        [0.0, 0.0, 1.0]
    } else {
        [0.0, 1.0, 0.0]
    };
    let view = Mat4::look_along(position, direction, up);

    let far = light.range.max(0.1);
    // Near is derived rather than authored, to keep the component to the fields an author has an
    // opinion about. It has to be small enough not to clip geometry right in front of a torch and
    // large enough that clip depth keeps some precision — a hundredth of the range, floored, is the
    // usual compromise.
    let near = (far * 0.01).clamp(0.02, 0.5);

    // Doubled: a field of view spans the cone, where `outer_angle` is measured from its axis. Capped
    // just under a hemisphere, which is where a perspective projection stops being usable at all.
    let fov = (light.outer_angle.max(light.inner_angle) * 2.0).clamp(1.0, 175.0);
    let projection = Mat4::perspective(fov, 1.0, near, far);

    Some(SpotShadow {
        view_projection: projection.mul(&view),
        requested_resolution: light.shadow_resolution.clamp(16, 8192),
        layer,
        // **Divided through the range rather than through `far - near`**, which is what
        // `fit_cascade` does, and the difference is that this projection is *perspective*. Clip depth
        // is compressed towards the far plane, so a world-unit offset is a far larger share of it out
        // at the range than up close — and the range is where precision is worst and where acne shows
        // first, so that is the end to size the bias for.
        bias: light.shadow_bias / far.max(1e-6),
    })
}

/// Every directional light in the world, in a reproducible order.
///
/// A light's **direction** is its own negative Z axis, the same convention a camera looks along — so
/// aiming a light is aiming a camera, and a scene file needs no separate vocabulary for it.
///
/// Returns the authored component alongside the frame data, because fitting a shadow box needs the
/// authored distance and resolution and those are deliberately not carried on [`LightData`] — a
/// backend is handed a finished matrix, not the settings it came from.
fn active_lights(world: &World) -> Vec<(DirectionalLight, LightData)> {
    let mut found: Vec<(amadeo_ecs::Entity, DirectionalLight, LightData)> = world
        .query::<(&DirectionalLight, &Transform, Option<&GlobalTransform>)>()
        .filter(|(_, (light, _, _))| light.intensity > 0.0)
        .map(|(entity, (light, transform, global))| {
            let matrix = match global {
                Some(global) => global.to_mat4(),
                None => local_matrix(transform),
            };
            // The third column is the entity's Z axis; light travels along its negative.
            let axis = matrix.columns[2];
            let length = (axis[0] * axis[0] + axis[1] * axis[1] + axis[2] * axis[2]).sqrt();
            // A collapsed axis means a zero scale on Z. Falling back to straight down keeps the
            // light visible and obviously wrong rather than producing NaN across the whole image.
            let direction = if length < 1e-6 {
                [0.0, -1.0, 0.0]
            } else {
                [-axis[0] / length, -axis[1] / length, -axis[2] / length]
            };
            (
                entity,
                *light,
                LightData {
                    direction,
                    colour: [
                        light.colour[0] * light.intensity,
                        light.colour[1] * light.intensity,
                        light.colour[2] * light.intensity,
                    ],
                    // Filled in per camera by `fit_shadow`, because where the shadow map covers
                    // depends on where the camera is and lights are collected once for the frame.
                    shadow: None,
                },
            )
        })
        .collect();

    // By entity, so the order is total and reproducible (I3) rather than whatever iteration gave.
    found.sort_by_key(|(entity, ..)| (entity.index(), entity.generation()));
    found
        .into_iter()
        .map(|(_, light, data)| (light, data))
        .collect()
}

/// Works out the box one shadow map covers, for one light and one camera — ADR 0038.
///
/// Returns `None` when the light casts no shadows, which is what a backend reads as "draw no shadow
/// pass for this light".
///
/// # The box follows the camera, and is snapped to a fixed world grid
///
/// A directional light has no position, so there is nothing to centre a shadow map on except what
/// the viewer can see. Centring it on the camera is what keeps the resolution where it is looked at.
///
/// But a box that slides continuously with the camera makes every shadow edge **crawl**: each
/// shadow-map pixel covers a slightly different patch of world every frame, so edges fizz and swim
/// even when nothing in the scene is moving. The fix is to snap the box to a grid **anchored at the
/// world origin** rather than at the camera — snapping relative to the camera would be snapping to
/// something that moves, which is no snapping at all. A fixed world point then lands on exactly the
/// same shadow-map pixel until the box jumps a whole pixel, which is what
/// `a_shadow_box_moves_in_whole_texels` pins.
fn fit_shadow(
    light: &DirectionalLight,
    direction: [f32; 3],
    camera: [f32; 3],
) -> Option<ShadowData> {
    let count = light.shadows.map_count();
    if count == 0 {
        return None;
    }

    // One radius per cascade. `Orthogonal` is the degenerate case of the same arithmetic — a single
    // cascade covering the whole distance — rather than a separate path, so the two modes cannot
    // fit their boxes differently.
    let radii = match light.shadows {
        ShadowMode::Cascaded { blend } => cascade_radii(light.shadow_distance, blend),
        _ => [light.shadow_distance; CASCADE_COUNT],
    };

    let mut cascades = [ShadowCascade::default(); CASCADE_COUNT];
    for index in 0..count {
        cascades[index] = fit_cascade(light, direction, camera, radii[index])?;
    }

    Some(ShadowData {
        cascades,
        count,
        resolution: light.shadow_resolution.clamp(16, 8192),
    })
}

/// How many shadow maps a cascaded light uses.
///
/// **Four, fixed rather than authored.** Four is what nearly every engine ships. Making it a field
/// would mean a variable-length texture array and a variable loop bound in the shader, to buy
/// flexibility nothing has asked for — and adding a `cascade_count` defaulting to four later changes
/// no existing file, so this is not a door being closed.
pub const CASCADE_COUNT: usize = 4;

/// Where each cascade's coverage ends, nearest first, as radii around the camera.
///
/// # The two obvious schemes are both wrong, and the fix is to mix them
///
/// **Uniform** splits — equal slices — give the distant cascade a sensible share and starve the near
/// one, which is where detail is actually looked at. **Logarithmic** splits do the opposite: they
/// match how perspective actually compresses distance, and spend so little on the far cascade that it
/// covers almost nothing useful.
///
/// The standard answer, from NVIDIA's parallel-split shadow map work, is to compute both and
/// interpolate between them. `blend` is the weight, conventionally called lambda: `0.0` is purely
/// uniform, `1.0` purely logarithmic, and around `0.5` is the usual choice.
///
/// # Concentric, rather than fitted to the camera's frustum
///
/// Each cascade is a box **centred on the camera**, like the single-map case, just smaller for the
/// nearer ones. A tighter implementation fits each cascade to its slice of the view frustum, which
/// wastes less resolution — a concentric box covers the space *behind* the viewer too, which is
/// roughly half of it.
///
/// Concentric is chosen for the same reason ADR 0043 chose concentric boxes over an octree for chunk
/// residency: it is predictable, it needs no fitting logic, and it does not change when the camera
/// turns. A shadow scheme that reshapes itself as the viewer looks around is one where edges shift
/// for a reason nobody can see. The wasted resolution is a real cost and is the first thing to
/// revisit if cascades are not sharp enough.
#[must_use]
pub fn cascade_radii(shadow_distance: f32, blend: f32) -> [f32; CASCADE_COUNT] {
    // The near distance the splits are measured from. Not the camera's own near plane, which is
    // centimetres — a cascade that small would cover a few square metres and waste a whole map.
    const NEAR: f32 = 1.0;

    let far = shadow_distance.max(NEAR * 2.0);
    let blend = blend.clamp(0.0, 1.0);
    let mut radii = [0.0f32; CASCADE_COUNT];

    for (index, radius) in radii.iter_mut().enumerate() {
        // Fraction of the way through the cascades, 1-based: the last one always lands exactly on
        // `far`, whatever the blend does in between.
        let fraction = (index + 1) as f32 / CASCADE_COUNT as f32;

        // Even slices of distance.
        let uniform = NEAR + (far - NEAR) * fraction;
        // Even slices of *ratio*, which is what perspective actually compresses by. `powf` is a
        // transcendental and is fine here for ADR 0044's usual reason: this decides where a shadow
        // map goes, which is pixels, not where the ground is.
        let logarithmic = NEAR * (far / NEAR).powf(fraction);

        *radius = uniform + (logarithmic - uniform) * blend;
    }
    radii
}

/// Fits one shadow box of a given radius, snapped to its own texel grid.
///
/// Split out of [`fit_shadow`] because cascades need it four times at four radii. **The snapping has
/// to be per cascade and cannot be shared**: the grid it snaps to is one shadow-map texel wide, and a
/// cascade covering a quarter of the distance at the same resolution has texels a quarter the size.
/// Snapping every cascade to the largest one's grid would leave the near cascades crawling, which is
/// the exact artefact the snapping exists to remove.
fn fit_cascade(
    light: &DirectionalLight,
    direction: [f32; 3],
    camera: [f32; 3],
    radius: f32,
) -> Option<ShadowCascade> {
    let half = radius.max(0.1);
    let resolution = light.shadow_resolution.clamp(16, 8192);

    // How far back along the light's own direction the light "stands". Enough that anything within
    // the box's own radius above the camera still casts, which is the common case of a wall or a
    // roof between the sun and the floor.
    let back_off = half;
    let near = 0.0;
    let far = half + back_off;

    // Light space anchored at the *world origin*, so the grid below does not move with the camera.
    let anchored = Mat4::look_along([0.0, 0.0, 0.0], direction, [0.0, 1.0, 0.0]);
    let centre = anchored.project_point(camera)?;

    // One shadow-map pixel, in world units. Snapping the box's centre to a multiple of this is what
    // stops the crawl.
    let texel = (2.0 * half) / resolution as f32;
    let snapped_x = (centre[0] / texel).round() * texel;
    let snapped_y = (centre[1] / texel).round() * texel;

    // The anchored view has its eye at the origin, so its translation column is empty and can simply
    // be *set* rather than composed with another matrix. x and y move the box onto the snapped
    // centre; z pulls the eye back so the whole box is in front of it.
    let mut view = anchored;
    view.columns[3][0] = -snapped_x;
    view.columns[3][1] = -snapped_y;
    view.columns[3][2] = -(centre[2] + back_off);

    let projection = Mat4::orthographic(half, half, near, far);

    Some(ShadowCascade {
        view_projection: projection.mul(&view),
        // What the shader selects by. The radius asked for, not the clamped `half`: a cascade
        // reporting a shorter reach than it was given would leave a ring covered by nothing.
        far: radius,
        // The author writes a world-unit offset because that is the unit everything else in the
        // scene is in; the shader compares clip depths, which span `far - near` world units across
        // 0 to 1. Converting here means the field keeps its honest unit — **and it is what makes the
        // bias scale per cascade**, since a near cascade's depth range is a fraction of the far
        // one's and the same authored offset becomes a proportionally larger clip-space nudge.
        bias: light.shadow_bias / (far - near).max(1e-6),
    })
}

/// One mesh instance, with the world-space box that decides whether it can be seen.
///
/// The box is kept beside the instance rather than inside it because [`MeshInstance`] travels to the
/// backend, which has no use for it — culling has already happened by then. Private for the same
/// reason.
struct Drawable {
    instance: MeshInstance,
    min: [f32; 3],
    max: [f32; 3],
}

/// Every mesh worth drawing, in draw order.
///
/// An entity whose mesh id has not loaded is **skipped and not substituted** — see
/// [`MeshCache::get`](crate::MeshCache::get) for why a missing mesh has no honest stand-in where a
/// missing texture does.
fn collect_meshes(world: &World) -> Vec<Drawable> {
    let materials = world.service::<MaterialCache>();
    let meshes = world.service::<MeshCache>();

    let mut found: Vec<Drawable> = world
        .query::<(
            &Mesh,
            &Transform,
            Option<&SortOrder>,
            Option<&GlobalTransform>,
        )>()
        .filter_map(|(_entity, (mesh, transform, order, global))| {
            // Nothing to draw without geometry. Checked here rather than in the backend so that a
            // frame carries only what can actually be drawn, and `render.describe` agrees with it.
            let bounds = meshes?.get(&mesh.mesh)?.bounds()?;

            let model = match global {
                Some(global) => global.to_mat4(),
                None => local_matrix(transform),
            };
            let (min, max) = frustum::transformed_bounds(&model, bounds.0, bounds.1);

            Some(Drawable {
                instance: MeshInstance {
                    mesh: mesh.mesh.clone(),
                    model,
                    material: match materials {
                        Some(cache) => cache.get(&mesh.material),
                        None => Material::default(),
                    },
                    order: order.copied().unwrap_or_default().order,
                },
                min,
                max,
            })
        })
        .collect();

    // Stable, so entities sharing an order keep their (reproducible) iteration order — the same
    // rule quads and sprites follow.
    found.sort_by_key(|drawable| drawable.instance.order);
    found
}

/// The meshes one view can actually see — **M2.5's exit gate 3**.
///
/// # Why a shadow-casting light gets a vote
///
/// The obvious implementation tests the camera's frustum and nothing else, and it is **wrong in a way
/// that is easy to ship**: a mesh behind or beside the camera can still cast a shadow *into* view. Cull
/// it and its shadow disappears, which looks like a shadow-mapping bug and is a culling one.
///
/// The shadow pass draws from this same list (it is the same instance buffer with a different
/// pipeline), so anything inside the light's box has to survive. Keeping the union costs a few extra
/// meshes in the colour pass — they are off-screen, so they rasterise to nothing — and it keeps the
/// picture correct, which is the trade this makes deliberately.
fn visible_meshes(drawables: &[Drawable], frustum: &Frustum) -> Vec<MeshInstance> {
    drawables
        .iter()
        .filter(|drawable| frustum.intersects_aabb(drawable.min, drawable.max))
        .map(|drawable| drawable.instance.clone())
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

    // Each camera's `environment` id becomes the look itself here, so the backend is handed
    // everything it needs and never reaches back into the world. A world with no `EnvironmentCache`
    // installed — which is every game that has not asked for post-processing — gets the default
    // look, exactly as an unresolved id does (ADR 0021).
    let looks = world.service::<EnvironmentCache>();
    let resolve = |camera: &Camera| match looks {
        Some(cache) => cache.get(&camera.environment),
        None => Environment::default(),
    };

    // The drawables are gathered once and then handed to each camera, rather than re-queried per
    // camera: what is in the world does not depend on who is looking at it, and re-collecting would
    // both cost more and open the door to two cameras disagreeing about one frame.
    let meshes = collect_meshes(world);
    let lights = active_lights(world);

    // The aspect ratio the frustum is built with has to be the one the backend draws with, or
    // culling and drawing disagree about where the left and right planes are — which trims a strip
    // off one edge of the screen and looks like a clipping bug.
    let viewport = world
        .service::<Renderer>()
        .map_or((1280, 720), Renderer::viewport);

    let frame = FrameData {
        clear_color,
        views: active_cameras(world)
            .into_iter()
            .map(|(camera, eye, eye_matrix)| {
                // **A camera's projection selects which pass it feeds**, which is ADR 0031's "two
                // passes, neither built on the other" reaching the collection stage. An orthographic
                // camera feeds the quad and sprite passes; a perspective one feeds the mesh pass.
                //
                // So a single camera does not draw both, and that is not a limitation to work
                // around: 2D over a 3D world is a *second* camera at a higher order, which is the
                // answer ADR 0031 already gave and the mechanism a HUD already uses.
                let flat = matches!(camera.projection, Projection::Orthographic { .. });

                // Shadows are fitted per camera, because where the map covers depends on where the
                // camera is. Only for a 3D camera: a 2D one draws no meshes, so a shadow map for it
                // would be a full extra pass rendering nothing.
                //
                // **At most one light casts.** Every extra shadow-casting light is another full pass
                // over the scene, and choosing between a loop in the shader and a pass per light is
                // the same open question `amadeo-render` already has about lighting in general —
                // answering it here, for shadows only, would be answering it in the wrong place.
                let mut shadowed = false;
                let view_lights: Vec<LightData> = lights
                    .iter()
                    .map(|(light, data)| {
                        if flat || shadowed {
                            return *data;
                        }
                        let fitted = fit_shadow(light, data.direction, eye_matrix.translation());
                        shadowed |= fitted.is_some();
                        LightData {
                            shadow: fitted,
                            ..*data
                        }
                    })
                    .collect();

                // Spot shadows take the layers **after** the directional light's cascades, because
                // the two share one texture array — see `View::shadow_atlas`. Collected before the
                // culling below, which has to know where every shadow-casting light looks.
                let after_cascades = view_lights
                    .iter()
                    .find_map(|light| light.shadow)
                    .map_or(0, |shadow| shadow.count as u32);
                let punctual = if flat {
                    Vec::new()
                } else {
                    collect_punctual(world, eye_matrix.translation(), after_cascades)
                };

                // Culled per camera, because what can be seen depends entirely on who is looking.
                // A 2D camera draws no meshes at all, so it skips the work rather than culling
                // everything away one box at a time.
                // Two lists, each culled against what actually needs it: the colour pass draws what
                // the camera can see, the shadow pass draws what the light can. See
                // `View::shadow_casters` for why one list holding the union does not work.
                let (visible, casters) = if flat {
                    (Vec::new(), Vec::new())
                } else {
                    let aspect = viewport.0 as f32 / viewport.1.max(1) as f32;
                    let projection = match camera.projection {
                        Projection::Perspective { fov, near, far } => {
                            Mat4::perspective(fov, aspect, near, far)
                        }
                        Projection::Orthographic { height } => {
                            let half = height / 2.0;
                            Mat4::orthographic(half * aspect, half, -1000.0, 1000.0)
                        }
                    };
                    let view_projection =
                        projection.mul(&eye_matrix.inverse_rigid().unwrap_or(Mat4::IDENTITY));

                    // **One caster list, culled to everything that casts.**
                    //
                    // For the sun that is its *largest* cascade, which contains all the others, so
                    // one list serves four passes; culling per cascade would be tighter and would
                    // mean four lists to save draws each cascade's own projection already clips.
                    //
                    // Spot lights then **widen** it, and getting that wrong is a bug worth naming:
                    // this list used to come from the directional light alone, so a scene lit only
                    // by a torch produced an empty caster list, every shadow pass cleared its layer
                    // and drew nothing, and every surface came out fully lit. A shadow map with
                    // nothing in it does not look broken — it looks like no shadows.
                    let mut volumes: Vec<Frustum> = Vec::new();
                    if let Some(shadow) = view_lights.iter().find_map(|light| light.shadow) {
                        let widest = shadow.cascades[shadow.count.saturating_sub(1)];
                        volumes.push(Frustum::from_view_projection(&widest.view_projection));
                    }
                    volumes.extend(
                        punctual
                            .iter()
                            .filter_map(|light| light.shadow)
                            .map(|spot| Frustum::from_view_projection(&spot.view_projection)),
                    );

                    // A mesh casts if **any** of them can see it. The union is deliberately loose:
                    // a pass whose own light cannot see a mesh clips it anyway, so the cost of a
                    // generous list is a few vertices and the cost of a tight one is a missing
                    // shadow.
                    let casters = if volumes.is_empty() {
                        Vec::new()
                    } else {
                        meshes
                            .iter()
                            .filter(|drawable| {
                                volumes.iter().any(|volume| {
                                    volume.intersects_aabb(drawable.min, drawable.max)
                                })
                            })
                            .map(|drawable| drawable.instance.clone())
                            .collect()
                    };

                    (
                        visible_meshes(&meshes, &Frustum::from_view_projection(&view_projection)),
                        casters,
                    )
                };

                View {
                    environment: resolve(&camera),
                    camera,
                    eye,
                    eye_matrix,
                    quads: if flat { quads.clone() } else { Vec::new() },
                    batches: if flat { batches.clone() } else { Vec::new() },
                    meshes: visible,
                    shadow_casters: casters,
                    lights: view_lights,
                    // Empty for a 2D view: a sprite has no normal and no position in depth, so
                    // nothing about it could respond to a light at a place. The same reason `flat`
                    // skips shadow fitting above.
                    punctual,
                }
            })
            .collect(),
    };

    // Views contributed by a layer this crate cannot see. `amadeo-ui` is the only filler today.
    let mut frame = frame;
    take_overlay_views(world, &mut frame);

    // Turn every texture id this frame names into pixels, before anything tries to draw with one.
    // Reads `Assets` and fills `TextureCache`; both are services, so neither can move the state
    // hash (ADR 0009).
    decode_frame_textures(world, &frame);
    // And every sky, which is the same idea one step more expensive: decode, project onto a cube,
    // and convolve twice. Cached, so this is free after the first frame that names an id.
    prefilter_frame_skies(world, &frame);

    world.with_service_taken::<Renderer, ()>(|world, renderer| {
        // The cache is read *inside* the taken-service closure because the renderer and the cache
        // are two entries in the same service map, and only one of them can be borrowed at a time.
        if let Some(cache) = world.service::<TextureCache>() {
            renderer.upload_frame_textures(&frame, cache);
        }
        if let Some(cache) = world.service::<MeshCache>() {
            renderer.upload_frame_meshes(&frame, cache);
        }
        if let Some(cache) = world.service::<SkyCache>() {
            renderer.upload_frame_skies(&frame, cache);
        }
        renderer.render(&frame);
    });
}

/// Moves any [`Overlay`] views into the frame, in camera order, and empties the service.
///
/// # Why it drains rather than reads
///
/// **A stale interface frozen on screen is worse than none.** If the system that fills the overlay
/// stops running — it was removed, it errored, a menu was torn down — reading would leave the last
/// frame's menu painted over the game forever, and it would look like a rendering bug rather than a
/// missing system. Draining means one frame without a filler is one frame without an overlay.
fn take_overlay_views(world: &mut World, frame: &mut FrameData) {
    let Some(overlay) = world.service_mut::<Overlay>() else {
        return;
    };
    if overlay.views.is_empty() {
        return;
    }

    frame.views.append(&mut overlay.views);
    // **Stable**, so two views at one order keep the order they were added in — and so an overlay is
    // *ordered against* the world's cameras rather than simply stapled after them. A HUD under a
    // transition wipe is then a matter of two numbers, not of who ran first.
    frame.views.sort_by_key(|view| view.camera.order);
}

/// Views contributed by a layer above this crate.
///
/// # Why the slot lives here and is filled from above
///
/// `amadeo-ui` sits *above* `amadeo-render` (invariant I6), so this crate cannot name a `UiNode` and
/// `render_quads` cannot go looking for one. The inversion is the same one `TextureCache`,
/// `MeshCache` and `SkyCache` already use: **the renderer owns the slot and something higher up puts
/// things in it.**
///
/// The alternative was to let a higher layer build and submit its own `FrameData`, which would mean
/// two submissions per frame and two present passes fighting over one surface.
///
/// A [`Service`], so nothing here can reach the state hash (ADR 0009) — which matters, because what
/// is in it depends on the window size.
#[derive(Debug, Default)]
pub struct Overlay {
    /// Views to draw this frame, each with its own camera. Emptied by [`render_quads`].
    pub views: Vec<View>,
}

impl Service for Overlay {}

/// Prefilters every sky this frame's cameras name, if it is not prefiltered already.
///
/// Split out for the same reason [`decode_frame_textures`] is: it needs [`SkyCache`] mutably and
/// `Assets` shared, which are two entries in one service map, so the cache is taken out of the world
/// for the duration.
///
/// Does nothing if either service is absent — a headless test that installed no asset system still
/// renders, with the neutral sky.
fn prefilter_frame_skies(world: &mut World, frame: &FrameData) {
    if !world.has_service::<SkyCache>() {
        return;
    }
    world.with_service_taken::<SkyCache, ()>(|world, cache| {
        let Some(assets) = world.service::<amadeo_assets::Assets>() else {
            return;
        };
        for view in &frame.views {
            cache.ensure(&view.environment.sky, assets);
        }
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
