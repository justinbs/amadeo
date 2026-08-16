//! The wgpu backend. Only compiled with the `gpu` feature.
//!
//! Renders every quad in a frame with a single instanced draw call. There is no vertex or index
//! buffer — the vertex shader builds each quad's four corners from the vertex index — so the only
//! per-frame upload is the instance buffer.
//!
//! # Reading this if you are new to GPU code
//!
//! The shape is the same in every graphics API:
//!
//! 1. **Instance / adapter / device / queue** — pick a GPU and open a connection to it. Done once.
//! 2. **Surface** — the swapchain of images the window displays. Reconfigured on every resize.
//! 3. **Pipeline** — a compiled shader plus all the fixed-function state it runs with. Done once.
//! 4. **Buffers** — the data the shader reads. Uploaded per frame.
//! 5. **Encoder / render pass** — record commands, then submit them to the queue.
//!
//! Steps 1–3 happen in [`WgpuBackend::new`]; steps 4–5 happen every frame in
//! [`WgpuBackend::render`].
//!
//! # What decides the order of those passes
//!
//! Not this file. [`crate::graph`] builds a plan for the frame — which passes exist, what each one
//! reads and writes — and derives the order from the dependencies. This backend *executes* that
//! plan and does nothing else about ordering. The split is what makes a pass-ordering bug catchable
//! with no GPU, and ADR 0034 is why the plan's types stay inside this crate.

use crate::backend::{FrameData, RenderBackend, RenderError, View};
use crate::environment::{Environment, Tonemap};
use crate::graph::{self, DESTINATION, PassKind, Plan, RenderGraph, TargetFormat};
use amadeo_image::{PixelFormat, TextureData};
use std::any::Any;
use std::collections::BTreeMap;

/// One quad as the GPU sees it.
///
/// `repr(C)` because the field order has to match the vertex attribute layout below exactly, and
/// Rust makes no ordering guarantee otherwise. `Pod`/`Zeroable` let `bytemuck` reinterpret a slice
/// of these as raw bytes without `unsafe` at this call site.
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct GpuInstance {
    /// xy = world centre, zw = full world size.
    center_size: [f32; 4],
    /// x = rotation in radians; the rest is padding for 16-byte alignment.
    rotation: [f32; 4],
    /// Linear RGBA.
    color: [f32; 4],
}

/// One sprite as the GPU sees it.
///
/// Four `vec4`s, matching `sprite.wgsl`'s `InstanceInput` field for field. The layout is padded to
/// 16-byte boundaries because both WGSL and the vertex buffer layout require it — `axis_y` carries
/// two real floats and two of padding for exactly that reason.
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct GpuSprite {
    /// xy = world centre, zw = the full-extent x axis.
    center_axis_x: [f32; 4],
    /// xy = the full-extent y axis, zw = padding.
    axis_y: [f32; 4],
    /// Linear RGBA tint.
    color: [f32; 4],
    /// Texture sub-rectangle, `[x, y, width, height]` in 0..1.
    region: [f32; 4],
}

/// The camera as the GPU sees it.
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct GpuCamera {
    center: [f32; 2],
    half_extents: [f32; 2],
}

/// One mesh vertex as the GPU sees it. Matches `Vertex` and `mesh.wgsl`'s `VertexInput`.
///
/// Twelve floats, which is 48 bytes and a multiple of 16 — not required for a vertex buffer, whose
/// only rule is that the stride matches what the pipeline declares, but it keeps the layout obvious
/// and matches how every other GPU struct here is written.
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct GpuVertex {
    position: [f32; 3],
    normal: [f32; 3],
    uv: [f32; 2],
    /// xyz = the texture's u direction across the surface, w = ±1 handedness (ADR 0047).
    tangent: [f32; 4],
}

/// One drawn copy of a mesh: where it is and what it is made of.
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct GpuMeshInstance {
    /// The model matrix, column by column — a vertex attribute cannot be a matrix.
    model: [[f32; 4]; 4],
    base_colour: [f32; 4],
    /// rgb = emissive, a unused.
    emissive: [f32; 4],
    /// How this surface responds to light: `[metallic, roughness, normal_strength, unused]`.
    ///
    /// The scalar half of the material. A metallic-roughness *texture* multiplies into the first two
    /// per pixel, exactly as `base_colour_texture` multiplies into `base_colour` — which is what
    /// glTF specifies and what lets one material serve a whole surface with varying wear.
    surface: [f32; 4],
}

/// One point or spot light, as the mesh shader reads it — ADR 0057.
///
/// Packed into three `vec4`s because a WGSL uniform pads every member to sixteen bytes: a position,
/// a range, a direction and two cosines as separate members would occupy ninety-six bytes where this
/// occupies forty-eight, and would read no more clearly than the field names here.
///
/// **Must match `PunctualLight` in `view.wgsl` field for field.** That declaration is shared between
/// the mesh and sky shaders so the two cannot disagree with each other; this one is Rust, and only a
/// wrong picture says it disagrees with them.
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct GpuPunctualLight {
    /// xyz = world position, w = range.
    position_range: [f32; 4],
    /// xyz = the direction the beam travels, w = cosine of the outer half-angle.
    direction_outer: [f32; 4],
    /// rgb = colour with intensity folded in, a = cosine of the inner half-angle.
    colour_inner: [f32; 4],
    /// x = shadow layer, or -1 for a light that casts none. y = its bias. zw unused.
    shadow: [f32; 4],
}

/// What one shadow pass needs: the one matrix it draws with.
///
/// # Why the shadow pass stopped sharing `GpuMeshView`
///
/// It used to read the mesh view's `light_view_projection`, which was a single matrix, and sharing
/// meant a view's light matrix was written once rather than into two buffers that could disagree.
/// Cascades made that matrix an *array* (ADR 0055), and a shadow pass draws exactly one of them — so
/// sharing would need the pass to be told which, which is a uniform of its own by another name.
///
/// The "cannot disagree" property is kept a different way: both are filled in the same loop from the
/// same [`ShadowData`](crate::ShadowData), so there is still one source.
///
/// Indexed by `view * MAX_SHADOW_LAYERS + layer`, which is why it is a dynamic offset like the mesh
/// view rather than a uniform rewritten between passes.
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct GpuShadowView {
    /// World to this cascade's light clip space.
    view_projection: [[f32; 4]; 4],
}

/// What one 3D view needs that is the same for every instance in it.
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct GpuMeshView {
    view_projection: [[f32; 4]; 4],
    /// World to each cascade's light clip space, nearest first (ADR 0038, ADR 0055).
    ///
    /// Always [`CASCADE_COUNT`](crate::CASCADE_COUNT) of them whatever the mode: an unused slot holds
    /// the identity, which costs a matrix the shader never selects. `shadow_params.w` says how many
    /// are real.
    light_view_projection: [[[f32; 4]; 4]; crate::CASCADE_COUNT],
    light_direction: [f32; 4],
    light_colour: [f32; 4],
    /// x = one shadow-map texel in UV, y = 1 when a shadow map is bound and 0 when the placeholder
    /// is, z = how many cascades are real, w unused.
    ///
    /// `y` is what stops the placeholder being *sampled* as though it were real. It is a number
    /// rather than a separate pipeline because a branch that every pixel takes the same way is
    /// nearly free on a GPU, where a second pipeline is a real state change.
    ///
    /// **The bias left this field when cascades landed**, because there is one per cascade now — see
    /// `cascade_bias`.
    shadow_params: [f32; 4],
    /// Where each cascade stops covering, as a distance from the camera. Nearest first.
    ///
    /// What the fragment stage selects a cascade *by*. A `vec4` rather than four floats because WGSL
    /// pads a `f32` in a uniform to sixteen bytes anyway, so four of them cost exactly this.
    cascade_far: [f32; 4],
    /// Each cascade's depth bias, in its own clip space. Nearest first.
    ///
    /// **Per cascade rather than shared, and this is the one that bites.** A bias tuned for a near
    /// cascade spanning ten metres is a quarter of what a far one spanning seventy needs, and too
    /// little bias is shadow acne — a surface stippling itself dark. The conversion in `fit_cascade`
    /// produces this for free by dividing through each cascade's own depth range.
    cascade_bias: [f32; 4],
    /// The fog colour in linear light, xyz, with its density in w (ADR 0073).
    ///
    /// **Density in the alpha slot rather than in a field of its own**, because WGSL pads a lone
    /// `f32` in a uniform to sixteen bytes: a separate one would cost exactly as much as this whole
    /// vector and would leave the colour looking like it might be used without it.
    fog_colour: [f32; 4],
    /// x = how far from the eye fog starts, in world units. yzw unused.
    fog_params: [f32; 4],
    /// The camera's world position, xyz. w unused.
    ///
    /// New with PBR (ADR 0048) and not needed before it: diffuse lighting looks the same from every
    /// direction, so Lambert never had to know where the viewer was. A **specular** highlight is a
    /// reflection, and where a reflection lands depends entirely on where you are standing.
    eye: [f32; 4],
    /// Three vectors that turn a screen position into a world direction, for drawing the sky.
    ///
    /// `direction = sky_right * ndc.x + sky_up * ndc.y + sky_forward`, then normalised. The first two
    /// already carry the field of view and the aspect ratio, so the shader needs no trigonometry and
    /// no matrix.
    ///
    /// **A basis rather than an inverse view-projection matrix**, which is the usual way to do this.
    /// `Mat4` has `inverse_rigid` — an inverse for transforms that only rotate and translate — and a
    /// projection is neither, so a general inverse would have had to be written and tested for this
    /// one caller. Three vectors computed where the projection is already being built cost nothing
    /// and need no new maths.
    sky_right: [f32; 4],
    sky_up: [f32; 4],
    sky_forward: [f32; 4],
    /// x = how many of `punctual` are real, yzw unused.
    punctual_count: [f32; 4],
    /// Point and spot lights, nearest first (ADR 0057). Unused slots are zeroed and never read.
    punctual: [GpuPunctualLight; crate::MAX_PUNCTUAL_LIGHTS],
    /// World to each shadow-casting spot light's clip space (ADR 0058), indexed by that light's
    /// shadow layer minus the number of cascades.
    spot_shadow: [[[f32; 4]; 4]; crate::MAX_SHADOW_SPOTS],
}

/// One uploaded mesh's buffers.
///
/// Held by id in a `BTreeMap`, exactly as an uploaded texture is, and for the same reason: geometry
/// travels to the device once rather than in every frame.
#[derive(Debug)]
struct GpuMesh {
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    index_count: u32,
}

/// How many instances the buffer starts with. Grows as needed; never shrinks.
const INITIAL_INSTANCE_CAPACITY: usize = 256;

/// How many cameras the uniform buffer starts with room for. Grows as needed.
///
/// Four rather than one because the multi-camera cases are small and bounded — a world view, a HUD,
/// a minimap, an editor viewport — so this covers essentially every real frame without a reallocation.
const INITIAL_VIEW_CAPACITY: usize = 4;

/// The byte stride between two cameras in the uniform buffer.
///
/// A dynamic offset must be a multiple of the device's alignment, which is a hardware limit rather
/// than something to pick — commonly 256 bytes even though a [`GpuCamera`] is 16.
fn camera_stride(device: &wgpu::Device) -> u64 {
    let alignment = u64::from(device.limits().min_uniform_buffer_offset_alignment).max(1);
    let size = size_of::<GpuCamera>() as u64;
    size.div_ceil(alignment) * alignment
}

/// Where one view's data sits inside the frame-wide buffers.
///
/// Every view's quads and sprites are packed into two shared buffers, so drawing a view means
/// drawing its slice of each. Recorded while packing rather than recomputed while encoding: a
/// running offset maintained in two places is what drifts.
struct ViewDraws<'a> {
    /// This view's instances within the shared quad buffer.
    quads: std::ops::Range<u32>,
    /// One texture id and its instance range per batch, in draw order.
    draws: Vec<(&'a str, std::ops::Range<u32>)>,
}

/// The format the cameras draw into: **high dynamic range**, linear, sixteen-bit float.
///
/// A pixel can be brighter than white here, which is what gives tonemapping something to compress
/// and bloom something to isolate. `rgba16float` is renderable and blendable in core WebGPU, so this
/// costs no device feature and rules out no adapter.
const SCENE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;

/// The format the finished, displayable picture sits in before it reaches the destination.
///
/// Deliberately **not** the destination's own format, and it matters more than it sounds: a window
/// surface is commonly BGRA, so an output image that inherited it would come back from `capture`
/// with red and blue swapped on the windowed path and not the offscreen one. Fixing it here means
/// the present pass is the single place the destination's format is met, and the hardware does that
/// conversion while writing.
const OUTPUT_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;

/// Turns the graph's format into wgpu's.
///
/// A `match` rather than a constant, so that adding a variant to [`TargetFormat`] fails to compile
/// here instead of silently allocating the wrong thing.
fn wgpu_format(format: TargetFormat) -> wgpu::TextureFormat {
    match format {
        TargetFormat::Srgb8 => OUTPUT_FORMAT,
        TargetFormat::Hdr16 => SCENE_FORMAT,
        // The same wgpu format as the scene depth buffer. What differs is the usage it is created
        // with and the layout it is bound through -- see `TargetFormat::ShadowMap32`.
        TargetFormat::ShadowMap32 { .. } | TargetFormat::Depth32Ms => DEPTH_FORMAT,
    }
}

/// How many samples an anti-aliased target takes per pixel — ADR 0051.
///
/// **Four**, which is the near-universal choice: two is visibly not enough on a near-vertical edge,
/// and eight costs twice the memory for a difference most people cannot see. Every adapter supports
/// four for the formats used here.
///
/// This is what anti-aliases *geometry edges* — the only kind low-poly has (ADR 0050). It leaves
/// texture detail and flat colour untouched, which is precisely why it was chosen over a
/// post-process filter like FXAA: that one would smear exactly the crisp facets the look depends on.
const SAMPLE_COUNT: u32 = 4;

/// The multisample state every pipeline that draws into the **scene** must declare.
///
/// wgpu requires a pipeline's sample count to match the attachment it draws into, and fails at
/// pipeline creation rather than at draw when it does not — which is the good direction for this to
/// fail in. Four pipelines target the scene: quads, sprites, meshes and the sky.
///
/// The other three deliberately keep the single-sampled default. **Post** and **present** are
/// full-screen passes over already-resolved images; a **shadow** map is sampled directly by the mesh
/// shader, and a multisampled one could not be.
const SCENE_MULTISAMPLE: wgpu::MultisampleState = wgpu::MultisampleState {
    count: SAMPLE_COUNT,
    mask: !0,
    // Off: it is for anti-aliasing *cutouts* in alpha-tested foliage, and nothing here alpha-tests.
    alpha_to_coverage_enabled: false,
};

/// The depth buffer's format.
///
/// `Depth32Float` rather than a packed depth-stencil format: nothing here uses a stencil buffer, and
/// asking for one would cost memory on every 3D frame to carry eight bits nothing reads.
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

/// The post-process settings as the GPU sees them.
///
/// Packed into `vec4`s rather than named scalars because a WGSL uniform pads every member to 16
/// bytes — four separate `f32`s would occupy 64 bytes and read no more clearly. Matches `Post` in
/// `post.wgsl` field for field.
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct GpuPost {
    /// x = exposure, y = tonemap operator, z = vignette intensity, w = vignette radius.
    controls: [f32; 4],
    /// x = contrast, y = saturation, zw unused.
    grade: [f32; 4],
    /// rgb = tint, a unused.
    tint: [f32; 4],
    /// x = bloom threshold, y = bloom intensity, zw unused.
    ///
    /// Read by `bloom.wgsl`'s bright pass and by `post.wgsl`'s composite. Both shaders declare this
    /// struct through the shared `post_uniform.wgsl`, so there is one WGSL statement of the layout —
    /// **this Rust one is the copy that cannot be shared**, and adding a field here means adding it
    /// there, in the same position.
    bloom: [f32; 4],
}

impl GpuPost {
    /// Flattens an [`Environment`] into what the shader reads.
    ///
    /// The tonemap becomes a number, and **the numbering is a contract with `post.wgsl`** — see
    /// `tonemap_operator`.
    fn from_environment(look: &Environment) -> Self {
        Self {
            controls: [
                look.exposure,
                tonemap_operator(look.tonemap),
                look.vignette.intensity,
                look.vignette.radius,
            ],
            grade: [look.grade.contrast, look.grade.saturation, 0.0, 0.0],
            tint: [
                look.grade.tint[0],
                look.grade.tint[1],
                look.grade.tint[2],
                0.0,
            ],
            bloom: [look.bloom.threshold, look.bloom.intensity, 0.0, 0.0],
        }
    }
}

/// Which curve `post.wgsl` should apply.
///
/// An exhaustive `match` rather than a cast, so adding a [`Tonemap`] variant fails to compile here
/// rather than silently selecting the wrong curve — a cast from an enum's discriminant would happily
/// send an unknown number to a shader that would then fall through to "no tonemap".
fn tonemap_operator(tonemap: Tonemap) -> f32 {
    match tonemap {
        Tonemap::None => 0.0,
        Tonemap::Reinhard => 1.0,
        Tonemap::AcesFilmic => 2.0,
    }
}

/// One physical texture backing a graph transient.
///
/// Kept across frames rather than allocated per frame: a full-screen image is several megabytes, and
/// creating one sixty times a second would be the most expensive thing the renderer did.
#[derive(Debug)]
struct PooledTexture {
    width: u32,
    height: u32,
    format: TargetFormat,
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    /// One view per array layer, for **attaching** rather than sampling. Empty unless this is a
    /// shadow map.
    ///
    /// A render pass attaches exactly one texture view, and a layer of an array is its own view — so
    /// four cascades means four of these and four passes (ADR 0055). Built here rather than per frame
    /// for the same reason the bind group is: creating a view is a driver-side allocation.
    layer_views: Vec<wgpu::TextureView>,
    /// Built once at creation so a later pass can sample it, for the same reason an uploaded
    /// texture's bind group is: creating one is a driver-side allocation.
    ///
    /// **`None` for the scene depth buffer, which nothing samples.**
    ///
    /// This was written expecting shadow maps to be what finally read a depth texture, and that is
    /// what happened (ADR 0038) — but the `Option` survives rather than going away, because it turned
    /// out there are *two* kinds of depth texture and only one of them is sampled. A shadow map gets
    /// a bind group built against `shadow_layout`, whose sample type is `Depth` and whose sampler
    /// compares rather than filters; the scene depth buffer still gets none.
    ///
    /// Building a colour bind group against either would fail at bind-group *creation* rather than
    /// at draw, so it reads as an allocation bug rather than a layout one.
    bind_group: Option<wgpu::BindGroup>,
}

