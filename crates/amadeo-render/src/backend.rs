//! The rendering backend abstraction, and the null backend every build must have.

use crate::components::Camera2d;
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
}

/// One quad, flattened into what a GPU needs.
///
/// Produced by the collection pass from [`Transform2d`](amadeo_transform::Transform2d) and
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

/// Everything needed to draw one frame.
///
/// Built by reading the world, then handed to a backend. Nothing in here borrows the world, which is
/// what keeps rendering strictly read-only with respect to simulation (ADR 0005).
#[derive(Debug, Clone, PartialEq)]
pub struct FrameData {
    /// Background colour, linear RGBA.
    pub clear_color: [f32; 4],
    /// The camera to draw through.
    pub camera: Camera2d,
    /// Quads to draw, already sorted by layer.
    pub quads: Vec<QuadInstance>,
}

impl Default for FrameData {
    fn default() -> Self {
        Self {
            // A dark neutral that is clearly not black, so "nothing rendered" and "cleared but empty"
            // are distinguishable at a glance.
            clear_color: [0.06, 0.07, 0.09, 1.0],
            camera: Camera2d::default(),
            quads: Vec::new(),
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

    /// Draws one frame.
    fn render(&mut self, frame: &FrameData) -> Result<(), RenderError>;
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

    /// How many quads the most recent frame contained.
    #[must_use]
    pub fn last_quad_count(&self) -> usize {
        self.last_frame
            .as_ref()
            .map_or(0, |frame| frame.quads.len())
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
            quads: vec![QuadInstance {
                center: [1.0, 2.0],
                size: [1.0, 1.0],
                rotation: 0.0,
                color: [1.0, 0.0, 0.0, 1.0],
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
            backend.last_frame().expect("rendered").quads[0].center,
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
        assert!(frame.quads.is_empty());
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
