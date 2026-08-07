//! The rendering backend abstraction, and the null backend every build must have.

use crate::components::Camera;
use amadeo_image::TextureData;
use std::fmt;

/// What can go wrong while rendering.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum RenderError {
    /// The surface could not be acquired this frame.
    ///
    /// Usually transient — a resize in progress, or a minimised window. Callers should skip the
    /// frame rather than treat it as fatal.
    #[error(
        "render surface unavailable this frame ({reason}); skip the frame and try the next one"
    )]
    SurfaceUnavailable {
        /// What the backend reported.
        reason: String,
    },

    /// The backend could not be created at all.
    #[error("could not initialise the {backend} backend: {reason}")]
    InitFailed {
        /// Which backend failed.
        backend: &'static str,
        /// Why.
        reason: String,
    },

    /// The frame plan the renderer built for itself does not hold together.
    ///
    /// **Always an engine bug rather than anything content can cause** — ADR 0034 keeps the render
    /// graph internal, so no game, scene file or asset can declare a pass. It is a typed error
    /// anyway, because the alternative to reporting it is drawing a wrong picture with nothing to
    /// read.
    #[error(
        "the renderer built an invalid frame plan: {reason}. This is an engine bug — please report it"
    )]
    GraphInvalid {
        /// What the graph objected to.
        reason: String,
    },

    /// This backend cannot hand back the pixels it drew.
    ///
    /// Not a failure so much as a fact about the backend, which is why it says which one and what to
    /// use instead. `NullBackend` draws nothing at all; a *windowed* wgpu backend draws into a
    /// swapchain image that is not readable.
    #[error("{backend} cannot capture: {reason}")]
    CaptureUnsupported {
        /// Which backend was asked.
        backend: &'static str,
        /// Why not, and what answers the same question instead.
        reason: String,
    },
}

/// One quad, flattened into what a GPU needs.
///
/// Produced by the collection pass from [`Transform`](amadeo_transform::Transform) and
/// [`Quad`](crate::Quad). Kept deliberately flat and `Copy` so it can be uploaded to a buffer
/// without further transformation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct QuadInstance {
    /// World-space centre.
    pub center: [f32; 2],
    /// Full world-space size, after the transform's scale.
    pub size: [f32; 2],
    /// Rotation in radians.
    pub rotation: f32,
    /// Linear RGBA.
    pub color: [f32; 4],
}

/// One textured sprite, flattened into what a GPU needs.
///
/// Deliberately holds no texture id: a sprite's texture is decided by which [`SpriteBatch`] it is
/// in, so it does not need repeating twenty thousand times. That is the whole point of batching.
///
/// # Why this carries axes rather than a size and an angle
///
/// [`QuadInstance`] stores `size` plus `rotation`, which means the collection pass has to *decompose*
/// the transform matrix — two `hypot` calls and an `atan2` per instance — and the shader then has to
/// recompose it with a sine and a cosine. That is a round trip through trigonometry to recover
/// numbers the matrix already contained.
///
/// Storing the matrix's linear part instead removes both ends of it. Measured at 20,000 sprites,
/// dropping the decomposition was worth roughly a third of the batcher's total cost
/// (`tests/sprite_throughput.rs`).
///
/// It is also strictly more expressive: a size-and-angle pair cannot represent a sheared or
/// non-uniformly scaled-then-rotated sprite, so the decomposition was quietly lossy for any entity
/// whose parent scaled it on one axis and turned it.
///
/// `QuadInstance` keeps its older shape for now because the wgpu backend already renders it and
/// nothing was broken; it should follow when that backend is next touched.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpriteInstance {
    /// World-space centre.
    pub center: [f32; 2],
    /// The sprite's two full-extent axes in world space, as `[x_axis, y_axis]`.
    ///
    /// Each is the transform matrix's corresponding column scaled by the sprite's size, so it points
    /// along that edge and its length *is* that edge's world-space length. For an unrotated sprite
    /// they are `[width, 0]` and `[0, height]`.
    ///
    /// A corner at local `(u, v)`, with `u` and `v` in `-0.5..=0.5`, sits at
    /// `center + axes[0] * u + axes[1] * v`. That is the same convention [`QuadInstance::size`]
    /// uses — a full size, halved by the corner offsets — and `sprite.wgsl` builds its four corners
    /// exactly that way.
    pub axes: [[f32; 2]; 2],
    /// Linear RGBA tint.
    pub color: [f32; 4],
    /// Sub-rectangle of the texture, `[x, y, width, height]` in `0.0..=1.0`.
    pub region: [f32; 4],
}

