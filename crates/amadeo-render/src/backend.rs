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
    /// Quads to draw, already sorted by [`SortOrder`](crate::SortOrder).
    pub quads: Vec<QuadInstance>,
    /// Textured sprites, grouped into draw calls and ordered by [`SpriteBatch::order`].
    pub batches: Vec<SpriteBatch>,
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

    /// Draws one frame.
    fn render(&mut self, frame: &FrameData) -> Result<(), RenderError>;

    /// Hands back the pixels of the most recently drawn frame.
    ///
    /// **The agent's eyes** — ADR 0021 names capture as exactly that, and it is the one thing
    /// `render.describe` cannot do: `describe` reports what *should* be drawn, computed from the
    /// world, and nothing else checks what the GPU actually produced.
    ///
    /// # The default is an error, and that is the right answer for most backends
    ///
    /// A backend that cannot read its own output should say so rather than return a blank image that
    /// a caller would have to know not to trust. `NullBackend` draws nothing; a windowed wgpu backend
    /// draws into a swapchain image that is not created with `COPY_SRC`. Only
    /// [`WgpuBackend::offscreen`](crate::WgpuBackend::offscreen) can answer, and agent mode is
    /// headless anyway.
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

    fn render(&mut self, frame: &FrameData) -> Result<(), RenderError> {
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
                eye: [0.0, 0.0],
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