/// Creates the colour target an offscreen backend draws into.
///
/// `COPY_SRC` is the whole point: it is what allows the finished frame to be copied into a buffer
/// and read back, and it is exactly what a swapchain image does not have.
fn offscreen_texture(
    device: &wgpu::Device,
    width: u32,
    height: u32,
    format: wgpu::TextureFormat,
) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("amadeo offscreen target"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    })
}

/// One uploaded texture and the bind group that binds it.
///
/// The bind group is built once at upload and kept, rather than rebuilt per frame: creating one is a
/// driver-side allocation, and doing it per batch per frame would reintroduce exactly the per-draw
/// cost ADR 0023's batching exists to remove.
///
/// The `wgpu::Texture` is held alongside the view purely to keep it alive — dropping it would
/// invalidate the view and the bind group that reference it.
/// How many passes a frame can time. Beyond this, later passes are simply not measured.
///
/// A frame is one pass per camera plus post and present, so a scene would need a dozen cameras to
/// reach this. Fixed rather than grown because a query set cannot be resized and reallocating one
/// per frame would cost more than the thing being measured.
const MAX_TIMED_PASSES: usize = 16;

/// The GPU-side machinery for timing passes — **M2.5's exit gate 4**.
///
/// # Why this is optional at every level
///
/// `TIMESTAMP_QUERY` is not a feature every adapter advertises, and ADR 0002's whole argument for
/// wgpu is that the engine runs broadly. So it is requested **only if the adapter offers it**, the
/// whole struct is an `Option`, and a machine without it renders exactly as before and reports no
/// timings rather than refusing to start.
#[derive(Debug)]
struct GpuTiming {
    /// Two timestamps per pass: one at the start, one at the end.
    queries: wgpu::QuerySet,
    /// Where `resolve_query_set` writes the raw tick counts.
    resolved: wgpu::Buffer,
    /// A mappable copy of the above, because a `QUERY_RESOLVE` buffer cannot also be `MAP_READ`.
    readback: wgpu::Buffer,
    /// Nanoseconds per tick, which is a property of the queue rather than of the query.
    period: f32,
    /// Whether to actually time the next frame.
    ///
    /// **Off by default, and that is not caution.** Reading the results back means waiting for the
    /// GPU to finish, which is a full pipeline stall — the exact thing a real frame loop exists to
    /// avoid. A profiler may stall; a game may not. So a measurement harness turns this on and
    /// nothing else does.
    enabled: bool,
}

impl GpuTiming {
    /// Builds the query machinery, or `None` if this device cannot time anything.
    fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Option<GpuTiming> {
        if !device.features().contains(wgpu::Features::TIMESTAMP_QUERY) {
            return None;
        }

        let count = (MAX_TIMED_PASSES * 2) as u32;
        // Eight bytes per timestamp, which is what `resolve_query_set` writes.
        let size = u64::from(count) * 8;

        Some(GpuTiming {
            queries: device.create_query_set(&wgpu::QuerySetDescriptor {
                label: Some("amadeo pass timings"),
                ty: wgpu::QueryType::Timestamp,
                count,
            }),
            resolved: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("amadeo timing resolve"),
                size,
                usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            }),
            readback: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("amadeo timing readback"),
                size,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            }),
            period: queue.get_timestamp_period(),
            enabled: false,
        })
    }

    /// Where in the query set a given pass writes its two timestamps.
    fn writes(&self, pass: usize) -> Option<wgpu::RenderPassTimestampWrites<'_>> {
        if !self.enabled || pass >= MAX_TIMED_PASSES {
            return None;
        }
        Some(wgpu::RenderPassTimestampWrites {
            query_set: &self.queries,
            beginning_of_pass_write_index: Some((pass * 2) as u32),
            end_of_pass_write_index: Some((pass * 2 + 1) as u32),
        })
    }
}

/// What one frame cost on the GPU.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GpuFrameTiming {
    /// Wall time on the GPU from the first pass beginning to the last one ending.
    ///
    /// Larger than the sum of the passes below when the GPU idled between them, which is itself
    /// worth seeing: it means the bottleneck is elsewhere.
    pub total: std::time::Duration,
    /// Each timed pass, by the label the render graph gave it, in the order they ran.
    pub passes: Vec<(String, std::time::Duration)>,
}

#[derive(Debug)]
struct GpuTexture {
    #[allow(
        dead_code,
        reason = "held to keep the texture alive; the view and bind groups borrow from it"
    )]
    texture: wgpu::Texture,
    /// For sprites: clamped and unfiltered, because a `region` is a rectangle inside a shared sheet.
    ///
    /// A sprite draws one texture and can therefore have its bind group made once, here. A *surface*
    /// draws several at once — base colour and normal today, more with PBR — so its bind group
    /// belongs to the combination rather than to any one texture, and lives in
    /// [`WgpuBackend::material_bind_groups`].
    bind_group: wgpu::BindGroup,
    /// The view the material bind groups above are built from.
    ///
    /// Kept because a bind group is assembled from views, and a pair's group cannot be made until
    /// both textures exist — which may be different frames.
    view: wgpu::TextureView,
}

/// A prefiltered environment as the GPU holds it: two cube textures and the bind group naming them.
#[derive(Debug)]
struct GpuEnvironment {
    #[allow(
        dead_code,
        reason = "held to keep the cube textures alive; the bind group's views borrow from them"
    )]
    irradiance: wgpu::Texture,
    #[allow(
        dead_code,
        reason = "held to keep the cube textures alive; the bind group's views borrow from them"
    )]
    specular: wgpu::Texture,
    bind_group: wgpu::BindGroup,
}

/// One axis of a transform's basis, scaled — column `axis` of the matrix, times `by`.
///
/// A world transform's first three columns are the directions its local x, y and z point in the
/// world, which is exactly what turning a screen position into a view ray needs.
fn scaled_axis(matrix: &amadeo_transform::Mat4, axis: usize, by: f32) -> [f32; 4] {
    [
        matrix.columns[axis][0] * by,
        matrix.columns[axis][1] * by,
        matrix.columns[axis][2] * by,
        0.0,
    ]
}

/// Uploads a cube map, one mip level per entry, as a single cube texture.
///
/// The levels are handed over sharpest first and each is half the previous, which is exactly a mip
/// chain — so the specular chain and a mip chain are the same object, and the shader picks a
/// roughness by asking for a mip level.
fn upload_cubemap(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    label: &str,
    levels: &[crate::Cubemap],
) -> (wgpu::Texture, wgpu::TextureView) {
    let base = levels[0].size;
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width: base,
            height: base,
            // **Six layers, not six textures.** A cube map is an array of six faces that the
            // sampler addresses with a direction rather than a coordinate; the view below is what
            // tells wgpu to treat the layers that way.
            depth_or_array_layers: 6,
        },
        mip_level_count: levels.len() as u32,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba16Float,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });

    for (level, cube) in levels.iter().enumerate() {
        for (face, pixels) in cube.faces.iter().enumerate() {
            // Converted a level at a time rather than all at once, so the scratch buffer stays the
            // size of one face instead of the whole chain.
            let mut halves: Vec<u16> = Vec::with_capacity(pixels.len() * 4);
            for pixel in pixels {
                for channel in pixel {
                    halves.push(to_half(*channel));
                }
            }

            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: level as u32,
                    origin: wgpu::Origin3d {
                        x: 0,
                        y: 0,
                        // The array layer, which for a cube texture is the face.
                        z: face as u32,
                    },
                    aspect: wgpu::TextureAspect::All,
                },
                bytemuck::cast_slice(&halves),
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    // Four channels of two bytes each.
                    bytes_per_row: Some(cube.size * 8),
                    rows_per_image: Some(cube.size),
                },
                wgpu::Extent3d {
                    width: cube.size,
                    height: cube.size,
                    depth_or_array_layers: 1,
                },
            );
        }
    }

    let view = texture.create_view(&wgpu::TextureViewDescriptor {
        label: Some(label),
        dimension: Some(wgpu::TextureViewDimension::Cube),
        ..Default::default()
    });
    (texture, view)
}

/// Converts a 32-bit float to a 16-bit one, as the GPU stores an environment map.
///
/// # Why halve them at all
///
/// `Rgba32Float` is **not filterable** in wgpu's base feature set — sampling one with anything but
/// nearest-neighbour needs the `FLOAT32_FILTERABLE` feature, which plenty of adapters do not
/// advertise. `Rgba16Float` is filterable everywhere. An environment map is sampled with smooth
/// interpolation across faces and between roughness levels, so it must be the one that filters, and
/// half the memory is a bonus rather than the reason.
///
/// Half floats carry about three decimal digits and reach ~65504. Both are ample: this is lighting
/// information about to be blurred and multiplied by a surface colour, not a coordinate.
///
/// Written out rather than taking the `half` crate, because it is twenty lines of bit arithmetic and
/// this engine holds two non-`thiserror` dependencies in total.
fn to_half(value: f32) -> u16 {
    let bits = value.to_bits();
    let sign = ((bits >> 16) & 0x8000) as u16;
    // Exponent, rebiased from the 32-bit bias of 127 to the 16-bit bias of 15.
    let mut exponent = ((bits >> 23) & 0xff) as i32 - 127 + 15;
    let mantissa = bits & 0x007f_ffff;

    if exponent >= 0x1f {
        // Too big for a half, or already an infinity or a NaN. Saturating to infinity rather than
        // wrapping to zero, which is what a naive truncation would do and would turn the brightest
        // part of a sky into a black hole.
        let is_nan = ((bits >> 23) & 0xff) == 0xff && mantissa != 0;
        return sign | 0x7c00 | u16::from(is_nan) << 9;
    }
    if exponent <= 0 {
        // Below the smallest normal half. Subnormals are not worth the arithmetic here — every value
        // this small is thousands of times dimmer than anything visible.
        return sign;
    }
    // Truncating the mantissa rather than rounding. One bit of a half float is far below what any
    // reflection could show, and rounding would cost a branch per channel per texel.
    exponent &= 0x1f;
    sign | ((exponent as u16) << 10) | ((mantissa >> 13) as u16)
}

/// Builds a one-pixel texture holding a single colour, and a view onto it.
///
/// For the stand-ins a material binds when it names no texture of its own. Both are returned because
/// a bind group is assembled from the *view*, while the texture itself has to be kept alive or the
/// view dangles.
fn single_pixel_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    label: &str,
    format: wgpu::TextureFormat,
    pixel: [u8; 4],
) -> (wgpu::Texture, wgpu::TextureView) {
    let size = wgpu::Extent3d {
        width: 1,
        height: 1,
        depth_or_array_layers: 1,
    };
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &pixel,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(4),
            rows_per_image: Some(1),
        },
        size,
    );
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    (texture, view)
}

/// Which textures a surface draws with: the key a material bind group is cached under.
///
/// Ids rather than the textures themselves, because this is built from a `Material` before anything
/// has looked up whether either has been uploaded — an absent one falls back to a placeholder and
/// still draws, per ADR 0021.
///
/// **This grows as the material model does.** Metallic-roughness and occlusion maps join it, which
/// is the reason group 2 is keyed on a combination at all rather than a fourth and fifth bind group:
/// wgpu guarantees only four groups, and 0, 1 and 2 are already the view, the shadow map and this.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct MaterialTextures {
    base_colour: String,
    normal: String,
    metallic_roughness: String,
}

impl MaterialTextures {
    /// The texture ids a material names. Empty strings stay empty and mean "bind the placeholder".
    fn of(material: &crate::Material) -> MaterialTextures {
        MaterialTextures {
            base_colour: material.base_colour_texture.clone(),
            normal: material.normal_texture.clone(),
            metallic_roughness: material.metallic_roughness_texture.clone(),
        }
    }

    /// Every id this names, in binding order, including the empty ones.
    ///
    /// One place that knows the slot list, so adding a fourth texture does not mean remembering to
    /// update the decode walk, the upload walk and the invalidation check separately — which is
    /// exactly how `base_colour_texture` went a milestone without reaching a pixel.
    fn ids(&self) -> [&str; 3] {
        [
            self.base_colour.as_str(),
            self.normal.as_str(),
            self.metallic_roughness.as_str(),
        ]
    }
}

/// Where a finished frame goes.
///
/// The windowed and offscreen backends differ in **this and nothing else** — same pipelines, same
/// buffers, same passes — which is what makes a captured image evidence about the real renderer
/// rather than about a second one written to be testable.
#[derive(Debug)]
enum Target {
    /// A window's swapchain. Frames are presented and cannot be read back.
    Window {
        surface: wgpu::Surface<'static>,
        config: wgpu::SurfaceConfiguration,
    },
    /// A texture this backend owns. Frames stay in memory and **can** be read back.
    ///
    /// What agent mode uses, since ADR 0016 launches a game with no window. Reading a *surface*
    /// texture back is not possible — a swapchain image is not created with `COPY_SRC` and wgpu does
    /// not let you ask for one — so this is the only target a capture can read *after* the present
    /// pass, which is why it is the path whose tests cover the whole pipeline.
    Offscreen { texture: wgpu::Texture },
}

/// Turns wgpu's adapter info into the engine's own description.
///
/// `DeviceType::Cpu` is wgpu's tag for a software rasteriser — lavapipe on Linux, WARP on Windows.
/// Both report themselves honestly, so this needs no name matching.
fn describe_adapter(adapter: &wgpu::Adapter) -> AdapterDescription {
    let info = adapter.get_info();
    AdapterDescription {
        software: info.device_type == wgpu::DeviceType::Cpu,
        name: info.name,
    }
}

/// Which GPU actually answered, and whether it is one.
///
/// # Why the engine reports this rather than assuming
///
/// `WgpuBackend::offscreen` deliberately asks for an adapter with no compatible surface, which is
/// what lets a **software** adapter answer on a machine with no GPU — that is how CI captures images
/// at all. But a software adapter is not slightly slower than hardware, it is *dozens of times*
/// slower, and the difference is invisible from inside the engine.
///
/// That invisibility cost a red CI once: `games/atrium`'s frame-budget test measured 130 µs on a
/// developer machine and **8764 µs** on a runner, against a tripwire of 8333 µs. Nothing was wrong
/// with the engine; the number was measured on a CPU pretending to be a GPU. A timing claim that
/// cannot tell those two apart is not a measurement.
///
/// An engine-owned type rather than `wgpu::AdapterInfo`, so no wgpu type crosses the boundary —
/// the same rule ADR 0036 §4 states for rapier, applied here because the reason is identical.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdapterDescription {
    /// The driver's name for the adapter, for diagnostics and error messages.
    pub name: String,
    /// Whether this is a CPU implementation rather than real hardware.
    ///
    /// **A performance number measured on one of these means nothing** about how the game runs. Any
    /// test asserting a time bound must check this first and report rather than fail.
    pub software: bool,
}