impl SpriteInstance {
    /// The sprite's world-space width and height.
    ///
    /// Recovered from the axes, so it costs two square roots — for diagnostics and tests, not for
    /// the render path, which is precisely why the axes are what gets stored.
    #[must_use]
    pub fn size(&self) -> [f32; 2] {
        [
            self.axes[0][0].hypot(self.axes[0][1]),
            self.axes[1][0].hypot(self.axes[1][1]),
        ]
    }

    /// The sprite's rotation in radians, from the angle of its x axis.
    ///
    /// As with [`SpriteInstance::size`], this is for looking at rather than for drawing.
    #[must_use]
    pub fn rotation(&self) -> f32 {
        self.axes[0][1].atan2(self.axes[0][0])
    }
}

/// One mesh to draw, flattened into what a GPU needs.
///
/// Carries the material **by value** rather than by id, unlike [`SpriteBatch::texture`]. The reason
/// is the difference between the two: a texture's pixels are megabytes and belong on the device once
/// (see [`RenderBackend::upload_texture`]), while a material is five numbers and a string, and
/// resolving it once here means the backend never reaches back into the world for it.
///
/// The geometry stays an **id**, because that *is* megabytes and follows the texture rule exactly.
#[derive(Debug, Clone, PartialEq)]
pub struct MeshInstance {
    /// The declared asset id of the geometry (ADR 0020).
    pub mesh: String,
    /// Where the mesh sits in the world, from its `GlobalTransform`.
    pub model: amadeo_transform::Mat4,
    /// What the surface is made of, already resolved from its id (ADR 0033).
    pub material: crate::Material,
    /// The sort order this instance belongs to. Absent means zero.
    pub order: i32,
}

/// A light with a direction but no position — the sun, or the moon.
///
/// **An entity, following ADR 0031's precedent for the camera**: a world may hold any number, a
/// scene file authors them, and parenting one to something is how it follows.
///
/// Direction rather than position is what makes it *directional*: every surface in the world is lit
/// from the same angle, which is what distant light looks like and is far cheaper than a light that
/// falls off with distance. Point lights are still to come.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LightData {
    /// The direction the light **travels**, normalised. `[0, -1, 0]` is straight down.
    pub direction: [f32; 3],
    /// Linear RGB, already multiplied by intensity.
    pub colour: [f32; 3],
    /// How this light casts shadows, if it does (ADR 0038).
    ///
    /// `None` for a light with [`ShadowMode::Off`](crate::ShadowMode), and also for one whose
    /// direction and camera position could not produce a usable matrix. Both mean the same thing to
    /// a backend — draw no shadow pass for this light — which is why they collapse into one
    /// `Option` rather than being distinguished here.
    pub shadow: Option<ShadowData>,
}

/// Everything a backend needs to render and sample one shadow map — ADR 0038.
///
/// The matrix is computed here rather than in the backend for the same reason a view's is: a backend
/// should be handed everything it needs and never reach back into the world. It also means
/// `NullBackend` can report what *would* have been rendered, so a shadow-fitting bug is catchable
/// with no GPU.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShadowData {
    /// World space to the light's clip space: what the shadow pass draws with, and what the mesh
    /// pass transforms each pixel by to look its depth up.
    pub view_projection: amadeo_transform::Mat4,
    /// How many pixels across the shadow map is.
    pub resolution: u32,
    /// How far to push a depth comparison away from the surface, in the light's clip depth.
    ///
    /// Already converted out of world units by dividing through the depth range the light's
    /// projection covers, because the shader compares clip depths and the author writes world ones.
    pub bias: f32,
}

/// A run of sprites sharing one texture, drawn in one call.
///
/// A batch is the unit of work a backend turns into a draw call. Binding a texture is the expensive
/// state change in 2D rendering, so the number of batches — not the number of sprites — is what
/// decides whether a frame is fast.
#[derive(Debug, Clone, PartialEq)]
pub struct SpriteBatch {
    /// The declared asset id of the texture every instance in this batch uses (ADR 0020).
    pub texture: String,
    /// The sort order this batch belongs to. Batches are emitted in ascending order.
    pub order: i32,
    /// The sprites, in draw order.
    pub instances: Vec<SpriteInstance>,
}

/// What one camera contributes to a frame.
///
/// A frame is a list of these since ADR 0031, because a world may hold any number of cameras. A view
/// is one camera's settings, resolved position, and the drawables it saw — everything a backend needs
/// to draw that camera's pass without going back to the world.
#[derive(Debug, Clone, PartialEq)]
pub struct View {
    /// The camera's settings, copied off its entity.
    pub camera: Camera,
    /// The camera's world position, taken from the `Transform` on the same entity.
    ///
    /// Separate from [`View::camera`] because a `Camera` deliberately holds no position: ADR 0018
    /// keeps that on the transform, so that parenting a camera to a character is a follow camera.
    pub eye: [f32; 2],
    /// The look this camera asked for, already resolved from its id (ADR 0034).
    ///
    /// Resolved here rather than in the backend for the same reason a frame holds no pixels: a
    /// backend should be handed everything it needs to draw and never have to reach back into the
    /// world. A camera whose environment has not loaded, or which named none, carries
    /// [`Environment::default`](crate::Environment) — which does nothing, so there is no "no look"
    /// case to branch on.
    pub environment: crate::Environment,
    /// The camera's full world transform.
    ///
    /// Needed by the mesh pass, where [`View::eye`]'s two numbers are not enough — a 3D camera has
    /// an orientation, and its view matrix is the inverse of this
    /// ([`Mat4::inverse_rigid`](amadeo_transform::Mat4::inverse_rigid)).
    ///
    /// The **projection** is deliberately not here. It needs the target's aspect ratio, which only
    /// the backend knows, and computing it in two places is how the mesh pass and the sprite pass
    /// would end up disagreeing about what a camera sees.
    pub eye_matrix: amadeo_transform::Mat4,
    /// Quads to draw, already sorted by [`SortOrder`](crate::SortOrder).
    pub quads: Vec<QuadInstance>,
    /// Textured sprites, grouped into draw calls and ordered by [`SpriteBatch::order`].
    pub batches: Vec<SpriteBatch>,
    /// Meshes to draw, already sorted by [`SortOrder`](crate::SortOrder).
    pub meshes: Vec<MeshInstance>,
    /// Directional lights affecting this view.
    ///
    /// On the view rather than the frame because a camera rendering to a texture may one day want
    /// its own lighting, and because everything else a backend needs to draw one pass already lives
    /// here — reaching up to the frame for lights would be the one exception.
    pub lights: Vec<LightData>,
}

/// Everything needed to draw one frame.
///
/// Built by reading the world, then handed to a backend. Nothing in here borrows the world, which is
/// what keeps rendering strictly read-only with respect to simulation (ADR 0005).
#[derive(Debug, Clone, PartialEq)]
pub struct FrameData {
    /// Background colour, linear RGBA.
    pub clear_color: [f32; 4],
    /// One per active camera, **already in `Camera::order`**, low to high.
    ///
    /// Empty when a world has no camera, which draws a cleared screen rather than failing. That is
    /// deliberate: a world under construction has no camera yet, and a hard error there would make
    /// the first frame of every new game a crash.
    pub views: Vec<View>,
}

impl FrameData {
    /// The first view, if there is one.
    ///
    /// A convenience for the common single-camera case and for tests, so they do not all have to
    /// write `frame.views.first()`. Named rather than indexed because "the camera" stopped being a
    /// meaningful phrase once there could be several — this is *a* view, the one drawn first.
    #[must_use]
    pub fn primary(&self) -> Option<&View> {
        self.views.first()
    }