/// A wgpu-backed renderer drawing into a window surface or an offscreen texture.
#[derive(Debug)]
pub struct WgpuBackend {
    /// Where finished frames go, and the only thing that differs between the two constructors.
    target: Target,
    /// Which adapter answered, and whether it is real hardware.
    adapter: AdapterDescription,
    device: wgpu::Device,
    queue: wgpu::Queue,
    /// GPU pass timing, when the adapter supports it (exit gate 4). `None` is a machine that cannot
    /// measure, which renders identically and reports nothing.
    timing: Option<GpuTiming>,
    /// What the last timed frame cost, or `None` if timing is off or nothing has been drawn yet.
    last_timing: Option<GpuFrameTiming>,
    /// Drawable size in physical pixels.
    width: u32,
    /// Drawable size in physical pixels.
    height: u32,
    /// The colour format the **destination** uses — a window's surface format, or the offscreen
    /// texture's. Everything is *drawn* in [`SCENE_FORMAT`]; this is only what the present pass
    /// writes into.
    format: wgpu::TextureFormat,
    pipeline: wgpu::RenderPipeline,
    sprite_pipeline: wgpu::RenderPipeline,
    /// Copies a finished transient onto the destination. See `present.wgsl`.
    present_pipeline: wgpu::RenderPipeline,
    /// Draws 3D geometry with one directional light. See `mesh.wgsl`.
    mesh_pipeline: wgpu::RenderPipeline,
    /// Draws the scene from a light's point of view, depth only (ADR 0038).
    shadow_pipeline: wgpu::RenderPipeline,
    /// Paints the environment behind everything else (ADR 0049). Shares `mesh_pipeline`'s layout.
    sky_pipeline: wgpu::RenderPipeline,
    /// The layout a shadow map is bound through: `Depth` sample type, comparison sampler.
    shadow_layout: wgpu::BindGroupLayout,
    shadow_sampler: wgpu::Sampler,
    /// Bound when nothing casts a shadow, so there is one mesh pipeline rather than two.
    shadow_placeholder_bind_group: wgpu::BindGroup,
    /// A 1×1 opaque white view, bound for a material that names no base colour texture.
    ///
    /// White is the identity of the multiply the shader does, so binding it is arithmetically the
    /// same as not sampling — which is what keeps this to one mesh pipeline rather than a textured
    /// one and an untextured one. Deliberately **not** the magenta `TextureCache` placeholder:
    /// magenta means "an asset is missing and you should notice", and an untextured material is not
    /// missing anything.
    white_placeholder_view: wgpu::TextureView,
    /// A 1×1 flat normal view, bound for a material that names no normal map. See its construction
    /// for why the pixel is `(128, 128, 255)` and why it is linear rather than sRGB.
    flat_normal_placeholder_view: wgpu::TextureView,
    /// One bind group per *combination* of material textures, built on first use.
    ///
    /// Keyed on the combination rather than on a single texture because group 2 now carries several
    /// images at once. Two materials naming the same pair share an entry, which is the common case:
    /// forty-four walls wearing one stone texture are one bind group.
    ///
    /// Cleared whenever a texture is uploaded — see [`WgpuBackend::upload_texture`] — because an
    /// entry built while an id was still a placeholder would otherwise keep showing the placeholder
    /// forever.
    material_bind_groups: BTreeMap<MaterialTextures, wgpu::BindGroup>,
    /// The multisampled colour buffer every view pass draws into, and its view (ADR 0051).
    ///
    /// Kept here rather than in the transient pool because it is not a transient: the pool hands out
    /// textures the *graph* declared, and the graph deliberately has no vocabulary for resolving —
    /// see `TargetFormat::Depth32Ms`. Re-created only when the frame size changes.
    multisampled_target: Option<(wgpu::Texture, wgpu::TextureView)>,
    /// What [`WgpuBackend::multisampled_target`] was last sized for, so a resize re-creates it and a
    /// steady frame does not.
    multisampled_size: (u32, u32),
    /// Prefiltered environments, by asset id (ADR 0049).
    environments: BTreeMap<String, GpuEnvironment>,
    /// What a view binds when it names no sky, or names one not yet prefiltered.
    neutral_environment_bind_group: wgpu::BindGroup,
    /// The layout every environment bind group is built to.
    environment_layout: wgpu::BindGroupLayout,
    /// Clamped, linear, and linear *between mip levels* — see its construction for why that last
    /// part is what makes roughness continuous rather than stepped.
    environment_sampler: wgpu::Sampler,
    #[allow(
        dead_code,
        reason = "held to keep the neutral cube textures alive; their views borrow from them"
    )]
    neutral_irradiance: wgpu::Texture,
    #[allow(
        dead_code,
        reason = "held to keep the neutral cube textures alive; their views borrow from them"
    )]
    neutral_specular: wgpu::Texture,
    #[allow(
        dead_code,
        reason = "held to keep the placeholder texture alive; its view and bind group borrow from it"
    )]
    shadow_placeholder: wgpu::Texture,
    #[allow(
        dead_code,
        reason = "held to keep the placeholder texture alive; its view borrows from it"
    )]
    white_placeholder: wgpu::Texture,
    #[allow(
        dead_code,
        reason = "held to keep the placeholder texture alive; its view borrows from it"
    )]
    flat_normal_placeholder: wgpu::Texture,
    /// Uploaded geometry, by asset id.
    meshes: BTreeMap<String, GpuMesh>,
    /// One aligned slot per 3D view, addressed by dynamic offset — the same arrangement the 2D
    /// camera uniform uses, and for the same reason (a queue write lands before the single submit,
    /// so writing per view would overwrite rather than follow).
    mesh_view_buffer: wgpu::Buffer,
    mesh_view_bind_group: wgpu::BindGroup,
    mesh_view_stride: u64,
    mesh_view_capacity: usize,
    mesh_view_layout: wgpu::BindGroupLayout,
    /// One aligned slot per (view, cascade), addressed by dynamic offset — ADR 0055.
    shadow_view_buffer: wgpu::Buffer,
    shadow_view_bind_group: wgpu::BindGroup,
    shadow_view_stride: u64,
    shadow_view_capacity: usize,
    shadow_view_layout: wgpu::BindGroupLayout,
    /// Per-instance model matrices and material colours.
    mesh_instance_buffer: wgpu::Buffer,
    mesh_instance_capacity: usize,
    /// Applies the camera's `Environment` and brings HDR down to displayable. See `post.wgsl`.
    post_pipeline: wgpu::RenderPipeline,
    /// One frame's post-process settings, rewritten each frame.
    post_buffer: wgpu::Buffer,
    post_bind_group: wgpu::BindGroup,
    /// Bloom's three passes: isolate the bright parts, blur along x, blur along y. See `bloom.wgsl`.
    bloom_bright_pipeline: wgpu::RenderPipeline,
    bloom_blur_x_pipeline: wgpu::RenderPipeline,
    bloom_blur_y_pipeline: wgpu::RenderPipeline,
    /// Bound as the glow when bloom is off — a 1×1 black texture, which adds nothing.
    bloom_placeholder_bind_group: wgpu::BindGroup,
    #[allow(
        dead_code,
        reason = "held to keep the placeholder texture alive; its view borrows from it"
    )]
    black_placeholder: wgpu::Texture,
    #[allow(
        dead_code,
        reason = "held to keep the placeholder texture alive; its bind group borrows from it"
    )]
    black_placeholder_view: wgpu::TextureView,
    /// Physical textures backing the graph's transients, reused within a frame and across frames.
    transients: Vec<PooledTexture>,
    /// Which pooled texture holds the last frame's finished picture.
    ///
    /// Only the **windowed** backend reads it, since an offscreen one can read its destination
    /// directly and gets the present pass's output that way. An index rather than the texture,
    /// because that keeps the pool the single owner — a second handle would have to be kept valid
    /// across a resize that throws the pool away.
    capture_source: Option<usize>,
    /// One aligned slot per camera, addressed by dynamic offset. ADR 0031.
    camera_buffer: wgpu::Buffer,
    camera_bind_group: wgpu::BindGroup,
    /// Bytes between two cameras in that buffer. A device limit, not a choice.
    camera_stride: u64,
    /// How many cameras the current buffer can hold.
    camera_capacity: usize,
    /// Kept so the buffer can be rebuilt when a frame has more cameras than it has room for.
    camera_layout: wgpu::BindGroupLayout,
    instance_buffer: wgpu::Buffer,
    /// How many instances the current buffer can hold.
    instance_capacity: usize,
    /// Sprite instances, in a buffer of their own so quads and sprites do not fight over one.
    sprite_buffer: wgpu::Buffer,
    /// How many sprite instances the current buffer can hold.
    sprite_capacity: usize,
    /// Uploaded textures, by asset id. Ordered, like every other registry in this engine.
    textures: BTreeMap<String, GpuTexture>,
    /// The layout every texture bind group is built against. Kept so an upload does not have to
    /// rebuild it.
    texture_layout: wgpu::BindGroupLayout,
    /// The filterable counterpart of [`WgpuBackend::texture_layout`], for textures worn by 3D
    /// surfaces rather than by sprites.
    surface_texture_layout: wgpu::BindGroupLayout,
    /// One sampler shared by every texture.
    ///
    /// **Nearest-neighbour, deliberately.** Three of the eight target games are pixel art, where
    /// linear filtering turns crisp art into mush, and the `.ama-meta` sidecar already carries a
    /// `filter` setting for the day this becomes per-asset. One sampler is also one fewer thing to
    /// switch between batches.
    sampler: wgpu::Sampler,
    /// Repeating and filtered, for textures worn by 3D surfaces. See [`GpuTexture`].
    surface_sampler: wgpu::Sampler,
}

impl WgpuBackend {
    /// Creates a backend drawing into `target`.
    ///
    /// `target` is anything wgpu can make a surface from — in practice an `Arc<winit::Window>`,
    /// which is what makes the returned surface `'static` and therefore storable in a service.
    ///
    /// Blocks on GPU initialisation. That happens once at startup, outside the simulation loop, so
    /// blocking is fine and is far simpler than threading async through the app's construction.
    pub fn new(
        target: impl Into<wgpu::SurfaceTarget<'static>>,
        width: u32,
        height: u32,
    ) -> Result<Self, RenderError> {
        // The `_from_env` form honours WGPU_BACKEND and WGPU_POWER_PREF, which is genuinely useful
        // for diagnosing a driver problem without recompiling. No display handle is needed here
        // because the surface target carries the window handle itself.
        let instance =
            wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());