    /// The look the post pass should apply to this frame.
    ///
    /// # Why one environment for a frame that may have several cameras
    ///
    /// ADR 0031 has every camera compose into **one** image — a HUD camera loads what the world
    /// camera left rather than clearing — so by the time post-processing runs there is one picture
    /// and the cameras are no longer separable. This takes the first view's environment, which is
    /// the same "which camera when there are several" rule ADR 0031 gave `render.describe`.
    ///
    /// **The limitation is real and recorded as Q23**: a HUD camera cannot have a different grade
    /// from the world beneath it. Fixing it means per-camera targets, which is the same work as
    /// `Camera::target`, so the two belong together rather than being solved twice.
    #[must_use]
    pub fn look(&self) -> crate::Environment {
        self.primary()
            .map_or_else(crate::Environment::default, |view| view.environment)
    }

    /// Every quad across every view.
    ///
    /// Two cameras looking at one quad report it twice, which is correct: it is drawn twice.
    pub fn quads(&self) -> impl Iterator<Item = &QuadInstance> {
        self.views.iter().flat_map(|view| view.quads.iter())
    }

    /// Every sprite batch across every view.
    pub fn batches(&self) -> impl Iterator<Item = &SpriteBatch> {
        self.views.iter().flat_map(|view| view.batches.iter())
    }

    /// How many quads are in the frame, across every view.
    #[must_use]
    pub fn quad_count(&self) -> usize {
        self.views.iter().map(|view| view.quads.len()).sum()
    }

    /// How many sprites are in the frame, across every batch of every view.
    #[must_use]
    pub fn sprite_count(&self) -> usize {
        self.batches().map(|batch| batch.instances.len()).sum()
    }

    /// How many draw calls the sprite batches will cost.
    ///
    /// The number worth watching: sprites are cheap and state changes are not. Counted across views,
    /// because two cameras drawing the same world cost two sets of draw calls.
    #[must_use]
    pub fn batch_count(&self) -> usize {
        self.batches().count()
    }
}

impl Default for FrameData {
    fn default() -> Self {
        Self {
            // A dark neutral that is clearly not black, so "nothing rendered" and "cleared but empty"
            // are distinguishable at a glance.
            clear_color: [0.06, 0.07, 0.09, 1.0],
            views: Vec::new(),
        }
    }
}

/// Something that can draw a frame.
///
/// # Every build has a null backend
///
/// Invariant I7 requires the whole engine to run with no window and no GPU. That is not a debugging
/// convenience — it is how the agent verifies games, how CI runs, and how a dedicated server will
/// work later (ADR 0006). So [`NullBackend`] is part of the engine, not a test fixture, and the
/// determinism suite asserts that a null-backed run and a GPU-backed run reach identical simulation
/// state.
pub trait RenderBackend: fmt::Debug + Send + Sync {
    /// Upcast, so a caller can recover the concrete backend type.
    ///
    /// Same pattern as component columns and services elsewhere in the engine: a trait object cannot
    /// be downcast directly, so it is turned into `&dyn Any` first. Used by
    /// `Renderer::null_backend` to let headless tests assert on what would have been drawn.
    fn as_any(&self) -> &dyn std::any::Any;

    /// A short name for diagnostics.
    fn name(&self) -> &'static str;

    /// Current drawable size in physical pixels.
    fn viewport(&self) -> (u32, u32);

    /// Tells the backend the drawable size changed.
    fn resize(&mut self, width: u32, height: u32);

    /// Whether the backend already holds this texture and can draw a batch naming it.
    ///
    /// # Why textures are pushed to the backend rather than carried in the frame
    ///
    /// A [`FrameData`] holds texture *ids*, not pixels, and that is deliberate: a frame is rebuilt
    /// every tick, and copying several megabytes of decoded image into it sixty times a second to
    /// draw the same wall would be the most expensive thing the renderer did.
    ///
    /// So pixels travel once, out of band, through [`RenderBackend::upload_texture`], and the
    /// backend keeps whatever GPU state it needs keyed by id. This method is how the caller knows
    /// whether that has happened yet.
    fn has_texture(&self, id: &str) -> bool;

    /// Hands the backend a decoded texture to hold under `id`, replacing any earlier one.
    ///
    /// Replacing rather than refusing is what makes a late arrival work: a texture that fell back to
    /// a placeholder on frame one and decoded on frame ten is uploaded again, and the sprites using
    /// it change without anything being respawned. ADR 0021 explicitly permits that.
    ///
    /// # Errors
    ///
    /// [`RenderError`] if the backend could not take it — typically out of video memory, or a size
    /// beyond what the device supports.
    fn upload_texture(&mut self, id: &str, texture: &TextureData) -> Result<(), RenderError>;

    /// Whether the backend already holds this geometry and can draw an instance naming it.
    ///
    /// The same push-rather-than-carry rule [`RenderBackend::upload_texture`] explains: a
    /// [`MeshInstance`] holds an *id*, because vertices are megabytes and a frame is rebuilt sixty
    /// times a second.
    fn has_mesh(&self, _id: &str) -> bool {
        false
    }

    /// Hands the backend geometry to hold under `id`, replacing any earlier version.
    ///
    /// # Errors
    ///
    /// [`RenderError`] if the backend could not take it — typically out of video memory.
    fn upload_mesh(&mut self, _id: &str, _mesh: &crate::MeshData) -> Result<(), RenderError> {
        Ok(())
    }

    /// Drops the geometry held under `id`, freeing whatever it cost.
    ///
    /// # Why this exists at all, when [`RenderBackend::upload_texture`] has no counterpart
    ///
    /// Because geometry became *transient*. Every mesh in this engine used to be an asset loaded at
    /// startup, so a backend accumulating them until exit was accumulating a fixed set. Chunked
    /// terrain streaming (ADR 0043) is the first thing that produces geometry the world will later
    /// stop wanting — walk far enough in one direction and, without this, video memory grows for as
    /// long as the game runs.
    ///
    /// Removing an id the backend does not hold is **not an error**, and callers rely on that: the
    /// terrain streamer reports every chunk leaving the drawn region, including ones whose mesh never
    /// arrived and ones that were empty. Making that list conditional on what had been delivered is
    /// the defect `docs/07` warns about, so the removal is made harmless instead.
    fn remove_mesh(&mut self, _id: &str) {}

    /// Draws one frame.
    fn render(&mut self, frame: &FrameData) -> Result<(), RenderError>;

    /// Hands back the pixels of the most recently drawn frame.
    ///
    /// **The agent's eyes** — ADR 0021 names capture as exactly that, and it is the one thing
    /// `render.describe` cannot do: `describe` reports what *should* be drawn, computed from the
    /// world, and nothing else checks what the GPU actually produced.
    ///
    /// # The default is an error, and that is the right answer for a backend that draws nothing
    ///
    /// A backend that cannot read its own output should say so rather than return a blank image that
    /// a caller would have to know not to trust. `NullBackend` draws nothing at all, so it refuses
    /// and names `render.describe` instead.
    ///
    /// **Both wgpu backends can answer**, which was not true before the render graph landed. Every
    /// camera now draws into an off-screen transient and a final pass copies it onward, so a
    /// *windowed* run has a readable image of the finished frame even though a window's own image
    /// can never be read back. The two differ by exactly that last copy: an offscreen backend reads
    /// the destination after it, a windowed one reads the transient before it.
    ///
    /// # Errors
    ///
    /// [`RenderError::CaptureUnsupported`] by default, naming the backend and what to use instead.
    fn capture(&mut self) -> Result<TextureData, RenderError> {
        Err(RenderError::CaptureUnsupported {
            backend: self.name(),
            reason:
                "this backend does not keep the pixels it drew. `render.describe` answers what \
                     should be on screen without a GPU; capture needs an offscreen wgpu backend"
                    .to_string(),
        })
    }
}

/// A backend that draws nothing and records what it was asked to draw.
///
/// Used for headless runs, CI, and tests. The recorded frame makes it possible to assert *what would
/// have been drawn* without a GPU — a cheap stand-in for the screenshot comparison the agent
/// interface layer will offer in M1, and enough to catch "nothing is on screen" bugs in a test.
#[derive(Debug, Clone)]
pub struct NullBackend {
    viewport: (u32, u32),
    frames_rendered: u64,
    last_frame: Option<FrameData>,
    /// What was uploaded, by id — the whole texture, so a headless test can assert on the pixels a
    /// GPU would have received. Ordered, so listing it is reproducible (invariant I3).
    textures: std::collections::BTreeMap<String, TextureData>,
    /// The labels of the passes the last frame's graph resolved to, in execution order.
    last_passes: Vec<String>,
    /// Geometry that reached the backend, by id — so a headless test can assert the *right* mesh
    /// arrived, which is a much sharper claim than "no error was returned".
    meshes: std::collections::BTreeMap<String, crate::MeshData>,
}