        let surface = instance
            .create_surface(target)
            .map_err(|error| RenderError::InitFailed {
                backend: "wgpu",
                reason: format!("could not create a surface for the window: {error}"),
            })?;

        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            compatible_surface: Some(&surface),
            // Everything else stays at wgpu's defaults. Spelling out only what we care about means
            // a new field in a future wgpu release does not break this call.
            ..Default::default()
        }))
        .map_err(|error| RenderError::InitFailed {
            backend: "wgpu",
            reason: format!(
                "no compatible GPU adapter found: {error}. On Windows this usually means the \
                 graphics driver is out of date, or the app is running without a GPU available."
            ),
        })?;

        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("amadeo device"),
            // Defaults keep compatibility broad. Raise these only when a feature actually needs it,
            // since every requirement added here is a machine the engine stops running on.
            //
            // `TIMESTAMP_QUERY` is asked for **only if this adapter already has it** (exit gate 4).
            // Intersecting with `adapter.features()` is what keeps that true: requiring it outright
            // would turn "cannot measure GPU time here" into "cannot run here".
            required_features: adapter.features() & wgpu::Features::TIMESTAMP_QUERY,
            required_limits: wgpu::Limits::default(),
            ..Default::default()
        }))
        .map_err(|error| RenderError::InitFailed {
            backend: "wgpu",
            reason: format!("the GPU adapter refused a device: {error}"),
        })?;

        // A zero-sized surface is invalid, and a minimised window reports one, so clamp to 1.
        // Starting from wgpu's own default config rather than filling every field by hand keeps this
        // working across releases that add configuration options.
        let mut config = surface
            .get_default_config(&adapter, width.max(1), height.max(1))
            .ok_or_else(|| RenderError::InitFailed {
                backend: "wgpu",
                reason: "the adapter does not support this surface".to_string(),
            })?;
        config.present_mode = wgpu::PresentMode::AutoVsync;

        // Prefer an sRGB format so colours written by the shader are displayed correctly. Adapters
        // that offer none keep whatever the default config chose.
        let capabilities = surface.get_capabilities(&adapter);
        if let Some(srgb) = capabilities
            .formats
            .iter()
            .copied()
            .find(wgpu::TextureFormat::is_srgb)
        {
            config.format = srgb;
        }
        let format = config.format;

        surface.configure(&device, &config);

        Self::build(
            device,
            queue,
            describe_adapter(&adapter),
            Target::Window { surface, config },
            width.max(1),
            height.max(1),
            format,
        )
    }

    /// Creates a backend drawing into a texture it owns, with no window.
    ///
    /// **This is the one whose output can be read back** — see [`RenderBackend::capture`]. Agent mode
    /// has no window (ADR 0016 launches a game headless), so this is the path that gives the agent
    /// eyes, and ADR 0021 named that as capture's whole purpose.
    ///
    /// Everything after the device is identical to [`WgpuBackend::new`]: same shaders, same
    /// pipelines, same passes. Only where the frame lands differs, which is what makes a captured
    /// image evidence about the renderer that actually ships.
    ///
    /// # Errors
    ///
    /// [`RenderError::InitFailed`] if no adapter or device is available — which is the ordinary case
    /// on a machine with no GPU at all, and worth handling rather than panicking, since a headless
    /// CI runner is exactly such a machine.
    pub fn offscreen(width: u32, height: u32) -> Result<Self, RenderError> {
        let instance =
            wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());

        // No `compatible_surface`, which is the only difference in adapter selection — and what
        // allows a software adapter to answer on a machine with no GPU.
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            ..Default::default()
        }))
        .map_err(|error| RenderError::InitFailed {
            backend: "wgpu",
            reason: format!(
                "no GPU adapter found for offscreen rendering: {error}. \
                 A machine with no GPU and no software fallback cannot capture; \
                 `render.describe` answers the same questions without one."
            ),
        })?;

        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("amadeo offscreen device"),
            // Same as the windowed path: ask for timestamps only if this adapter already has them
            // (exit gate 4). **This is the one that matters for measurement** — the frame budget is
            // taken offscreen, so an offscreen device without the feature can time nothing.
            required_features: adapter.features() & wgpu::Features::TIMESTAMP_QUERY,
            required_limits: wgpu::Limits::default(),
            ..Default::default()
        }))
        .map_err(|error| RenderError::InitFailed {
            backend: "wgpu",
            reason: format!("the GPU adapter refused a device: {error}"),
        })?;

        // sRGB to match what a window surface is configured with, so a captured image and a
        // displayed one agree about colour rather than differing by a gamma curve.
        let format = wgpu::TextureFormat::Rgba8UnormSrgb;
        let width = width.max(1);
        let height = height.max(1);
        let texture = offscreen_texture(&device, width, height, format);

        Self::build(
            device,
            queue,
            describe_adapter(&adapter),
            Target::Offscreen { texture },
            width,
            height,
            format,
        )
    }

    /// Which adapter answered, and whether it is real hardware.
    ///
    /// **Check `software` before asserting any timing bound.** See [`AdapterDescription`] for the
    /// red CI run that made this method exist.
    #[must_use]
    pub fn adapter(&self) -> &AdapterDescription {
        &self.adapter
    }

    /// Everything both constructors share: shaders, pipelines, buffers, samplers.
    fn build(
        device: wgpu::Device,
        queue: wgpu::Queue,
        adapter: AdapterDescription,
        target: Target,
        width: u32,
        height: u32,
        format: wgpu::TextureFormat,
    ) -> Result<Self, RenderError> {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("amadeo quad shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("quad.wgsl").into()),
        });

        // One slot per camera, addressed by a dynamic offset. ADR 0031 made a world able to hold
        // several cameras, and each needs its own uniform *while the encoder is still recording* —
        // writing the buffer once per view instead would just overwrite the previous write, since
        // every queue write lands before the single submit at the end.
        //
        // The alignment is a hardware rule (commonly 256 bytes) rather than a choice, so it is read
        // off the device rather than assumed.
        let camera_stride = camera_stride(&device);
        let camera_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("amadeo camera uniforms"),
            size: camera_stride * INITIAL_VIEW_CAPACITY as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let camera_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("amadeo camera layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: true,
                    // Stated so the driver can validate a dynamic offset against it: a bind group
                    // that binds the whole buffer would otherwise let an out-of-range offset through
                    // to the GPU.
                    min_binding_size: wgpu::BufferSize::new(size_of::<GpuCamera>() as u64),
                },
                count: None,
            }],
        });

        let camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("amadeo camera bind group"),
            layout: &camera_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &camera_buffer,
                    offset: 0,
                    size: wgpu::BufferSize::new(size_of::<GpuCamera>() as u64),
                }),
            }],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("amadeo quad pipeline layout"),
            bind_group_layouts: &[Some(&camera_layout)],
            // No `var<immediate>` in the shader, so no immediate-data budget is needed. A non-zero
            // value here would require the IMMEDIATES feature, which not every adapter has.
            immediate_size: 0,
        });

        // Three vec4s per instance, matching `GpuInstance` and the shader's `InstanceInput`.
        let instance_layout = wgpu::VertexBufferLayout {
            array_stride: size_of::<GpuInstance>() as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &wgpu::vertex_attr_array![
                0 => Float32x4,
                1 => Float32x4,
                2 => Float32x4,
            ],
        };

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("amadeo quad pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[Some(instance_layout)],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    // The transient, not the destination: every camera draws into the graph's
                    // scene image and the present pass puts it on screen.
                    format: SCENE_FORMAT,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                // Four vertices per quad as a strip, so no index buffer is needed.
                topology: wgpu::PrimitiveTopology::TriangleStrip,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                // No culling: a quad flipped by a negative scale must still be visible.
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: None,
            multisample: SCENE_MULTISAMPLE,
            multiview_mask: None,
            cache: None,
        });

        let instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("amadeo instance buffer"),
            size: (size_of::<GpuInstance>() * INITIAL_INSTANCE_CAPACITY) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // --- The sprite pipeline, from here down. ---
        //
        // A second pipeline rather than one shared with quads, because the two draw genuinely
        // different things: a quad reads no texture and needs no second bind group, and folding
        // them together would mean binding a dummy texture for every untextured rectangle. They do
        // share the camera bind group, which is the only state that is actually common.

        // Group 1: the texture and its sampler. `filterable: false` on the sample type together
        // with `NonFiltering` on the sampler is what keeps this working on every adapter --
        // requiring a filterable float texture is a capability some hardware does not advertise for
        // every format, and nearest-neighbour sampling does not need it.
        let texture_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("amadeo sprite texture layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::NonFiltering),
                    count: None,
                },
            ],
        });

        // What a 3D surface binds: **every** texture its material names, plus one shared sampler.
        //
        // Filterable where the sprite layout above is not. A separate layout rather than relaxing
        // that one, because its `filterable: false` is a deliberate compatibility choice: requiring
        // a filterable float texture is a capability some adapters do not advertise for every
        // format, and nearest-neighbour sprites do not need it. Asking for it only here confines the
        // requirement to the 3D path.
        //
        // One sampler for both textures, because a normal map wants exactly what a base colour map
        // wants — repeat, filter, mip. They are sampled at the same coordinate on the same surface;
        // a second sampler would be the same settings under a different name.
        let surface_texture_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("amadeo surface texture layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                    // The normal map (ADR 0047). Always bound — a material naming none gets a 1×1
                    // flat-blue placeholder, which decodes to "leaning nowhere" and leaves the
                    // geometric normal untouched. Same trick as the white base-colour placeholder:
                    // one pipeline serves both instead of a textured and an untextured variant that
                    // can drift apart.
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    // The metallic-roughness map (ADR 0048): green is roughness, blue is metallic.
                    // Always bound, and the placeholder is **white** — because these values
                    // multiply the material's scalars, and one is the identity of a multiply.
                    wgpu::BindGroupLayoutEntry {
                        binding: 3,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                ],
            });

        // Bind group 3: the environment a view is lit by (ADR 0049). Two cube textures and a
        // sampler.
        //
        // **Group 3 is the last one wgpu guarantees**, and it is the right home rather than merely
        // the only slot left: 0 is the view, 1 is the shadow map, 2 is the material. An environment
        // changes per *view* like the first two, not per draw like the third, so it is set once per
        // camera rather than once per object.
        let environment_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("amadeo environment layout"),
                entries: &[
                    // Diffuse irradiance: for each direction, the light a matte surface facing it gets.
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::Cube,
                            multisampled: false,
                        },
                        count: None,
                    },
                    // Specular: the environment blurred once per roughness level, as a mip chain.
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::Cube,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            });

        // Clamped and linear, with linear filtering *between* mip levels too.
        //
        // That last part is what makes roughness read as continuous rather than stepping between six
        // discrete looks: the shader asks for a fractional mip level and the hardware blends the two
        // either side. Clamped rather than repeating because a direction cannot run off the end of a
        // cube — there is no edge to wrap around.
        let environment_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("amadeo environment sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("amadeo sprite sampler"),
            // Clamping rather than repeating: a `region` is a rectangle inside a shared sheet, and
            // repeating would bleed a neighbouring tile in at the seam.
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            // **Pinned to level 0**, now that textures carry mip levels. A sprite is pixel art drawn
            // at roughly its own size and wants to stay crisp; letting it drop to a smaller level
            // would blur exactly the art `games/vault` hand-authored. Surfaces get the opposite
            // treatment below, which is the point of there being two samplers.
            lod_min_clamp: 0.0,
            lod_max_clamp: 0.0,
            ..Default::default()
        });

        // The other half of the same choice, for 3D surfaces. Repeating, because a material texture
        // tiles across geometry whose UVs run well past 1.0 -- terrain projects its own from world
        // coordinates, so clamping would smear one edge pixel across the entire landscape. Filtered,
        // because a surface is seen at every angle and every distance, where a sprite is seen
        // face-on at a fixed size.
        let surface_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("amadeo surface sampler"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            address_mode_w: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            // **Linear between mip levels, not nearest** (ADR 0045). Nearest snaps from one level to
            // the next, and the snap is visible as a band sliding across the ground as the camera
            // moves — "mip banding". Blending across the boundary is what makes the transition
            // invisible, and it is the cheap half of the fix.
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            // **Anisotropic filtering**, the other half, and the single most visible texture setting
            // there is. A surface seen at a glancing angle — which is most of a landscape — is
            // squashed far more along one axis than the other, so any single mip level is either too
            // blurry across it or too noisy along it. Sampling several times along the direction of
            // the squash is what keeps ground sharp into the distance.
            //
            // 16 is the conventional maximum. wgpu requires all three filters to be `Linear` for it,
            // which is why this sampler exists separately from the sprite one at all.
            anisotropy_clamp: 16,
            ..Default::default()
        });

        let sprite_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("amadeo sprite shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("sprite.wgsl").into()),
        });

        let sprite_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("amadeo sprite pipeline layout"),
                // Group 0 is the camera, shared with the quad pipeline; group 1 is the texture,
                // rebound once per batch.
                bind_group_layouts: &[Some(&camera_layout), Some(&texture_layout)],
                immediate_size: 0,
            });

        // Four vec4s per instance, matching `GpuSprite` and the shader's `InstanceInput`. Locations
        // continue from 0 because this is a separate pipeline with its own shader, not a
        // continuation of the quad one.
        let sprite_layout = wgpu::VertexBufferLayout {
            array_stride: size_of::<GpuSprite>() as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &wgpu::vertex_attr_array![
                0 => Float32x4,
                1 => Float32x4,
                2 => Float32x4,
                3 => Float32x4,
            ],
        };

        let sprite_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("amadeo sprite pipeline"),
            layout: Some(&sprite_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &sprite_shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[Some(sprite_layout)],
            },
            fragment: Some(wgpu::FragmentState {
                module: &sprite_shader,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    // The transient, as with the quad pipeline above.
                    format: SCENE_FORMAT,
                    // Alpha blending, because a sprite sheet's transparent margins are the normal
                    // case rather than an effect.
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleStrip,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                // No culling: a sprite flipped by a negative scale must still be visible, which is
                // how a character faces left.
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: None,
            multisample: SCENE_MULTISAMPLE,
            multiview_mask: None,
            cache: None,
        });

        let sprite_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("amadeo sprite buffer"),
            size: (size_of::<GpuSprite>() * INITIAL_INSTANCE_CAPACITY) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // --- The present pipeline. ---
        //
        // Reads a finished transient and writes the destination, so it is the one pipeline whose
        // target format is `format` rather than `SCENE_FORMAT`. It reuses `texture_layout`, since
        // sampling the scene image needs exactly what sampling a sprite's texture needs: one texture
        // and one non-filtering sampler.
        let present_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("amadeo present shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("present.wgsl").into()),
        });

        let present_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("amadeo present pipeline layout"),
                // No camera: a full-screen pass has no view to be seen from.
                bind_group_layouts: &[Some(&texture_layout)],
                immediate_size: 0,
            });

        let present_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("amadeo present pipeline"),
            layout: Some(&present_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &present_shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                // No vertex buffer at all: the three corners come from the vertex index.
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &present_shader,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    // Replace rather than blend. The scene image already has everything composited
                    // into it, and blending it again would mix it with whatever the destination
                    // happened to contain.
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        // --- The mesh pipeline. ---
        //
        // The first pipeline in this backend with a real vertex buffer: geometry comes from a mesh
        // asset rather than from corners derivable from the vertex index. Two buffers, then —
        // per-vertex and per-instance — plus a depth state, which is what makes nearer geometry
        // hide further geometry rather than whatever drew last winning.
        let mesh_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("amadeo mesh shader"),
            source: wgpu::ShaderSource::Wgsl(
                concat!(include_str!("view.wgsl"), include_str!("mesh.wgsl")).into(),
            ),
        });

        let mesh_view_stride = {
            let alignment = u64::from(device.limits().min_uniform_buffer_offset_alignment).max(1);
            let size = size_of::<GpuMeshView>() as u64;
            size.div_ceil(alignment) * alignment
        };

        let mesh_view_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("amadeo mesh view layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: true,
                    min_binding_size: wgpu::BufferSize::new(size_of::<GpuMeshView>() as u64),
                },
                count: None,
            }],
        });

        let mesh_view_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("amadeo mesh view uniforms"),
            size: mesh_view_stride * INITIAL_VIEW_CAPACITY as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let mesh_view_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("amadeo mesh view bind group"),
            layout: &mesh_view_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &mesh_view_buffer,
                    offset: 0,
                    size: wgpu::BufferSize::new(size_of::<GpuMeshView>() as u64),
                }),
            }],
        });

        // The shadow pass's own uniform: one matrix per (view, cascade). Mirrors the mesh view
        // buffer above in every respect except size — see `GpuShadowView` for why it is separate.
        let shadow_view_stride = {
            let alignment = u64::from(device.limits().min_uniform_buffer_offset_alignment).max(1);
            let size = size_of::<GpuShadowView>() as u64;
            size.div_ceil(alignment) * alignment
        };

        let shadow_view_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("amadeo shadow view layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    // Vertex only: a shadow pass has no fragment stage at all.
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: true,
                        min_binding_size: wgpu::BufferSize::new(size_of::<GpuShadowView>() as u64),
                    },
                    count: None,
                }],
            });

        let shadow_view_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("amadeo shadow view uniforms"),
            size: shadow_view_stride * (INITIAL_VIEW_CAPACITY * crate::MAX_SHADOW_LAYERS) as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let shadow_view_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("amadeo shadow view bind group"),
            layout: &shadow_view_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &shadow_view_buffer,
                    offset: 0,
                    size: wgpu::BufferSize::new(size_of::<GpuShadowView>() as u64),
                }),
            }],
        });

        // --- Shadow maps (ADR 0038). ---
        //
        // A depth texture bound for *comparison*: the sampler is told what to compare against and
        // returns how many of the neighbouring texels passed, rather than returning a depth. That is
        // hardware PCF — a soft shadow edge for the price of one sample — and it is why this cannot
        // reuse `texture_layout`, whose sample type is `Float` and whose sampler filters.
        let shadow_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("amadeo shadow map layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        // **An array, always** — one layer when there is one cascade (ADR 0055). One
                        // declaration in the shader and one pipeline, rather than a cascaded variant
                        // and a single-map variant that can drift apart.
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison),
                    count: None,
                },
            ],
        });

        let shadow_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("amadeo shadow sampler"),
            // Clamped, so sampling just outside the map repeats its edge rather than wrapping to the
            // far side — which would put a shadow from one corner of the map onto the opposite one.
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            // Linear here does not blur depths — with a comparison sampler it blends the *results*
            // of the comparisons, which is exactly the softening wanted.
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            // `Less`: a fragment is lit when it is nearer to the light than what the map recorded.
            // The same direction the depth test uses, and for the same reason.
            compare: Some(wgpu::CompareFunction::Less),
            ..Default::default()
        });

        // A 1×1 shadow map that is always available, so the mesh pipeline has something to bind when
        // nothing casts shadows.
        //
        // The same argument as the placeholder texture in `TextureCache`: the last resort must be
        // something that cannot itself be missing. Without it the mesh pipeline would need a second
        // variant compiled without the shadow bindings, which means two pipelines, two shaders that
        // can drift, and a 2D game paying for a distinction it cannot observe.
        let shadow_placeholder = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("amadeo shadow placeholder"),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_FORMAT,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        // Bound through the same array layout as a real shadow map, so the placeholder is a
        // one-layer array rather than a plain 2D texture. wgpu infers `D2` here otherwise, and the
        // bind group would fail at *creation* with a dimension mismatch.
        let shadow_placeholder_view =
            shadow_placeholder.create_view(&wgpu::TextureViewDescriptor {
                dimension: Some(wgpu::TextureViewDimension::D2Array),
                ..Default::default()
            });
        let shadow_placeholder_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("amadeo shadow placeholder bind group"),
            layout: &shadow_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&shadow_placeholder_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&shadow_sampler),
                },
            ],
        });

        // A 1×1 opaque white texture, for a material that names no base colour texture.
        //
        // **White rather than the magenta `TextureCache` placeholder, and the difference is the
        // point.** Magenta means *this asset is missing and you should notice*; an untextured
        // material is not missing anything, it is a surface whose colour is entirely its
        // `base_colour`. White is the identity of the multiply, so binding this is arithmetically
        // the same as not sampling at all — which is what lets there be one mesh pipeline instead of
        // a textured one and an untextured one that can drift apart.
        let (white_placeholder, white_placeholder_view) = single_pixel_texture(
            &device,
            &queue,
            "amadeo white placeholder",
            wgpu::TextureFormat::Rgba8UnormSrgb,
            [255, 255, 255, 255],
        );

        // A 1×1 black texture, bound as the glow when bloom is off.
        //
        // **Black is the identity of an addition**, exactly as white is of a multiply — so the post
        // pass samples it, adds nothing, and there is one post pipeline rather than a bloomed one and
        // a plain one. The alternative is a shader branch on a uniform, which is what
        // `shadow_params.y` does for shadows; either works, and a placeholder is chosen here because
        // the post pass has no other reason to know whether bloom ran.
        let (black_placeholder, black_placeholder_view) = single_pixel_texture(
            &device,
            &queue,
            "amadeo bloom placeholder",
            wgpu::TextureFormat::Rgba8Unorm,
            [0, 0, 0, 255],
        );

        // A 1×1 flat normal, for a material that names no normal map.
        //
        // `(128, 128, 255)` is what a tangent-space normal map stores where the surface is not
        // leaning at all: the shader decodes each channel from `0..1` to `-1..1`, so this comes out
        // as `(0, 0, 1)` — straight along the geometric normal. Binding it is arithmetically the
        // same as not sampling, which is the white placeholder's argument applied to directions.
        //
        // **Linear, not sRGB.** The whole point of `PixelFormat::Rgba8Unorm` is that these bytes are
        // not colour; tagging the placeholder sRGB would decode 128 to about 55 and tilt a surface
        // that is meant to be perfectly flat.
        let (flat_normal_placeholder, flat_normal_placeholder_view) = single_pixel_texture(
            &device,
            &queue,
            "amadeo flat normal placeholder",
            wgpu::TextureFormat::Rgba8Unorm,
            [128, 128, 255, 255],
        );

        // The environment a camera gets when it names no sky, or names one that has not prefiltered
        // yet. A uniform dim neutral, which is exactly the `0.12` constant that stood in for ambient
        // light before ADR 0049 — so a game that asks for nothing shades as it always did, rather
        // than going black and making image-based lighting look like a regression.
        let neutral = crate::EnvironmentMap::solid(crate::DEFAULT_SKY);
        let (neutral_irradiance, neutral_irradiance_view) = upload_cubemap(
            &device,
            &queue,
            "amadeo neutral irradiance",
            std::slice::from_ref(&neutral.irradiance),
        );
        let (neutral_specular, neutral_specular_view) = upload_cubemap(
            &device,
            &queue,
            "amadeo neutral specular",
            &neutral.specular,
        );
        let neutral_environment_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("amadeo neutral environment"),
            layout: &environment_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&neutral_irradiance_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&neutral_specular_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&environment_sampler),
                },
            ],
        });

        let shadow_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("amadeo shadow shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shadow.wgsl").into()),
        });

        let shadow_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("amadeo shadow pipeline layout"),
                bind_group_layouts: &[Some(&shadow_view_layout)],
                immediate_size: 0,
            });

        let shadow_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("amadeo shadow pipeline"),
            layout: Some(&shadow_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shadow_shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[
                    Some(wgpu::VertexBufferLayout {
                        array_stride: size_of::<GpuVertex>() as u64,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &wgpu::vertex_attr_array![
                            0 => Float32x3,
                            1 => Float32x3,
                            2 => Float32x2,
                        ],
                    }),
                    Some(wgpu::VertexBufferLayout {
                        array_stride: size_of::<GpuMeshInstance>() as u64,
                        step_mode: wgpu::VertexStepMode::Instance,
                        attributes: &wgpu::vertex_attr_array![
                            3 => Float32x4,
                            4 => Float32x4,
                            5 => Float32x4,
                            6 => Float32x4,
                            7 => Float32x4,
                            8 => Float32x4,
                        ],
                    }),
                ],
            },
            // No fragment stage: a shadow pass writes depth and nothing else, so there is no colour
            // for one to return.
            fragment: None,
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                // **Front** faces are culled here where the mesh pass culls back ones, which is
                // deliberate and is the cheapest fix for shadow acne there is: recording the far
                // side of each object moves the stored depth away from the surface being lit, so the
                // surface stops shadowing itself. It costs correctness only for geometry with no
                // thickness, which is what `shadow_bias` is still there for.
                cull_mode: Some(wgpu::Face::Front),
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        let mesh_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("amadeo mesh pipeline layout"),
            // All four groups wgpu guarantees, and each holds things that change at a different
            // rate: 0 the view, 1 the shadow map, 2 the material's textures, 3 the environment.
            // Groups 0, 1 and 3 are set once per camera; only group 2 changes per draw.
            bind_group_layouts: &[
                Some(&mesh_view_layout),
                Some(&shadow_layout),
                Some(&surface_texture_layout),
                Some(&environment_layout),
            ],
            immediate_size: 0,
        });

        let mesh_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("amadeo mesh pipeline"),
            layout: Some(&mesh_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &mesh_shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[
                    // Per vertex: position, normal, uv, tangent.
                    Some(wgpu::VertexBufferLayout {
                        array_stride: size_of::<GpuVertex>() as u64,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &wgpu::vertex_attr_array![
                            0 => Float32x3,
                            1 => Float32x3,
                            2 => Float32x2,
                            // 9, not 3: locations are one namespace shared with the instance
                            // buffer below, which already claimed 3 through 8. Renumbering those
                            // to keep the vertex attributes contiguous would touch the shadow
                            // pipeline and both shaders to buy nothing.
                            9 => Float32x4,
                        ],
                    }),
                    // Per instance: four matrix columns, two colours, and the surface parameters.
                    // Locations continue from 3, because they share one shader with the vertex
                    // attributes above — which is why the tangent had to take 9 and this takes 10.
                    Some(wgpu::VertexBufferLayout {
                        array_stride: size_of::<GpuMeshInstance>() as u64,
                        step_mode: wgpu::VertexStepMode::Instance,
                        attributes: &wgpu::vertex_attr_array![
                            3 => Float32x4,
                            4 => Float32x4,
                            5 => Float32x4,
                            6 => Float32x4,
                            7 => Float32x4,
                            8 => Float32x4,
                            10 => Float32x4,
                        ],
                    }),
                ],
            },
            fragment: Some(wgpu::FragmentState {
                module: &mesh_shader,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: SCENE_FORMAT,
                    // Opaque. Transparent meshes need back-to-front sorting within a `SortOrder`
                    // (ADR 0018 says so), and doing that before there is anything transparent to
                    // sort would be guessing at the shape of a problem nobody has yet.
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                // **Nothing is culled, and that fixes seeing the sky from underground.**
                //
                // Terrain from surface nets is an open *surface*, not a closed solid: it is the
                // boundary between rock and air and it has no underside. Culling back faces meant
                // that from below it vanished entirely — so a camera under the ground looked
                // straight through the world to the skybox, which is what "digging down shows the
                // sky" actually was.
                //
                // The reason this is safe rather than a trade is worth stating: **for a closed mesh
                // it changes nothing at all.** A box's back faces are always behind its front faces,
                // so the depth test rejects them and the picture is identical — the cost is
                // rasterising fragments that early-Z then discards. The only views that change are
                // exactly the ones that were wrong.
                //
                // The shader flips the normal for a back face (`@builtin(front_facing)`), without
                // which the underside of the ground would be lit as though it faced the sky.
                //
                // Per-material would be the grown-up version and is deliberately not done yet: it is
                // a field on every `.material` file (Q32) to buy back overdraw the frame budget says
                // is nowhere near mattering.
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(true),
                // `Less` because the projection puts near at 0 and far at 1 — a fragment passes
                // when it is nearer than what is already there. Reversed depth would flip this.
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: SCENE_MULTISAMPLE,
            multiview_mask: None,
            cache: None,
        });

        // The sky, drawn behind everything (ADR 0049).
        //
        // **Its own layout with two groups rather than the mesh pipeline's four.** Sharing that one
        // looked tidier — the bind groups would already be set — but the sky has to draw for a
        // camera with *no meshes at all*, where nothing has bound the shadow map or a material. Two
        // groups is what this shader actually reads, so it is also what it declares.
        let sky_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("amadeo sky shader"),
            // Prepended with the shared view declaration, exactly as the mesh shader is — the two
            // read the same buffer at the same binding, and one declaration is what stops them
            // disagreeing about its layout. See `view.wgsl` for the bug that caused.
            source: wgpu::ShaderSource::Wgsl(
                concat!(include_str!("view.wgsl"), include_str!("sky.wgsl")).into(),
            ),
        });
        let sky_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("amadeo sky pipeline layout"),
            bind_group_layouts: &[Some(&mesh_view_layout), Some(&environment_layout)],
            immediate_size: 0,
        });
        let sky_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("amadeo sky pipeline"),
            layout: Some(&sky_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &sky_shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                // No vertex buffers at all: three vertices derived from the index.
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &sky_shader,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: SCENE_FORMAT,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                // **No culling.** The one triangle covers the screen and overhangs it, and which way
                // it happens to wind is an artefact of deriving it from an index rather than
                // something meaningful. Culling it would be a coin flip on an invisible screen.
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                // **Writes nothing.** The sky is infinitely far away; recording a depth for it would
                // occlude anything drawn afterwards.
                depth_write_enabled: Some(false),
                // `LessEqual` rather than the geometry's `Less`, and the difference is the whole
                // trick: the sky sits at exactly the far plane, which is exactly the cleared value,
                // so `Less` would reject every one of its fragments and draw nothing at all.
                depth_compare: Some(wgpu::CompareFunction::LessEqual),
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: SCENE_MULTISAMPLE,
            multiview_mask: None,
            cache: None,
        });

        let mesh_instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("amadeo mesh instance buffer"),
            size: (size_of::<GpuMeshInstance>() * INITIAL_INSTANCE_CAPACITY) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // --- The post pipeline. ---
        //
        // Group 0 is the scene image, the same layout every sampled texture uses. Group 1 is the
        // frame's `Environment`, flattened into three vec4s.
        // Prepended with the shared uniform declaration, as the mesh and sky shaders are with
        // `view.wgsl` — `post.wgsl` and `bloom.wgsl` read the same buffer at the same binding, and
        // one declaration is what keeps them from disagreeing about its layout.
        let post_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("amadeo post shader"),
            source: wgpu::ShaderSource::Wgsl(
                concat!(include_str!("post_uniform.wgsl"), include_str!("post.wgsl")).into(),
            ),
        });

        let bloom_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("amadeo bloom shader"),
            source: wgpu::ShaderSource::Wgsl(
                concat!(
                    include_str!("post_uniform.wgsl"),
                    include_str!("bloom.wgsl")
                )
                .into(),
            ),
        });

        let post_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("amadeo post uniform layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: wgpu::BufferSize::new(size_of::<GpuPost>() as u64),
                },
                count: None,
            }],
        });

        let post_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("amadeo post uniforms"),
            size: size_of::<GpuPost>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let post_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("amadeo post bind group"),
            layout: &post_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: post_buffer.as_entire_binding(),
            }],
        });

        // The glow slot's stand-in, built against the same layout a pooled colour transient gets, so
        // the post pass binds one or the other with no other difference.
        let bloom_placeholder_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("amadeo bloom placeholder bind group"),
            layout: &texture_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&black_placeholder_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });

        let post_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("amadeo post pipeline layout"),
            // Group 2 is the blurred glow, bound to a 1×1 black placeholder when bloom is off — the
            // same one-pipeline argument the shadow map and the base-colour texture both make.
            bind_group_layouts: &[
                Some(&texture_layout),
                Some(&post_layout),
                Some(&texture_layout),
            ],
            immediate_size: 0,
        });

        // Bloom's three passes share one pipeline layout: each reads one image and the post uniform.
        let bloom_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("amadeo bloom pipeline layout"),
                bind_group_layouts: &[Some(&texture_layout), Some(&post_layout)],
                immediate_size: 0,
            });

        // One pipeline per entry point. The blur's *direction* is a property of the pass rather than
        // of the frame, so it is two pipelines over one shader rather than a uniform written twice
        // with two values and a second bind group to hold them.
        let bloom_pipeline = |label: &str, entry: &str| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&bloom_pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &bloom_shader,
                    entry_point: Some("vs_main"),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    buffers: &[],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &bloom_shader,
                    entry_point: Some(entry),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        // HDR, like the scene target: a glow clamped to white before the tonemap
                        // sees it has already lost the difference bloom exists to show.
                        format: SCENE_FORMAT,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    strip_index_format: None,
                    front_face: wgpu::FrontFace::Ccw,
                    cull_mode: None,
                    polygon_mode: wgpu::PolygonMode::Fill,
                    unclipped_depth: false,
                    conservative: false,
                },
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview_mask: None,
                cache: None,
            })
        };
        let bloom_bright_pipeline = bloom_pipeline("amadeo bloom bright pipeline", "fs_bright");
        let bloom_blur_x_pipeline =
            bloom_pipeline("amadeo bloom blur x pipeline", "fs_blur_horizontal");
        let bloom_blur_y_pipeline =
            bloom_pipeline("amadeo bloom blur y pipeline", "fs_blur_vertical");

        let post_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("amadeo post pipeline"),
            layout: Some(&post_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &post_shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &post_shader,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    // The displayable image, not the destination — see `OUTPUT_FORMAT`.
                    format: OUTPUT_FORMAT,
                    // Replace: this pass produces the whole picture from the scene image.
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        Ok(Self {
            target,
            adapter,
            // Built before the device is moved into the struct. `None` on an adapter without
            // `TIMESTAMP_QUERY`, which is the whole graceful-degradation story in one line.
            timing: GpuTiming::new(&device, &queue),
            last_timing: None,
            device,
            queue,
            width,
            height,
            format,
            pipeline,
            sprite_pipeline,
            present_pipeline,
            mesh_pipeline,
            shadow_pipeline,
            sky_pipeline,
            shadow_layout,
            shadow_sampler,
            shadow_placeholder_bind_group,
            white_placeholder_view,
            flat_normal_placeholder_view,
            material_bind_groups: BTreeMap::new(),
            multisampled_target: None,
            multisampled_size: (0, 0),
            environments: BTreeMap::new(),
            neutral_environment_bind_group,
            environment_layout,
            environment_sampler,
            neutral_irradiance,
            neutral_specular,
            white_placeholder,
            flat_normal_placeholder,
            shadow_placeholder,
            meshes: BTreeMap::new(),
            mesh_view_buffer,
            mesh_view_bind_group,
            mesh_view_stride,
            mesh_view_capacity: INITIAL_VIEW_CAPACITY,
            mesh_view_layout,
            shadow_view_buffer,
            shadow_view_bind_group,
            shadow_view_stride,
            shadow_view_capacity: INITIAL_VIEW_CAPACITY * crate::MAX_SHADOW_LAYERS,
            shadow_view_layout,
            mesh_instance_buffer,
            mesh_instance_capacity: INITIAL_INSTANCE_CAPACITY,
            post_pipeline,
            post_buffer,
            post_bind_group,
            bloom_bright_pipeline,
            bloom_blur_x_pipeline,
            bloom_blur_y_pipeline,
            bloom_placeholder_bind_group,
            black_placeholder,
            black_placeholder_view,
            transients: Vec::new(),
            capture_source: None,
            camera_buffer,
            camera_bind_group,
            camera_stride,
            camera_capacity: INITIAL_VIEW_CAPACITY,
            camera_layout,
            instance_buffer,
            instance_capacity: INITIAL_INSTANCE_CAPACITY,
            sprite_buffer,
            sprite_capacity: INITIAL_INSTANCE_CAPACITY,
            textures: BTreeMap::new(),
            texture_layout,
            surface_texture_layout,
            sampler,
            surface_sampler,
        })
    }

    /// Grows the instance buffer if this frame needs more room than it has.
    ///
    /// Doubles rather than fitting exactly, so a steadily growing scene reallocates a logarithmic
    /// number of times instead of once per frame.
    fn ensure_instance_capacity(&mut self, needed: usize) {
        if needed <= self.instance_capacity {
            return;
        }
        let mut capacity = self.instance_capacity.max(1);
        while capacity < needed {
            capacity *= 2;
        }

        self.instance_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("amadeo instance buffer"),
            size: (size_of::<GpuInstance>() * capacity) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.instance_capacity = capacity;
    }

    /// Grows the camera uniform buffer, and rebuilds the bind group that points at it.
    ///
    /// Unlike the instance buffers, the bind group has to be recreated too — it names a specific
    /// buffer, so a new buffer means a stale binding otherwise. Doubling, for the same reason.
    fn ensure_camera_capacity(&mut self, needed: usize) {
        if needed <= self.camera_capacity {
            return;
        }
        let mut capacity = self.camera_capacity.max(1);
        while capacity < needed {
            capacity *= 2;
        }

        self.camera_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("amadeo camera uniforms"),
            size: self.camera_stride * capacity as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.camera_bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("amadeo camera bind group"),
            layout: &self.camera_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &self.camera_buffer,
                    offset: 0,
                    size: wgpu::BufferSize::new(size_of::<GpuCamera>() as u64),
                }),
            }],
        });
        self.camera_capacity = capacity;
    }

    /// One view's target rectangle in physical pixels, as an origin.
    fn viewport_origin(&self, view: &View) -> (f32, f32) {
        let rect = view.camera.viewport;
        (rect[0] * self.width as f32, rect[1] * self.height as f32)
    }

    /// One view's target rectangle in physical pixels, as a size.
    fn viewport_pixels(&self, view: &View) -> (f32, f32) {
        let rect = view.camera.viewport;
        (rect[2] * self.width as f32, rect[3] * self.height as f32)
    }

    /// The same doubling growth for the sprite buffer.
    fn ensure_sprite_capacity(&mut self, needed: usize) {
        if needed <= self.sprite_capacity {
            return;
        }
        let mut capacity = self.sprite_capacity.max(1);
        while capacity < needed {
            capacity *= 2;
        }

        self.sprite_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("amadeo sprite buffer"),
            size: (size_of::<GpuSprite>() * capacity) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.sprite_capacity = capacity;
    }

    /// Grows the mesh view uniform buffer, and rebuilds the bind group naming it.
    fn ensure_mesh_view_capacity(&mut self, needed: usize) {
        if needed <= self.mesh_view_capacity {
            return;
        }
        let mut capacity = self.mesh_view_capacity.max(1);
        while capacity < needed {
            capacity *= 2;
        }

        self.mesh_view_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("amadeo mesh view uniforms"),
            size: self.mesh_view_stride * capacity as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.mesh_view_bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("amadeo mesh view bind group"),
            layout: &self.mesh_view_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &self.mesh_view_buffer,
                    offset: 0,
                    size: wgpu::BufferSize::new(size_of::<GpuMeshView>() as u64),
                }),
            }],
        });
        self.mesh_view_capacity = capacity;
    }

    /// The same doubling growth for the shadow pass's uniform, sized in (view, cascade) slots.
    fn ensure_shadow_view_capacity(&mut self, needed: usize) {
        if needed <= self.shadow_view_capacity {
            return;
        }
        let mut capacity = self.shadow_view_capacity.max(1);
        while capacity < needed {
            capacity *= 2;
        }

        self.shadow_view_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("amadeo shadow view uniforms"),
            size: self.shadow_view_stride * capacity as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.shadow_view_bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("amadeo shadow view bind group"),
            layout: &self.shadow_view_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &self.shadow_view_buffer,
                    offset: 0,
                    size: wgpu::BufferSize::new(size_of::<GpuShadowView>() as u64),
                }),
            }],
        });
        self.shadow_view_capacity = capacity;
    }

    /// The same doubling growth for the mesh instance buffer.
    fn ensure_mesh_instance_capacity(&mut self, needed: usize) {
        if needed <= self.mesh_instance_capacity {
            return;
        }
        let mut capacity = self.mesh_instance_capacity.max(1);
        while capacity < needed {
            capacity *= 2;
        }
        self.mesh_instance_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("amadeo mesh instance buffer"),
            size: (size_of::<GpuMeshInstance>() * capacity) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.mesh_instance_capacity = capacity;
    }

    /// Creates one physical texture a graph transient can be drawn into and then sampled from.
    ///
    /// The three usages are each load-bearing: `RENDER_ATTACHMENT` to draw into it,
    /// `TEXTURE_BINDING` so the present pass can sample it, and `COPY_SRC` so `capture` can read it
    /// back — which is the usage a window's own image can never have, and therefore the reason a
    /// windowed run can capture at all.
    fn create_transient(&self, width: u32, height: u32, format: TargetFormat) -> PooledTexture {
        // The scene depth buffer is only ever attached, never sampled or read back — so it asks for
        // neither `TEXTURE_BINDING` nor `COPY_SRC`. Requesting usages nothing needs is not free:
        // some backends choose a less efficient memory layout to satisfy them.
        //
        // A shadow map is the exception, and the *reason* the two depth formats are separate
        // variants: it is drawn into and then sampled, so it needs the binding usage the scene depth
        // buffer deliberately does without.
        let usage = match format {
            TargetFormat::Depth32Ms => wgpu::TextureUsages::RENDER_ATTACHMENT,
            TargetFormat::ShadowMap32 { .. } => {
                wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING
            }
            TargetFormat::Srgb8 | TargetFormat::Hdr16 => {
                wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::COPY_SRC
            }
        };

        // Only a shadow map has more than one, and it has one per cascade (ADR 0055).
        let layers = match format {
            TargetFormat::ShadowMap32 { layers } => layers.max(1),
            _ => 1,
        };

        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("amadeo transient target"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: layers,
            },
            mip_level_count: 1,
            sample_count: match format {
                TargetFormat::Depth32Ms => SAMPLE_COUNT,
                _ => 1,
            },
            dimension: wgpu::TextureDimension::D2,
            format: wgpu_format(format),
            usage,
            view_formats: &[],
        });

        // A shadow map is *sampled* as an array — always, even with one layer, so the mesh shader
        // has one declaration and the backend one pipeline. wgpu infers `D2` for a one-layer texture
        // unless told otherwise, and a `texture_depth_2d_array` binding refuses a `D2` view at bind
        // group creation, so this cannot be left to the default.
        let view = texture.create_view(&wgpu::TextureViewDescriptor {
            dimension: match format {
                TargetFormat::ShadowMap32 { .. } => Some(wgpu::TextureViewDimension::D2Array),
                _ => None,
            },
            ..Default::default()
        });

        // One view per layer, because a render pass attaches exactly one — so drawing four cascades
        // is four passes into four views of one texture. Empty for everything else.
        let layer_views = match format {
            TargetFormat::ShadowMap32 { .. } => (0..layers)
                .map(|layer| {
                    texture.create_view(&wgpu::TextureViewDescriptor {
                        label: Some("amadeo shadow cascade layer"),
                        dimension: Some(wgpu::TextureViewDimension::D2),
                        base_array_layer: layer,
                        array_layer_count: Some(1),
                        ..Default::default()
                    })
                })
                .collect(),
            _ => Vec::new(),
        };
        // Three cases, and each is a different answer to "how would a later pass sample this".
        // See `PooledTexture::bind_group`.
        let bind_group = match format {
            // Nothing samples the scene depth buffer, so it gets no bind group at all. A
            // multisampled one could not be sampled without resolving even if something wanted to.
            TargetFormat::Depth32Ms => None,
            // A shadow map is sampled through the *comparison* layout, not the colour one: its
            // sample type is `Depth`, and building a colour bind group against it fails at
            // creation rather than at draw.
            TargetFormat::ShadowMap32 { .. } => {
                Some(self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("amadeo shadow map bind group"),
                    layout: &self.shadow_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(&view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::Sampler(&self.shadow_sampler),
                        },
                    ],
                }))
            }
            TargetFormat::Srgb8 | TargetFormat::Hdr16 => {
                Some(self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("amadeo transient bind group"),
                    layout: &self.texture_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(&view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::Sampler(&self.sampler),
                        },
                    ],
                }))
            }
        };

        PooledTexture {
            width,
            height,
            format,
            texture,
            view,
            layer_views,
            bind_group,
        }
    }

    /// Draws the scene from a light's point of view into a shadow map — ADR 0038.
    ///
    /// The same geometry and the same instance buffer as the mesh pass, through a pipeline with no
    /// fragment stage and no colour attachment. Nothing is painted; only how far the light can see
    /// is recorded.
    ///
    /// Reuses the mesh pass's uniform at the same offset, so a view's light matrix is written once
    /// per frame rather than into two buffers that could disagree.
    fn run_shadow_pass(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        declared: &graph::Pass,
        assigned: &BTreeMap<String, usize>,
        draws: Option<&Vec<(&str, MaterialTextures, std::ops::Range<u32>)>>,
        timestamp_writes: Option<wgpu::RenderPassTimestampWrites<'_>>,
    ) {
        // Read from the declaration rather than passed alongside it: the pass already says which
        // view and which cascade it is, and handing those in as well would be two statements of one
        // fact that a call site could get out of step.
        let PassKind::Shadow {
            view: view_index,
            layer,
        } = declared.kind
        else {
            return;
        };

        // The map it draws into is declared in `writes`, which is what orders this pass before the
        // view pass that reads it — but it is bound as depth, not as colour.
        let Some(pooled) = declared.writes.first().and_then(|name| assigned.get(name)) else {
            return;
        };
        // **This cascade's layer**, not the whole array: attaching the array view would fail, since
        // a depth attachment must be a single 2D view (ADR 0055).
        let Some(map) = self.transients[*pooled].layer_views.get(layer) else {
            return;
        };

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some(declared.label.as_str()),
            color_attachments: &[],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: map,
                depth_ops: Some(wgpu::Operations {
                    // Cleared to the far plane, so anywhere no geometry covers reads as "the light
                    // sees all the way", which is "nothing is shadowed here".
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            multiview_mask: None,
            timestamp_writes,
            occlusion_query_set: None,
        });

        // A frame whose meshes have not uploaded still clears the map, which is why this returns
        // after beginning the pass rather than before it: an uncleared shadow map holds the previous
        // frame's depths, and would shadow the scene with geometry that is no longer there.
        let Some(draws) = draws else {
            return;
        };
        if draws.is_empty() {
            return;
        }

        pass.set_pipeline(&self.shadow_pipeline);
        // The slot for this (view, cascade), which is the flat index the upload above wrote to.
        let slot = view_index * crate::MAX_SHADOW_LAYERS + layer;
        pass.set_bind_group(
            0,
            &self.shadow_view_bind_group,
            &[slot as u32 * self.shadow_view_stride as u32],
        );
        pass.set_vertex_buffer(1, self.mesh_instance_buffer.slice(..));

        // The texture is ignored here on purpose: a shadow pass writes depth only, so what a surface
        // looks like cannot affect it. Only the geometry and where it sits matter.
        for (mesh_id, _texture_id, range) in draws {
            let Some(mesh) = self.meshes.get(*mesh_id) else {
                continue;
            };
            pass.set_vertex_buffer(0, mesh.vertices.slice(..));
            pass.set_index_buffer(mesh.indices.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..mesh.index_count, 0, range.clone());
        }
    }

    /// Gives every transient in the plan a physical texture, by name.
    ///
    /// Two transients share one texture when their descriptions match **and their lifetimes do not
    /// overlap** — the graph works out when each one is live, and handing the same memory to two
    /// images that are live at once is how a frame ends up with a picture nobody can explain.
    ///
    /// Textures also survive between frames, which is the saving that actually matters: a
    /// full-screen image is several megabytes, and allocating one per frame would cost more than
    /// everything else this backend does.
    fn assign_transients(&mut self, graph: &RenderGraph, plan: &Plan) -> BTreeMap<String, usize> {
        // Which pooled texture each transient took, and when that transient is live.
        let mut claimed: Vec<(usize, graph::Lifetime)> = Vec::new();
        let mut assigned = BTreeMap::new();

        for transient in graph.transients() {
            // Declared but never written. `compile` already refused anything that *reads* such a
            // resource, so there is nothing to allocate and nothing that can observe the gap.
            let Some(&life) = plan.lifetimes().get(&transient.name) else {
                continue;
            };

            // A pooled texture of the right description that is not already serving a transient
            // live at the same time.
            let mut reusable = None;
            for (index, pooled) in self.transients.iter().enumerate() {
                if pooled.width != transient.width
                    || pooled.height != transient.height
                    || pooled.format != transient.format
                {
                    continue;
                }
                let busy = claimed
                    .iter()
                    .any(|(taken, taken_life)| *taken == index && taken_life.overlaps(&life));
                if !busy {
                    reusable = Some(index);
                    break;
                }
            }

            let index = match reusable {
                Some(index) => index,
                None => {
                    let pooled =
                        self.create_transient(transient.width, transient.height, transient.format);
                    self.transients.push(pooled);
                    self.transients.len() - 1
                }
            };

            claimed.push((index, life));
            assigned.insert(transient.name.clone(), index);
        }

        assigned
    }

    /// Makes sure the multisampled colour buffer exists at the current frame size — ADR 0051.
    ///
    /// Cheap on every frame but the first and the ones after a resize: the size is compared and
    /// nothing is created when it matches. Re-creating it per frame would allocate several megabytes
    /// sixty times a second.
    fn ensure_multisampled_target(&mut self) {
        let size = (self.width.max(1), self.height.max(1));
        if self.multisampled_target.is_some() && self.multisampled_size == size {
            return;
        }

        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("amadeo multisampled scene"),
            size: wgpu::Extent3d {
                width: size.0,
                height: size.1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: SAMPLE_COUNT,
            dimension: wgpu::TextureDimension::D2,
            // The same format as the scene transient it resolves into. A resolve requires both to
            // agree, so this is not a free choice.
            format: SCENE_FORMAT,
            // **Attachment only.** A multisampled texture cannot be sampled or copied from without
            // being resolved first, and asking for usages nothing can use is not free.
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        // Replacing drops the old texture, which is what releases its video memory on a resize.
        self.multisampled_target = Some((texture, view));
        self.multisampled_size = size;
    }

    /// Makes sure a bind group exists for every texture combination this frame will draw.
    ///
    /// **Runs before the render pass opens, and has to.** Creating a bind group needs `&mut self` to
    /// cache it, and a pass holds the encoder borrowed for its whole life — so building one lazily
    /// inside the draw loop does not compile. Doing it up front is also simply better: a bind group
    /// is built once per combination rather than reconsidered per draw.
    ///
    /// A texture that has not been uploaded falls back to a placeholder rather than skipping the
    /// draw. ADR 0021 again: a material naming a texture that has not decoded yet still draws in its
    /// base colour, which is survivable and visible rather than an object that vanishes.
    fn ensure_material_bind_groups<'a>(
        &mut self,
        wanted: impl Iterator<Item = &'a MaterialTextures>,
    ) {
        for key in wanted {
            if self.material_bind_groups.contains_key(key) {
                continue;
            }

            let base_colour = self
                .textures
                .get(&key.base_colour)
                .map_or(&self.white_placeholder_view, |texture| &texture.view);
            let normal = self
                .textures
                .get(&key.normal)
                .map_or(&self.flat_normal_placeholder_view, |texture| &texture.view);
            // The white placeholder again, reused rather than duplicated as a linear one. White is
            // the one value where the two formats agree exactly -- a byte of 255 is 1.0 whether it
            // is read through the sRGB curve or not -- and 1.0 is the identity of the multiply this
            // feeds. Any other colour would need its own linear texture.
            let metallic_roughness = self
                .textures
                .get(&key.metallic_roughness)
                .map_or(&self.white_placeholder_view, |texture| &texture.view);

            let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("amadeo material textures"),
                layout: &self.surface_texture_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(base_colour),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&self.surface_sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::TextureView(normal),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::TextureView(metallic_roughness),
                    },
                ],
            });
            self.material_bind_groups.insert(key.clone(), bind_group);
        }
    }
}