impl Default for NullBackend {
    fn default() -> Self {
        Self::new(1280, 720)
    }
}

impl NullBackend {
    /// Creates a null backend reporting a given viewport size.
    #[must_use]
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            viewport: (width, height),
            frames_rendered: 0,
            last_frame: None,
            textures: std::collections::BTreeMap::new(),
            last_passes: Vec::new(),
            meshes: std::collections::BTreeMap::new(),
        }
    }

    /// How many frames have been rendered.
    #[must_use]
    pub fn frames_rendered(&self) -> u64 {
        self.frames_rendered
    }

    /// The most recent frame, if anything has been rendered.
    #[must_use]
    pub fn last_frame(&self) -> Option<&FrameData> {
        self.last_frame.as_ref()
    }

    /// How many quads the most recent frame contained, across every view.
    #[must_use]
    pub fn last_quad_count(&self) -> usize {
        self.last_frame.as_ref().map_or(0, FrameData::quad_count)
    }

    /// The pixels uploaded under an id, if any.
    ///
    /// The reason texture work is testable with no GPU: a headless test can assert that the *right*
    /// image reached the backend, which is a much sharper claim than "no error was returned".
    #[must_use]
    pub fn texture(&self, id: &str) -> Option<&TextureData> {
        self.textures.get(id)
    }

    /// Every uploaded texture id, in order.
    pub fn texture_ids(&self) -> impl Iterator<Item = &str> {
        self.textures.keys().map(String::as_str)
    }

    /// The geometry uploaded under an id, if any.
    ///
    /// The mesh counterpart of [`NullBackend::texture`], and it exists for a sharper reason than
    /// symmetry: geometry is now *replaced* when a chunk is dug and *dropped* when one streams away,
    /// and both are silent failures. A stale mesh looks like solid rock over a tunnel; a mesh never
    /// dropped looks like nothing at all until video memory runs out. This is what lets a headless
    /// test see the difference.
    #[must_use]
    pub fn mesh(&self, id: &str) -> Option<&crate::MeshData> {
        self.meshes.get(id)
    }

    /// Every uploaded mesh id, in order.
    pub fn mesh_ids(&self) -> impl Iterator<Item = &str> {
        self.meshes.keys().map(String::as_str)
    }

    /// The passes the last frame resolved to, by label, in the order they would have run.
    ///
    /// # Why a backend that draws nothing still builds a plan
    ///
    /// The render graph (ADR 0034) is a plan, and building one is arithmetic rather than drawing —
    /// so a wrong plan is a bug catchable with no GPU, which is what invariant I7 asks of every
    /// subsystem. This is how a headless test sees the frame's structure: three cameras and a
    /// present pass, or a clear pass because nobody authored a camera at all.
    ///
    /// Labels only, deliberately. Reporting the plan is introspection, which this project wants
    /// everywhere; handing out the graph's *types* would make it an extension surface, which ADR
    /// 0034 decided against.
    pub fn last_passes(&self) -> &[String] {
        &self.last_passes
    }
}

impl RenderBackend for NullBackend {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn name(&self) -> &'static str {
        "null"
    }

    fn viewport(&self) -> (u32, u32) {
        self.viewport
    }

    fn resize(&mut self, width: u32, height: u32) {
        self.viewport = (width, height);
    }

    fn has_texture(&self, id: &str) -> bool {
        self.textures.contains_key(id)
    }

    fn upload_texture(&mut self, id: &str, texture: &TextureData) -> Result<(), RenderError> {
        self.textures.insert(id.to_string(), texture.clone());
        Ok(())
    }

    fn has_mesh(&self, id: &str) -> bool {
        self.meshes.contains_key(id)
    }

    fn upload_mesh(&mut self, id: &str, mesh: &crate::MeshData) -> Result<(), RenderError> {
        self.meshes.insert(id.to_string(), mesh.clone());
        Ok(())
    }

    /// Implemented for real rather than left as the trait's no-op, so that a headless test can
    /// assert geometry was actually released. A null backend that quietly kept everything would make
    /// a leak invisible to every test that does not need a GPU — which is most of them.
    fn remove_mesh(&mut self, id: &str) {
        self.meshes.remove(id);
    }

    fn render(&mut self, frame: &FrameData) -> Result<(), RenderError> {
        // The same graph the GPU backend builds, compiled and then thrown away. It draws nothing,
        // but a graph that does not hold together is a bug this catches on a machine with no GPU.
        let (width, height) = self.viewport;
        let graph = crate::graph::frame_graph(frame, width, height);
        let plan = graph.compile().map_err(|error| RenderError::GraphInvalid {
            reason: error.to_string(),
        })?;

        self.last_passes = plan
            .order()
            .iter()
            .map(|&index| graph.passes()[index].label.clone())
            .collect();
        self.frames_rendered += 1;
        self.last_frame = Some(frame.clone());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_frame() -> FrameData {
        FrameData {
            views: vec![View {
                camera: Camera::default(),
                environment: crate::Environment::default(),
                eye: [0.0, 0.0],
                eye_matrix: amadeo_transform::Mat4::IDENTITY,
                meshes: Vec::new(),
                lights: Vec::new(),
                quads: vec![QuadInstance {
                    center: [1.0, 2.0],
                    size: [1.0, 1.0],
                    rotation: 0.0,
                    color: [1.0, 0.0, 0.0, 1.0],
                }],
                batches: Vec::new(),
            }],
            ..FrameData::default()
        }
    }

    #[test]
    fn null_backend_records_what_it_was_asked_to_draw() {
        let mut backend = NullBackend::new(800, 600);
        assert_eq!(backend.name(), "null");
        assert_eq!(backend.viewport(), (800, 600));
        assert_eq!(backend.frames_rendered(), 0);
        assert_eq!(backend.last_frame(), None);

        backend.render(&sample_frame()).expect("null never fails");

        assert_eq!(backend.frames_rendered(), 1);
        assert_eq!(backend.last_quad_count(), 1);
        assert_eq!(
            backend
                .last_frame()
                .expect("rendered")
                .primary()
                .expect("one view")
                .quads[0]
                .center,
            [1.0, 2.0]
        );
    }

    #[test]
    fn a_headless_frame_still_resolves_a_pass_order() {
        // ADR 0034's graph is a plan rather than drawing, so it is checkable with no GPU — which is
        // the whole reason it does not live inside the wgpu backend.
        let mut backend = NullBackend::new(64, 64);
        backend.render(&sample_frame()).expect("null never fails");
        assert_eq!(backend.last_passes(), ["view 0", "post", "present"]);

        // And a world nobody gave a camera clears rather than freezing.
        backend
            .render(&FrameData::default())
            .expect("null never fails");
        assert_eq!(backend.last_passes(), ["clear", "post", "present"]);
    }

    #[test]
    fn null_backend_tracks_resizes() {
        let mut backend = NullBackend::default();
        assert_eq!(backend.viewport(), (1280, 720));
        backend.resize(1920, 1080);
        assert_eq!(backend.viewport(), (1920, 1080));
    }

    #[test]
    fn frame_defaults_are_visibly_not_black() {
        // So an empty-but-cleared frame is distinguishable from nothing having rendered at all.
        let frame = FrameData::default();
        assert!(frame.views.is_empty(), "a default frame has no camera");
        assert_eq!(frame.quad_count(), 0);
        assert!(frame.clear_color[0] > 0.0);
        assert_eq!(frame.clear_color[3], 1.0, "background must be opaque");
    }

    #[test]
    fn render_errors_are_actionable() {
        let error = RenderError::SurfaceUnavailable {
            reason: "outdated".to_string(),
        };
        let text = error.to_string();
        assert!(text.contains("outdated"), "{text}");
        // Says what to do, not just what happened.
        assert!(text.contains("skip the frame"), "{text}");
    }
}