impl RenderBackend for WgpuBackend {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn name(&self) -> &'static str {
        "wgpu"
    }

    fn viewport(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    fn resize(&mut self, width: u32, height: u32) {
        // A minimised window reports zero. Reconfiguring at that size is invalid, so the last valid
        // size is kept until the window comes back.
        if width == 0 || height == 0 {
            return;
        }
        self.width = width;
        self.height = height;
        // Every transient is destination-sized, so none of the pooled textures can serve the new
        // size. Dropping them rather than keeping them means a window dragged across three sizes
        // does not hold three full-screen images it will never use again; the next frame allocates
        // what it needs.
        self.transients.clear();
        self.capture_source = None;
        match &mut self.target {
            Target::Window { surface, config } => {
                config.width = width;
                config.height = height;
                surface.configure(&self.device, config);
            }
            // A texture cannot be resized, so it is replaced. The old one is dropped with the
            // assignment, and nothing outside this backend holds a view onto it.
            Target::Offscreen { texture } => {
                *texture = offscreen_texture(&self.device, width, height, self.format);
            }
        }
    }

    fn has_texture(&self, id: &str) -> bool {
        self.textures.contains_key(id)
    }

    fn has_mesh(&self, id: &str) -> bool {
        self.meshes.contains_key(id)
    }

    fn upload_mesh(&mut self, id: &str, mesh: &crate::MeshData) -> Result<(), RenderError> {
        // `bytemuck` needs the exact GPU layout, and `Vertex` is a plain Rust struct with no
        // guaranteed representation — so the vertices are rebuilt rather than reinterpreted. One
        // copy per mesh, once, which is the right trade for not depending on field order.
        let vertices: Vec<GpuVertex> = mesh
            .vertices
            .iter()
            .map(|vertex| GpuVertex {
                position: vertex.position,
                normal: vertex.normal,
                uv: vertex.uv,
                tangent: vertex.tangent,
            })
            .collect();

        // An empty mesh would produce a zero-sized buffer, which wgpu rejects. Refusing here names
        // the id; letting it through would fail at buffer creation with nothing to attribute it to.
        if vertices.is_empty() || mesh.indices.is_empty() {
            return Err(RenderError::InitFailed {
                backend: "wgpu",
                reason: format!(
                    "mesh `{id}` has no geometry ({} vertices, {} indices); \
                     a mesh asset that tessellated to nothing cannot be uploaded",
                    vertices.len(),
                    mesh.indices.len()
                ),
            });
        }

        let vertex_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(id),
            size: (size_of::<GpuVertex>() * vertices.len()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.queue
            .write_buffer(&vertex_buffer, 0, bytemuck::cast_slice(&vertices));

        let index_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(id),
            size: (size_of::<u32>() * mesh.indices.len()) as u64,
            usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.queue
            .write_buffer(&index_buffer, 0, bytemuck::cast_slice(&mesh.indices));

        // Replacing drops the old buffers and their video memory, which is what makes a reloaded
        // mesh work — the same property `upload_texture` relies on.
        self.meshes.insert(
            id.to_string(),
            GpuMesh {
                vertices: vertex_buffer,
                indices: index_buffer,
                index_count: mesh.indices.len() as u32,
            },
        );
        Ok(())
    }
    /// Drops a mesh's buffers, and with them their video memory.
    ///
    /// `wgpu` frees a buffer when the last handle to it is dropped, so removing the map entry is the
    /// whole of it — there is nothing to release explicitly and nothing that can be released twice.
    fn remove_mesh(&mut self, id: &str) {
        self.meshes.remove(id);
    }

    fn has_environment(&self, id: &str) -> bool {
        self.environments.contains_key(id)
    }

    fn upload_environment(
        &mut self,
        id: &str,
        environment: &crate::EnvironmentMap,
    ) -> Result<(), RenderError> {
        if environment.specular.is_empty() {
            return Err(RenderError::InitFailed {
                backend: "wgpu",
                reason: format!(
                    "environment `{id}` has no specular levels; a prefiltered environment needs at \
                     least one, and an empty one would leave the shader sampling nothing"
                ),
            });
        }

        let (irradiance, irradiance_view) = upload_cubemap(
            &self.device,
            &self.queue,
            id,
            std::slice::from_ref(&environment.irradiance),
        );
        let (specular, specular_view) =
            upload_cubemap(&self.device, &self.queue, id, &environment.specular);

        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(id),
            layout: &self.environment_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&irradiance_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&specular_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.environment_sampler),
                },
            ],
        });

        // Replacing rather than refusing, like every other upload here, so a hot-reloaded sky works
        // for nothing. Dropping the old textures releases their video memory.
        self.environments.insert(
            id.to_string(),
            GpuEnvironment {
                irradiance,
                specular,
                bind_group,
            },
        );
        Ok(())
    }

    fn upload_texture(&mut self, id: &str, texture: &TextureData) -> Result<(), RenderError> {
        let format = match texture.format {
            // `Srgb` here and an sRGB surface format together are what make colours come out right:
            // the art is gamma-encoded, the GPU converts to linear when sampling, blending happens
            // in linear, and the surface converts back on the way out.
            PixelFormat::Rgba8UnormSrgb => wgpu::TextureFormat::Rgba8UnormSrgb,
            // No conversion on sampling, because these bytes are not colour. A normal map read
            // through the sRGB curve comes out with every direction bent, which lights a surface as
            // though its bumps face somewhere they do not (ADR 0047).
            PixelFormat::Rgba8Unorm => wgpu::TextureFormat::Rgba8Unorm,
        };

        let limit = self.device.limits().max_texture_dimension_2d;
        if texture.width > limit || texture.height > limit {
            return Err(RenderError::InitFailed {
                backend: "wgpu",
                reason: format!(
                    "texture `{id}` is {}x{}, and this GPU accepts at most {limit} in either \
                     direction. Re-export it smaller",
                    texture.width, texture.height
                ),
            });
        }

        let size = wgpu::Extent3d {
            width: texture.width,
            height: texture.height,
            depth_or_array_layers: 1,
        };

        // **Mip levels — ADR 0045's first renderer item.** A texture drawn smaller than its pixel
        // count shimmers, because each screen pixel lands on a different unrelated texel as the
        // camera moves. Generated on the CPU rather than with a GPU blit chain: it happens once per
        // texture at upload, it is a dozen lines instead of a pipeline, and doing it in `amadeo-image`
        // is what lets it be tested with no GPU — including the part that is actually subtle, which
        // is averaging in linear light rather than in sRGB.
        let levels = amadeo_image::mip_chain(texture);

        let gpu_texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some(id),
            size,
            mip_level_count: levels.len() as u32,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        for (level, image) in levels.iter().enumerate() {
            self.queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &gpu_texture,
                    mip_level: level as u32,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                &image.pixels,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    // `TextureData` guarantees tightly packed rows with no padding, which is why this
                    // is a plain multiply and not a round-up to 256. That guarantee is checked once,
                    // in `TextureData::new`, rather than here.
                    bytes_per_row: Some(image.width * image.format.bytes_per_pixel()),
                    rows_per_image: Some(image.height),
                },
                wgpu::Extent3d {
                    width: image.width,
                    height: image.height,
                    depth_or_array_layers: 1,
                },
            );
        }

        let view = gpu_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(id),
            layout: &self.texture_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        });

        // **Every cached material bind group naming this id is now stale**, because a bind group
        // holds the view it was built from rather than looking it up again. Dropping them makes the
        // next frame rebuild against the new texture.
        //
        // This is what makes a late arrival visible: a surface drawn while its texture was still a
        // placeholder would otherwise keep the placeholder for the life of the process, which is
        // exactly the defect `placeholders_uploaded` exists to catch one layer up. Discarding the
        // whole entry rather than editing it in place, because a bind group is immutable once made.
        self.material_bind_groups
            .retain(|key, _| !key.ids().contains(&id));

        // Inserting replaces any earlier texture under this id, and dropping the old one releases
        // its video memory. That is what makes a late-arriving asset, and later hot-reload, work.
        self.textures.insert(
            id.to_string(),
            GpuTexture {
                texture: gpu_texture,
                bind_group,
                view,
            },
        );
        Ok(())
    }

    fn render(&mut self, frame: &FrameData) -> Result<(), RenderError> {
        // An offscreen target is always available; a window's is not. `surface_texture` is `Some`
        // only in the windowed case, and is what gets presented at the end.
        let surface_texture = match &self.target {
            Target::Offscreen { .. } => None,
            Target::Window { surface, config } => {
                // wgpu reports several non-fatal reasons a frame cannot be drawn right now. Each one
                // means "skip this frame", not "the renderer is broken": the window is being
                // resized, minimised, or covered. Treating any of them as fatal would kill a game on
                // an ordinary alt-tab.
                match surface.get_current_texture() {
                    // Suboptimal still draws correctly — usually the surface wants reconfiguring,
                    // which the next resize event will do anyway.
                    wgpu::CurrentSurfaceTexture::Success(texture)
                    | wgpu::CurrentSurfaceTexture::Suboptimal(texture) => Some(texture),
                    wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                        // The surface needs rebuilding at its current size before anything can be
                        // drawn.
                        surface.configure(&self.device, config);
                        return Err(RenderError::SurfaceUnavailable {
                            reason: "surface outdated or lost; reconfigured for the next frame"
                                .to_string(),
                        });
                    }
                    wgpu::CurrentSurfaceTexture::Timeout => {
                        return Err(RenderError::SurfaceUnavailable {
                            reason: "timed out acquiring the next frame".to_string(),
                        });
                    }
                    wgpu::CurrentSurfaceTexture::Occluded => {
                        return Err(RenderError::SurfaceUnavailable {
                            reason: "window is occluded".to_string(),
                        });
                    }
                    other => {
                        return Err(RenderError::SurfaceUnavailable {
                            reason: format!("unexpected surface state: {other:?}"),
                        });
                    }
                }
            }
        };

        let destination_view = match (&surface_texture, &self.target) {
            (Some(texture), _) => texture
                .texture
                .create_view(&wgpu::TextureViewDescriptor::default()),
            (None, Target::Offscreen { texture }) => {
                texture.create_view(&wgpu::TextureViewDescriptor::default())
            }
            // Unreachable: a window target always produced a texture above or returned.
            (None, Target::Window { .. }) => {
                return Err(RenderError::SurfaceUnavailable {
                    reason: "no surface texture was acquired".to_string(),
                });
            }
        };

        // The plan for this frame: which passes exist, what each touches, and therefore what order
        // they run in. Built and checked before anything is recorded, so an inconsistent graph is a
        // clean error rather than half a frame.
        let graph = graph::frame_graph(frame, self.width, self.height);
        let plan = graph.compile().map_err(|error| RenderError::GraphInvalid {
            reason: error.to_string(),
        })?;

        // Everything is packed for the whole frame first, then drawn view by view. Packing per view
        // inside the encoder would not work: a queue write lands before the single submit at the
        // end, so the second view's write would overwrite the first's rather than following it.
        //
        // Each view's data is concatenated and its own ranges recorded. Today every view holds the
        // same drawables, so this uploads them once per camera — deliberately, because per-camera
        // culling is coming and a shortcut that assumed the views agreed would have to be unpicked.
        // 3D data, packed for the whole frame alongside the 2D data below and for the same reason:
        // a queue write lands before the single submit, so writing per view would overwrite rather
        // than follow. `mesh_draws` records each view's slice and which mesh each run uses.
        let mut mesh_views: Vec<u8> = vec![0; self.mesh_view_stride as usize * frame.views.len()];
        // One slot per (view, cascade), whether or not the view casts shadows — sized flat so the
        // dynamic offset is plain arithmetic rather than a lookup. Unused slots stay zeroed and are
        // never bound, because the graph declares no pass for them.
        let mut shadow_views: Vec<u8> =
            vec![
                0;
                self.shadow_view_stride as usize * frame.views.len() * crate::MAX_SHADOW_LAYERS
            ];
        let mut mesh_instances: Vec<GpuMeshInstance> = Vec::new();
        let mut mesh_draws: Vec<Vec<(&str, MaterialTextures, std::ops::Range<u32>)>> =
            Vec::with_capacity(frame.views.len());
        // The shadow pass's own ranges, over the same instance buffer. Kept in step with
        // `mesh_draws`: every path that pushes to one pushes to the other, so one `view_index`
        // addresses both and they cannot drift out of alignment.
        let mut shadow_draws: Vec<Vec<(&str, MaterialTextures, std::ops::Range<u32>)>> =
            Vec::with_capacity(frame.views.len());

        for (index, view) in frame.views.iter().enumerate() {
            let (px_width, px_height) = self.viewport_pixels(view);
            let aspect = px_width / px_height.max(1.0);

            // Only a perspective camera feeds this pass. An orthographic one carries no meshes at
            // all (the collection pass sees to that), so this is belt and braces — but the
            // projection below would be meaningless for one, so it is worth being explicit.
            let (fov, near, far) = match view.camera.projection {
                crate::Projection::Perspective { fov, near, far } => (fov, near, far),
                crate::Projection::Orthographic { .. } => {
                    mesh_draws.push(Vec::new());
                    shadow_draws.push(Vec::new());
                    continue;
                }
            };

            // World to clip: undo the camera's placement, then project. `inverse_rigid` returns
            // `None` only for a collapsed transform, which is a camera that flattened the world —
            // it draws nothing rather than filling the screen with NaN.
            let Some(camera_view) = view.eye_matrix.inverse_rigid() else {
                mesh_draws.push(Vec::new());
                shadow_draws.push(Vec::new());
                continue;
            };
            let projection = amadeo_transform::Mat4::perspective(fov, aspect, near, far);
            let view_projection = projection.mul(&camera_view);

            // How far the view opens out at one unit ahead — the tangent of half the vertical field
            // of view. Multiplying the camera's right and up axes by this is what makes a screen
            // position into a direction, and it is why the sky basis needs no trigonometry.
            let half_extent = (fov.to_radians() * 0.5).tan();

            // The first light, or none. Several directional lights need either a loop in the shader
            // or a pass each, and picking between those before anything wants two is guessing.
            let light = view.lights.first().copied().unwrap_or(crate::LightData {
                direction: [0.0, -1.0, 0.0],
                colour: [0.0, 0.0, 0.0],
                shadow: None,
            });

            // The shadow the collection pass fitted for this view, if any (ADR 0038). Taken from
            // whichever light carries one rather than only the first, so a scene whose first light
            // is a fill light and whose second is the sun still gets its shadows.
            let shadow = view.lights.iter().find_map(|light| light.shadow);

            // The cascades, unpacked into the flat arrays the uniform wants. An unused slot keeps
            // the identity matrix and a `far` of zero, which the shader's selection never picks
            // because it only looks at the first `count`.
            let mut light_view_projection =
                [amadeo_transform::Mat4::IDENTITY.columns; crate::CASCADE_COUNT];
            let mut cascade_far = [0.0f32; crate::CASCADE_COUNT];
            let mut cascade_bias = [0.0f32; crate::CASCADE_COUNT];
            if let Some(shadow) = shadow {
                for index in 0..shadow.count {
                    let cascade = shadow.cascades[index];
                    light_view_projection[index] = cascade.view_projection.columns;
                    cascade_far[index] = cascade.far;
                    cascade_bias[index] = cascade.bias;
                }
            }

            // The punctual lights, flattened into the uniform's fixed array. The collection pass has
            // already sorted them nearest-first and truncated to the cap, so this takes them as they
            // come — `min` is belt and braces against a backend being handed more than it can hold.
            let mut punctual: [GpuPunctualLight; crate::MAX_PUNCTUAL_LIGHTS] =
                bytemuck::Zeroable::zeroed();
            let punctual_count = view.punctual.len().min(crate::MAX_PUNCTUAL_LIGHTS);
            for (slot, light) in punctual
                .iter_mut()
                .zip(view.punctual.iter())
                .take(punctual_count)
            {
                *slot = GpuPunctualLight {
                    position_range: [
                        light.position[0],
                        light.position[1],
                        light.position[2],
                        light.range,
                    ],
                    direction_outer: [
                        light.direction[0],
                        light.direction[1],
                        light.direction[2],
                        light.cone_outer_cos,
                    ],
                    colour_inner: [
                        light.colour[0],
                        light.colour[1],
                        light.colour[2],
                        light.cone_inner_cos,
                    ],
                    // `-1` for a light that casts nothing, which is what the shader tests. A float
                    // rather than an integer because everything else here is one and a lone `i32`
                    // would take its own sixteen-byte slot anyway.
                    shadow: light.shadow.map_or([-1.0, 0.0, 0.0, 0.0], |spot| {
                        [spot.layer as f32, spot.bias, 0.0, 0.0]
                    }),
                };
            }

            // Each shadow-casting spot's matrix, at its layer minus the cascade count — the same
            // arithmetic the shader does, so the two index the same slot.
            let mut spot_shadow =
                [amadeo_transform::Mat4::IDENTITY.columns; crate::MAX_SHADOW_SPOTS];
            let cascades = shadow.map_or(0, |shadow| shadow.count);
            for light in &view.punctual {
                let Some(spot) = light.shadow else {
                    continue;
                };
                if let Some(slot) = spot_shadow.get_mut(spot.layer as usize - cascades) {
                    *slot = spot.view_projection.columns;
                }
            }

            let uniform = GpuMeshView {
                view_projection: view_projection.columns,
                light_view_projection,
                light_direction: [
                    light.direction[0],
                    light.direction[1],
                    light.direction[2],
                    0.0,
                ],
                light_colour: [light.colour[0], light.colour[1], light.colour[2], 0.0],
                shadow_params: [
                    // **The texel size of the atlas, not of the cascades.** Both the directional
                    // path and ADR 0058's spot path use this to step their PCF taps, and they
                    // sample *one shared array* — so deriving it from the directional light alone
                    // left it at zero in a scene lit only by a torch, which collapses a spot's 3×3
                    // filter onto a single tap. `View::shadow_atlas` is the one place that decides
                    // the array's size, so it is the one place this can honestly come from.
                    view.shadow_atlas()
                        .map_or(0.0, |(resolution, _)| 1.0 / resolution.max(1) as f32),
                    // The flag the shader tests to tell a real *cascade* map from the placeholder.
                    // Directional-only, deliberately: a spot decides for itself from its own
                    // `shadow.x`, and folding the two together is what made the two consumers of
                    // this struct share a switch that only one of them owns.
                    if shadow.is_some() { 1.0 } else { 0.0 },
                    shadow.map_or(0.0, |shadow| shadow.count as f32),
                    0.0,
                ],
                cascade_far,
                cascade_bias,
                // Straight off the camera's resolved look. Density zero is off, and the shader
                // returns early on it, so a scene with no fog renders byte-identically.
                fog_colour: [
                    view.environment.fog.colour[0],
                    view.environment.fog.colour[1],
                    view.environment.fog.colour[2],
                    view.environment.fog.density,
                ],
                fog_params: [view.environment.fog.start, 0.0, 0.0, 0.0],
                // The camera's world position is the translation column of its world transform —
                // column 3. Taken from `eye_matrix` rather than from `View::eye`, which is the 2D
                // path's two numbers and has no height in it.
                eye: [
                    view.eye_matrix.columns[3][0],
                    view.eye_matrix.columns[3][1],
                    view.eye_matrix.columns[3][2],
                    0.0,
                ],
                sky_right: scaled_axis(&view.eye_matrix, 0, half_extent * aspect),
                sky_up: scaled_axis(&view.eye_matrix, 1, half_extent),
                // **Negated**, because a camera looks along its own *negative* Z — the same
                // convention `Camera` and `DirectionalLight` both use (ADR 0018). Taking column two
                // as-is points the sky out of the back of the camera, which draws a perfectly
                // plausible sky facing exactly the wrong way.
                sky_forward: scaled_axis(&view.eye_matrix, 2, -1.0),
                punctual_count: [punctual_count as f32, 0.0, 0.0, 0.0],
                punctual,
                spot_shadow,
            };
            let at = index * self.mesh_view_stride as usize;
            mesh_views[at..at + size_of::<GpuMeshView>()]
                .copy_from_slice(bytemuck::bytes_of(&uniform));

            // The shadow passes' matrices, filled from the same `shadow` this uniform just used —
            // which is what keeps the two buffers agreeing without them being one buffer.
            if let Some(shadow) = shadow {
                for cascade in 0..shadow.count {
                    let slot = index * crate::MAX_SHADOW_LAYERS + cascade;
                    let at = slot * self.shadow_view_stride as usize;
                    let uniform = GpuShadowView {
                        view_projection: shadow.cascades[cascade].view_projection.columns,
                    };
                    shadow_views[at..at + size_of::<GpuShadowView>()]
                        .copy_from_slice(bytemuck::bytes_of(&uniform));
                }
            }

            // And each shadow-casting spot light's, into the layers after the cascades (ADR 0058).
            // The layer was decided at collection, so this writes where the light says rather than
            // recomputing the arithmetic and risking the two disagreeing.
            for light in &view.punctual {
                let Some(spot) = light.shadow else {
                    continue;
                };
                let slot = index * crate::MAX_SHADOW_LAYERS + spot.layer as usize;
                let at = slot * self.shadow_view_stride as usize;
                let uniform = GpuShadowView {
                    view_projection: spot.view_projection.columns,
                };
                shadow_views[at..at + size_of::<GpuShadowView>()]
                    .copy_from_slice(bytemuck::bytes_of(&uniform));
            }

            // Consecutive instances sharing a mesh **and a texture** become one draw call. The
            // collection pass sorts by `SortOrder`, so this groups within an order rather than
            // across it — the same rule ADR 0023 gave sprites, and for the same reason: layering
            // must not be violated to save a draw call.
            //
            // The texture joins the key because a bind group is set per draw, so two instances of
            // one mesh wearing different textures cannot share a call. Two instances of one mesh
            // wearing the *same* texture still do, which is the common case by a wide margin.
            // Two runs over the same buffer: what the colour pass draws, then what the shadow pass
            // draws. They overlap, and the overlap is written twice — a matrix and two colours per
            // repeat, which is cheaper than the second buffer and the second resize path that
            // avoiding it would cost. See `View::shadow_casters` for why they are different lists.
            let mut draws: Vec<(&str, MaterialTextures, std::ops::Range<u32>)> = Vec::new();
            for instance in &view.meshes {
                let first = mesh_instances.len() as u32;
                mesh_instances.push(GpuMeshInstance {
                    model: instance.model.columns,
                    base_colour: instance.material.base_colour,
                    emissive: [
                        instance.material.emissive[0],
                        instance.material.emissive[1],
                        instance.material.emissive[2],
                        0.0,
                    ],
                    surface: [
                        instance.material.metallic,
                        instance.material.roughness,
                        instance.material.normal_strength,
                        0.0,
                    ],
                });
                let last = mesh_instances.len() as u32;

                let textures = MaterialTextures::of(&instance.material);
                match draws.last_mut() {
                    Some((mesh, bound, range))
                        if *mesh == instance.mesh.as_str() && *bound == textures =>
                    {
                        range.end = last;
                    }
                    _ => draws.push((instance.mesh.as_str(), textures, first..last)),
                }
            }
            mesh_draws.push(draws);

            let mut casters: Vec<(&str, MaterialTextures, std::ops::Range<u32>)> = Vec::new();
            for instance in &view.shadow_casters {
                let first = mesh_instances.len() as u32;
                mesh_instances.push(GpuMeshInstance {
                    model: instance.model.columns,
                    base_colour: instance.material.base_colour,
                    emissive: [
                        instance.material.emissive[0],
                        instance.material.emissive[1],
                        instance.material.emissive[2],
                        0.0,
                    ],
                    surface: [
                        instance.material.metallic,
                        instance.material.roughness,
                        instance.material.normal_strength,
                        0.0,
                    ],
                });
                let last = mesh_instances.len() as u32;

                let textures = MaterialTextures::of(&instance.material);
                match casters.last_mut() {
                    Some((mesh, bound, range))
                        if *mesh == instance.mesh.as_str() && *bound == textures =>
                    {
                        range.end = last;
                    }
                    _ => casters.push((instance.mesh.as_str(), textures, first..last)),
                }
            }
            shadow_draws.push(casters);
        }

        // Before any pass opens — see `ensure_material_bind_groups`. Both lists, because the shadow
        // pass draws its own set of instances and a caster the camera cannot see still needs a
        // binding to draw with.
        let wanted: Vec<MaterialTextures> = mesh_draws
            .iter()
            .chain(shadow_draws.iter())
            .flatten()
            .map(|(_, textures, _)| textures.clone())
            .collect();
        self.ensure_material_bind_groups(wanted.iter());

        let mut cameras: Vec<u8> = vec![0; self.camera_stride as usize * frame.views.len()];
        let mut instances: Vec<GpuInstance> = Vec::with_capacity(frame.quad_count());
        let mut sprites: Vec<GpuSprite> = Vec::with_capacity(frame.sprite_count());
        let mut per_view: Vec<ViewDraws> = Vec::with_capacity(frame.views.len());

        for (index, view) in frame.views.iter().enumerate() {
            // The world half-size this camera covers. Width follows the target rectangle's aspect
            // ratio, so a half-width viewport shows half the world rather than a squashed whole.
            let (px_width, px_height) = self.viewport_pixels(view);
            let aspect = px_width / px_height.max(1.0);
            let half_height = view.camera.projection.height().unwrap_or(2.0) / 2.0;
            let camera = GpuCamera {
                center: view.eye,
                half_extents: [half_height * aspect, half_height],
            };
            let at = index * self.camera_stride as usize;
            cameras[at..at + size_of::<GpuCamera>()].copy_from_slice(bytemuck::bytes_of(&camera));

            let quads_from = instances.len() as u32;
            instances.extend(view.quads.iter().map(|quad| GpuInstance {
                center_size: [quad.center[0], quad.center[1], quad.size[0], quad.size[1]],
                rotation: [quad.rotation, 0.0, 0.0, 0.0],
                color: quad.color,
            }));

            // Every batch's sprites go into **one** buffer, laid out back to back in batch order,
            // and each batch then draws its own slice of it. So the number of buffer writes per
            // frame is one regardless of how many batches there are -- the batches only decide how
            // many times the texture bind group changes, which is the cost ADR 0023 is about.
            //
            // `first` is recorded alongside each batch here rather than recomputed in the pass,
            // because a running offset maintained in two places is exactly what drifts.
            let mut draws: Vec<(&str, std::ops::Range<u32>)> =
                Vec::with_capacity(view.batches.len());
            for batch in &view.batches {
                let first = sprites.len() as u32;
                sprites.extend(batch.instances.iter().map(|sprite| GpuSprite {
                    center_axis_x: [
                        sprite.center[0],
                        sprite.center[1],
                        sprite.axes[0][0],
                        sprite.axes[0][1],
                    ],
                    axis_y: [sprite.axes[1][0], sprite.axes[1][1], 0.0, 0.0],
                    color: sprite.color,
                    region: sprite.region,
                }));
                draws.push((batch.texture.as_str(), first..sprites.len() as u32));
            }

            per_view.push(ViewDraws {
                quads: quads_from..instances.len() as u32,
                draws,
            });
        }

        self.ensure_camera_capacity(frame.views.len());
        if !cameras.is_empty() {
            self.queue.write_buffer(&self.camera_buffer, 0, &cameras);
        }

        self.ensure_instance_capacity(instances.len());
        if !instances.is_empty() {
            self.queue
                .write_buffer(&self.instance_buffer, 0, bytemuck::cast_slice(&instances));
        }

        self.ensure_sprite_capacity(sprites.len());
        if !sprites.is_empty() {
            self.queue
                .write_buffer(&self.sprite_buffer, 0, bytemuck::cast_slice(&sprites));
        }

        self.ensure_mesh_view_capacity(frame.views.len());
        if !mesh_views.is_empty() {
            self.queue
                .write_buffer(&self.mesh_view_buffer, 0, &mesh_views);
        }

        self.ensure_shadow_view_capacity(frame.views.len() * crate::MAX_SHADOW_LAYERS);
        if !shadow_views.is_empty() {
            self.queue
                .write_buffer(&self.shadow_view_buffer, 0, &shadow_views);
        }

        self.ensure_mesh_instance_capacity(mesh_instances.len());
        if !mesh_instances.is_empty() {
            self.queue.write_buffer(
                &self.mesh_instance_buffer,
                0,
                bytemuck::cast_slice(&mesh_instances),
            );
        }

        // The multisampled buffer view passes draw into (ADR 0051), allocated before the pool is
        // borrowed below. It has to happen here rather than lazily inside the pass loop: creating it
        // needs `&mut self`, and by then the loop is holding a transient's view immutably.
        self.ensure_multisampled_target();

        // Every transient the plan needs, backed by a pooled texture. After this the pool is not
        // touched again this frame, so the loop below can borrow it immutably.
        let assigned = self.assign_transients(&graph, &plan);

        // The look the post pass will apply. **One environment per frame, taken from the camera
        // that draws first** — the same "which camera when there are several" rule ADR 0031 gave
        // `render.describe`. Per-camera post needs per-camera targets, which arrive with
        // `Camera::target`; until then a HUD camera cannot have its own grade. Recorded as Q23.
        let look = frame.look();
        self.queue.write_buffer(
            &self.post_buffer,
            0,
            bytemuck::bytes_of(&GpuPost::from_environment(&look)),
        );

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("amadeo frame encoder"),
            });

        let clear = wgpu::LoadOp::Clear(wgpu::Color {
            r: f64::from(frame.clear_color[0]),
            g: f64::from(frame.clear_color[1]),
            b: f64::from(frame.clear_color[2]),
            a: f64::from(frame.clear_color[3]),
        });

        // Which pass slot each timed pass took, and what it was called. Recorded in execution order
        // rather than declaration order, because that is the order the timestamps come back in and
        // the order a reader wants to see a frame broken down.
        let mut timed: Vec<(usize, String)> = Vec::new();

        for &pass_index in plan.order() {
            let declared = &graph.passes()[pass_index];

            // A shadow pass is the one pass with **no colour attachment**: it writes depth and
            // nothing else, so what it declares as its target is bound where a depth buffer goes
            // rather than where an image does (ADR 0038). Handled before the colour path below
            // because that path would otherwise attach a depth texture as a colour target, which is
            // a validation error a long way from its cause.
            // The cascade is read inside `run_shadow_pass`, from the same declaration; only the view
            // is needed out here, to pick this view's list of casters.
            if let PassKind::Shadow {
                view: view_index, ..
            } = declared.kind
            {
                // Timed like any other pass. It used to be skipped simply because it takes a
                // different code path, and the omission showed: the per-pass breakdown summed to
                // 27 µs of a 37 µs frame with nothing accounting for the difference. A profiler
                // that silently loses a pass is worse than one that reports only a total.
                let slot = timed.len();
                let writes = self.timing.as_ref().and_then(|timing| timing.writes(slot));
                if writes.is_some() {
                    timed.push((slot, declared.label.clone()));
                }
                self.run_shadow_pass(
                    &mut encoder,
                    declared,
                    &assigned,
                    shadow_draws.get(view_index),
                    writes,
                );
                continue;
            }

            // Every other pass this backend knows how to run writes exactly one image. A pass
            // writing none has nothing to attach and is skipped rather than being a special case
            // here — the graph would have to grow a compute pass before that is reachable.
            let Some(target_name) = declared.writes.first() else {
                continue;
            };
            let target_view = if target_name == DESTINATION {
                &destination_view
            } else {
                let Some(&pooled) = assigned.get(target_name) else {
                    continue;
                };
                &self.transients[pooled].view
            };

            // Whether this pass starts from the clear colour or from what the previous pass left.
            // Only the *first* camera clears, which is what makes a HUD camera compose over a world
            // camera rather than erase it.
            let load = match declared.kind {
                PassKind::View { clears: false, .. } => wgpu::LoadOp::Load,
                PassKind::View { .. } | PassKind::Clear => clear,
                // Every full-screen pass overwrites every pixel of its target, so what was there is
                // irrelevant — but a load of undefined contents is worse than a clear on some
                // backends, and a clear of something about to be fully written costs nothing.
                PassKind::Post | PassKind::Present | PassKind::Bloom(_) => clear,
                // Returned above, before any colour attachment is chosen.
                PassKind::Shadow { .. } => continue,
            };

            // Only a 3D view pass declares one (see `Pass::depth`). Cleared to 1.0, the far plane,
            // because the projection puts near at 0 and far at 1 — the WebGPU convention rather than
            // OpenGL's, and clearing to the wrong end means everything fails the depth test.
            let depth_view = declared
                .depth
                .as_ref()
                .and_then(|name| assigned.get(name))
                .map(|pooled| &self.transients[*pooled].view);
            let depth_attachment = depth_view.map(|view| wgpu::RenderPassDepthStencilAttachment {
                view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            });

            // Timed through the pass descriptor rather than by writing into the encoder, because
            // `TIMESTAMP_QUERY` covers this form on every adapter that has it at all — the encoder
            // form needs a second feature that fewer machines advertise.
            let slot = timed.len();
            let timestamp_writes = self.timing.as_ref().and_then(|timing| timing.writes(slot));
            if timestamp_writes.is_some() {
                timed.push((slot, declared.label.clone()));
            }

            // **Anti-aliasing, and it is entirely contained in these four lines** (ADR 0051).
            //
            // A view pass draws into a multisampled buffer and names its ordinary target as the
            // `resolve_target`, so the hardware averages the samples down at the end of the pass and
            // everything downstream reads a plain single-sampled image. Nothing else in the frame —
            // post, present, capture — needs to know it happened.
            //
            // The multisampled buffer **persists across view passes** rather than being resolved and
            // discarded, which is what keeps a second camera composing over the first: view 1 loads
            // what view 0 left in it. Resolving on every pass rather than only the last costs a
            // little and means no pass has to know whether it is the final one.
            let multisampled = match declared.kind {
                PassKind::View { .. } => self.multisampled_target.as_ref().map(|(_, view)| view),
                _ => None,
            };
            let (attachment_view, resolve_target) = match multisampled {
                Some(view) => (view, Some(target_view)),
                None => (target_view, None),
            };

            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some(declared.label.as_str()),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: attachment_view,
                    depth_slice: None,
                    resolve_target,
                    ops: wgpu::Operations {
                        load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: depth_attachment,
                multiview_mask: None,
                timestamp_writes,
                occlusion_query_set: None,
            });

            match declared.kind {
                // Filling the target with the clear colour is the whole job, and beginning the pass
                // above has already done it.
                PassKind::Clear => {}

                PassKind::View { index, .. } => {
                    let (Some(view_data), Some(draws)) =
                        (frame.views.get(index), per_view.get(index))
                    else {
                        continue;
                    };

                    let (px_x, px_y) = self.viewport_origin(view_data);
                    let (px_width, px_height) = self.viewport_pixels(view_data);
                    // A zero-sized viewport is a validation error rather than a no-op, so a camera
                    // with a degenerate rectangle is skipped instead of taking the frame down.
                    if px_width < 1.0 || px_height < 1.0 {
                        continue;
                    }
                    pass.set_viewport(px_x, px_y, px_width, px_height, 0.0, 1.0);

                    // 3D first: meshes write depth, and everything 2D in this engine is drawn
                    // without depth at all. Today no camera carries both (a projection selects one
                    // or the other), so the order is a statement of intent rather than a behaviour
                    // anything depends on.
                    if let Some(draws) = mesh_draws.get(index)
                        && !draws.is_empty()
                    {
                        pass.set_pipeline(&self.mesh_pipeline);
                        pass.set_bind_group(
                            0,
                            &self.mesh_view_bind_group,
                            &[index as u32 * self.mesh_view_stride as u32],
                        );
                        // The shadow map this view reads, or the 1×1 placeholder. Something must be
                        // bound either way — the pipeline declares the binding, so leaving it empty
                        // is a validation error rather than a shader that skips the lookup. What
                        // makes the placeholder harmless is `shadow_params.z`, which the shader
                        // tests before sampling at all.
                        let shadow_binding = declared
                            .reads
                            .iter()
                            .find_map(|name| assigned.get(name))
                            .and_then(|pooled| self.transients[*pooled].bind_group.as_ref())
                            .unwrap_or(&self.shadow_placeholder_bind_group);
                        pass.set_bind_group(1, shadow_binding, &[]);

                        // The environment this camera is lit by, or the neutral one. Set here,
                        // once per view, rather than per draw — every surface a camera sees is
                        // surrounded by the same sky.
                        let environment = self
                            .environments
                            .get(view_data.environment.sky.as_str())
                            .map_or(&self.neutral_environment_bind_group, |held| {
                                &held.bind_group
                            });
                        pass.set_bind_group(3, environment, &[]);
                        pass.set_vertex_buffer(1, self.mesh_instance_buffer.slice(..));

                        for (mesh_id, textures, range) in draws {
                            // A mesh that was never uploaded is skipped rather than drawn with
                            // whatever buffer happened to be bound, which would render one shape
                            // wearing another's geometry — silently, and very confusingly.
                            let Some(mesh) = self.meshes.get(*mesh_id) else {
                                continue;
                            };
                            // Built above, so this is a lookup that cannot miss. Skipping rather
                            // than unwrapping if it somehow does: a dropped draw is a missing
                            // object, where a panic in a render pass takes the whole game down.
                            let Some(bound) = self.material_bind_groups.get(textures) else {
                                continue;
                            };
                            pass.set_bind_group(2, bound, &[]);
                            pass.set_vertex_buffer(0, mesh.vertices.slice(..));
                            pass.set_index_buffer(
                                mesh.indices.slice(..),
                                wgpu::IndexFormat::Uint32,
                            );
                            pass.draw_indexed(0..mesh.index_count, 0, range.clone());
                        }
                    }

                    // The sky, behind everything drawn above (ADR 0049).
                    //
                    // **Only when this camera actually names one**, and that condition is doing real
                    // work: a camera with no sky would otherwise have the neutral cube painted over
                    // its clear colour, turning every existing 2D game's background into flat dark
                    // grey. Naming no sky means "do not draw one", not "draw a default one".
                    //
                    // Outside the mesh block on purpose. A camera looking at an empty world still
                    // has a sky to show, and nesting this inside `if !draws.is_empty()` would make
                    // the horizon vanish the moment nothing was in front of it.
                    if let Some(sky) = self.environments.get(view_data.environment.sky.as_str()) {
                        pass.set_pipeline(&self.sky_pipeline);
                        pass.set_bind_group(
                            0,
                            &self.mesh_view_bind_group,
                            &[index as u32 * self.mesh_view_stride as u32],
                        );
                        pass.set_bind_group(1, &sky.bind_group, &[]);
                        // Three vertices, no buffer — see `sky.wgsl`.
                        pass.draw(0..3, 0..1);
                    }

                    let camera_offset = index as u32 * self.camera_stride as u32;

                    if !draws.quads.is_empty() {
                        pass.set_pipeline(&self.pipeline);
                        pass.set_bind_group(0, &self.camera_bind_group, &[camera_offset]);
                        pass.set_vertex_buffer(0, self.instance_buffer.slice(..));
                        // Four vertices (one strip), once per instance. One draw call for this
                        // view's quads.
                        pass.draw(0..4, draws.quads.clone());
                    }

                    // Sprites draw after quads, so a textured sprite sits over an untextured
                    // rectangle at the same position. `SortOrder` governs order *within* each of
                    // the two, and the two do not interleave -- worth knowing, and the reason a
                    // background drawn as a `Quad` behind sprites works with no sort order at all.
                    if !draws.draws.is_empty() {
                        pass.set_pipeline(&self.sprite_pipeline);
                        pass.set_bind_group(0, &self.camera_bind_group, &[camera_offset]);
                        pass.set_vertex_buffer(0, self.sprite_buffer.slice(..));

                        for (texture_id, range) in &draws.draws {
                            // A batch naming a texture that was never uploaded is skipped rather
                            // than drawn untextured. It should not happen -- `upload_frame_textures`
                            // uploads at least a placeholder for every batch before this runs -- and
                            // binding the previous batch's texture instead would draw the wrong
                            // picture silently, which is worse than a gap.
                            let Some(texture) = self.textures.get(*texture_id) else {
                                continue;
                            };
                            pass.set_bind_group(1, &texture.bind_group, &[]);
                            // The one state change per batch the whole batcher exists to minimise.
                            pass.draw(0..4, range.clone());
                        }
                    }
                }

                PassKind::Post | PassKind::Present | PassKind::Bloom(_) => {
                    let Some(source) = declared
                        .reads
                        .first()
                        .and_then(|name| assigned.get(name))
                        .copied()
                    else {
                        continue;
                    };

                    // The two full-screen passes differ only in which shader runs and whether the
                    // environment is bound — same triangle, same source binding, same draw.
                    // A full-screen pass reads a colour transient, which always has a bind group —
                    // only depth lacks one. Skipping rather than unwrapping keeps that a fact this
                    // code checks rather than one it assumes.
                    let Some(binding) = self.transients[source].bind_group.as_ref() else {
                        continue;
                    };

                    match declared.kind {
                        PassKind::Post => {
                            pass.set_pipeline(&self.post_pipeline);
                            pass.set_bind_group(1, &self.post_bind_group, &[]);
                            // The glow, at group 2. The graph puts it second in `reads` when bloom
                            // ran; otherwise the 1×1 black placeholder, which adds nothing.
                            let glow = declared
                                .reads
                                .get(1)
                                .and_then(|name| assigned.get(name))
                                .and_then(|pooled| self.transients[*pooled].bind_group.as_ref())
                                .unwrap_or(&self.bloom_placeholder_bind_group);
                            pass.set_bind_group(2, glow, &[]);
                        }
                        PassKind::Bloom(step) => {
                            pass.set_pipeline(match step {
                                graph::BloomStep::Bright => &self.bloom_bright_pipeline,
                                graph::BloomStep::BlurX => &self.bloom_blur_x_pipeline,
                                graph::BloomStep::BlurY => &self.bloom_blur_y_pipeline,
                            });
                            // Bloom reads the same look the post pass does — the threshold for the
                            // bright pass, and the exposure it has to apply first.
                            pass.set_bind_group(1, &self.post_bind_group, &[]);
                        }
                        _ => pass.set_pipeline(&self.present_pipeline),
                    }
                    pass.set_bind_group(0, binding, &[]);
                    // Three vertices, one instance, no buffers — see `present.wgsl`.
                    pass.draw(0..3, 0..1);
                }

                // Handled before this match, where it can be given a depth attachment and no colour
                // one. Unreachable rather than ignored, so adding a pass kind here still has to
                // think about it.
                PassKind::Shadow { .. } => {}
            }
        }

        // What `capture` should read back: whatever the present pass was about to put on screen.
        // Derived from the plan rather than named here, so inserting post-processing before the
        // present pass moves it automatically instead of quietly capturing the pre-effect picture.
        self.capture_source = plan
            .destination_source()
            .and_then(|name| assigned.get(name))
            .copied();

        // Copy the tick counts out of the query set while the encoder is still open. A
        // `QUERY_RESOLVE` buffer cannot also be `MAP_READ`, so this is two hops: resolve into one
        // buffer, copy that into a mappable one.
        let timing_slots = timed.len();
        if timing_slots > 0
            && let Some(timing) = self.timing.as_ref()
            && timing.enabled
        {
            let count = (timing_slots * 2) as u32;
            encoder.resolve_query_set(&timing.queries, 0..count, &timing.resolved, 0);
            encoder.copy_buffer_to_buffer(
                &timing.resolved,
                0,
                &timing.readback,
                0,
                u64::from(count) * 8,
            );
        }

        self.queue.submit(std::iter::once(encoder.finish()));

        if timing_slots > 0 {
            self.last_timing = self.read_timings(&timed);
        }
        // Presentation moved onto the queue in wgpu 30; it used to be a method on the texture.
        // An offscreen frame is not presented — it stays in its texture, which is the point.
        if let Some(texture) = surface_texture {
            self.queue.present(texture);
        }
        Ok(())
    }

    fn capture(&mut self) -> Result<TextureData, RenderError> {
        // # What gets read, and why the two backends differ by exactly one pass
        //
        // **Offscreen: the destination itself**, after the present pass has written it. So a
        // captured frame is evidence about the *whole* pipeline, the final full-screen copy
        // included — and since that is the path CI and agent mode both use (ADR 0016 launches a
        // game with no window), nothing in the renderer escapes being tested.
        //
        // **Windowed: the transient the present pass was about to copy onto the window.** A
        // window's image is not created with `COPY_SRC` and wgpu does not let you ask for one, so
        // this is as far as a readback can reach. It is everything except that last copy — which
        // is why a windowed run can capture at all now, where before it could only refuse.
        let texture = match &self.target {
            Target::Offscreen { texture } => texture,
            Target::Window { .. } => {
                let Some(source) = self.capture_source else {
                    return Err(RenderError::CaptureUnsupported {
                        backend: "wgpu (windowed)",
                        reason: "nothing has been rendered yet, so there is no finished frame to \
                                 read. Call `render` first; `render.describe` answers what *should* \
                                 be on screen without drawing anything"
                            .to_string(),
                    });
                };
                &self.transients[source].texture
            }
        };

        // A copy's row stride must be a multiple of 256 bytes, so the buffer is *wider* than the
        // image whenever the width is not. The padding is stripped again below; forgetting that is
        // the classic readback bug, and it looks like a picture sheared diagonally.
        let bytes_per_pixel = PixelFormat::Rgba8UnormSrgb.bytes_per_pixel();
        let unpadded = self.width * bytes_per_pixel;
        let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let padded = unpadded.div_ceil(align) * align;

        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("amadeo capture readback"),
            size: u64::from(padded) * u64::from(self.height),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("amadeo capture encoder"),
            });
        encoder.copy_texture_to_buffer(
            texture.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded),
                    rows_per_image: Some(self.height),
                },
            },
            wgpu::Extent3d {
                width: self.width,
                height: self.height,
                depth_or_array_layers: 1,
            },
        );
        self.queue.submit(std::iter::once(encoder.finish()));

        // Mapping is asynchronous, and the callback only runs while the device is polled. `Wait`
        // blocks until the queue is idle, which is exactly what is wanted here: capture is an
        // introspection call, never a per-frame one.
        let slice = buffer.slice(..);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        self.device
            .poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: None,
            })
            .map_err(|error| RenderError::CaptureUnsupported {
                backend: "wgpu",
                reason: format!("the device did not finish the capture copy: {error}"),
            })?;

        let mapped = slice
            .get_mapped_range()
            .map_err(|error| RenderError::CaptureUnsupported {
                backend: "wgpu",
                reason: format!("the readback buffer could not be mapped: {error}"),
            })?;
        let mut pixels = Vec::with_capacity((unpadded * self.height) as usize);
        for row in 0..self.height {
            let start = (row * padded) as usize;
            pixels.extend_from_slice(&mapped[start..start + unpadded as usize]);
        }
        drop(mapped);
        buffer.unmap();

        Ok(TextureData {
            width: self.width,
            height: self.height,
            format: PixelFormat::Rgba8UnormSrgb,
            pixels,
        })
    }
}

/// GPU timing — **exit gate 4**. Inherent rather than part of [`RenderBackend`], because a null
/// backend has no GPU to time and a future backend may measure differently; a measurement harness
/// reaches for the concrete backend it installed.
impl WgpuBackend {
    /// Reads back what the frame just submitted cost, in GPU time.
    ///
    /// # This blocks, and that is why it is off by default
    ///
    /// Getting the numbers means waiting for the GPU to finish the frame — a full pipeline stall,
    /// which is precisely what a real frame loop exists to avoid. A profiler may stall; a game may
    /// not. So `set_gpu_timing(true)` is something a measurement harness does and nothing else, and
    /// the alternative — reading last frame's numbers this frame — buys a non-stalling profiler at
    /// the cost of every reported number being about a frame nobody asked about.
    ///
    /// Returns `None` rather than an error if anything goes wrong: a profiler that takes the game
    /// down is worse than one that says nothing.
    fn read_timings(&self, timed: &[(usize, String)]) -> Option<GpuFrameTiming> {
        let timing = self.timing.as_ref()?;
        if !timing.enabled {
            return None;
        }

        let slice = timing.readback.slice(..);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        self.device
            .poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: None,
            })
            .ok()?;

        let mapped = slice.get_mapped_range().ok()?;
        let ticks: Vec<u64> = mapped
            .chunks_exact(8)
            .map(|bytes| u64::from_le_bytes(bytes.try_into().unwrap_or([0; 8])))
            .collect();
        drop(mapped);
        timing.readback.unmap();

        let to_duration = |ticks: u64| {
            // The period is nanoseconds per tick and is a property of the queue.
            std::time::Duration::from_nanos((ticks as f64 * f64::from(timing.period)) as u64)
        };

        let mut passes = Vec::with_capacity(timed.len());
        let mut first_begin = u64::MAX;
        let mut last_end = 0_u64;

        for (slot, label) in timed {
            let begin = *ticks.get(slot * 2)?;
            let end = *ticks.get(slot * 2 + 1)?;
            // A pass whose timestamps came back out of order is a driver quirk rather than a
            // negative duration; reporting zero is the honest reading of "too fast to measure".
            passes.push((label.clone(), to_duration(end.saturating_sub(begin))));
            first_begin = first_begin.min(begin);
            last_end = last_end.max(end);
        }

        Some(GpuFrameTiming {
            total: to_duration(last_end.saturating_sub(first_begin)),
            passes,
        })
    }

    /// Turns GPU pass timing on or off — **exit gate 4**.
    ///
    /// **Off by default**, and not out of caution: reading the results means waiting for the GPU to
    /// finish the frame, which is a full pipeline stall. A profiler may stall; a game may not. So a
    /// measurement harness turns this on and nothing else does.
    ///
    /// Does nothing on an adapter without `TIMESTAMP_QUERY`, where
    /// [`WgpuBackend::supports_gpu_timing`] is `false` and timings are always `None`.
    pub fn set_gpu_timing(&mut self, on: bool) {
        if let Some(timing) = self.timing.as_mut() {
            timing.enabled = on;
        }
    }

    /// Whether this adapter can time GPU work at all.
    #[must_use]
    pub fn supports_gpu_timing(&self) -> bool {
        self.timing.is_some()
    }

    /// What the last drawn frame cost on the GPU, if timing was on for it.
    #[must_use]
    pub fn last_gpu_timing(&self) -> Option<&GpuFrameTiming> {
        self.last_timing.as_ref()
    }
}
