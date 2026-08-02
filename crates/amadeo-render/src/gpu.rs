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

use crate::backend::{FrameData, RenderBackend, RenderError};
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

/// How many instances the buffer starts with. Grows as needed; never shrinks.
const INITIAL_INSTANCE_CAPACITY: usize = 256;

/// One uploaded texture and the bind group that binds it.
///
/// The bind group is built once at upload and kept, rather than rebuilt per frame: creating one is a
/// driver-side allocation, and doing it per batch per frame would reintroduce exactly the per-draw
/// cost ADR 0023's batching exists to remove.
///
/// The `wgpu::Texture` is held alongside the view purely to keep it alive — dropping it would
/// invalidate the view and the bind group that reference it.
#[derive(Debug)]
struct GpuTexture {
    #[allow(
        dead_code,
        reason = "held to keep the texture alive; the view and bind group borrow from it"
    )]
    texture: wgpu::Texture,
    bind_group: wgpu::BindGroup,
}

/// A wgpu-backed renderer drawing into a window surface.
#[derive(Debug)]
pub struct WgpuBackend {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    pipeline: wgpu::RenderPipeline,
    sprite_pipeline: wgpu::RenderPipeline,
    camera_buffer: wgpu::Buffer,
    camera_bind_group: wgpu::BindGroup,
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
    /// One sampler shared by every texture.
    ///
    /// **Nearest-neighbour, deliberately.** Three of the eight target games are pixel art, where
    /// linear filtering turns crisp art into mush, and the `.ama-meta` sidecar already carries a
    /// `filter` setting for the day this becomes per-asset. One sampler is also one fewer thing to
    /// switch between batches.
    sampler: wgpu::Sampler,
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
            required_features: wgpu::Features::empty(),
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

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("amadeo quad shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("quad.wgsl").into()),
        });

        let camera_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("amadeo camera uniform"),
            size: size_of::<GpuCamera>() as u64,
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
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("amadeo camera bind group"),
            layout: &camera_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buffer.as_entire_binding(),
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
                    format,
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
            multisample: wgpu::MultisampleState::default(),
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

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("amadeo sprite sampler"),
            // Clamping rather than repeating: a `region` is a rectangle inside a shared sheet, and
            // repeating would bleed a neighbouring tile in at the seam.
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            // A separate enum from the min/mag filters in wgpu 30, and only relevant once textures
            // carry mip levels -- which they do not until the import pipeline generates them.
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
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
                    format,
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
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        let sprite_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("amadeo sprite buffer"),
            size: (size_of::<GpuSprite>() * INITIAL_INSTANCE_CAPACITY) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Ok(Self {
            surface,
            device,
            queue,
            config,
            pipeline,
            sprite_pipeline,
            camera_buffer,
            camera_bind_group,
            instance_buffer,
            instance_capacity: INITIAL_INSTANCE_CAPACITY,
            sprite_buffer,
            sprite_capacity: INITIAL_INSTANCE_CAPACITY,
            textures: BTreeMap::new(),
            texture_layout,
            sampler,
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
}

impl RenderBackend for WgpuBackend {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn name(&self) -> &'static str {
        "wgpu"
    }

    fn viewport(&self) -> (u32, u32) {
        (self.config.width, self.config.height)
    }

    fn resize(&mut self, width: u32, height: u32) {
        // A minimised window reports zero. Reconfiguring at that size is invalid, so the last valid
        // size is kept until the window comes back.
        if width == 0 || height == 0 {
            return;
        }
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
    }

    fn has_texture(&self, id: &str) -> bool {
        self.textures.contains_key(id)
    }

    fn upload_texture(&mut self, id: &str, texture: &TextureData) -> Result<(), RenderError> {
        let format = match texture.format {
            // `Srgb` here and an sRGB surface format together are what make colours come out right:
            // the art is gamma-encoded, the GPU converts to linear when sampling, blending happens
            // in linear, and the surface converts back on the way out.
            PixelFormat::Rgba8UnormSrgb => wgpu::TextureFormat::Rgba8UnormSrgb,
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

        let gpu_texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some(id),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        self.queue.write_texture(
            gpu_texture.as_image_copy(),
            &texture.pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                // `TextureData` guarantees tightly packed rows with no padding, which is why this is
                // a plain multiply and not a round-up to 256. That guarantee is checked once, in
                // `TextureData::new`, rather than here.
                bytes_per_row: Some(texture.width * texture.format.bytes_per_pixel()),
                rows_per_image: Some(texture.height),
            },
            size,
        );

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

        // Inserting replaces any earlier texture under this id, and dropping the old one releases
        // its video memory. That is what makes a late-arriving asset, and later hot-reload, work.
        self.textures.insert(
            id.to_string(),
            GpuTexture {
                texture: gpu_texture,
                bind_group,
            },
        );
        Ok(())
    }

    fn render(&mut self, frame: &FrameData) -> Result<(), RenderError> {
        // wgpu reports several non-fatal reasons a frame cannot be drawn right now. Each one means
        // "skip this frame", not "the renderer is broken": the window is being resized, minimised,
        // or covered. Treating any of them as fatal would kill a game on an ordinary alt-tab.
        let surface_texture = match self.surface.get_current_texture() {
            // Suboptimal still draws correctly — usually the surface wants reconfiguring, which the
            // next resize event will do anyway.
            wgpu::CurrentSurfaceTexture::Success(texture)
            | wgpu::CurrentSurfaceTexture::Suboptimal(texture) => texture,
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                // The surface needs rebuilding at its current size before anything can be drawn.
                self.surface.configure(&self.device, &self.config);
                return Err(RenderError::SurfaceUnavailable {
                    reason: "surface outdated or lost; reconfigured for the next frame".to_string(),
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
        };

        let view = surface_texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        // Camera: the world half-size the view covers. Width follows the aspect ratio so resizing
        // shows more world rather than stretching what is already visible.
        let aspect = self.config.width as f32 / self.config.height as f32;
        let half_height = frame.camera.height / 2.0;
        let camera = GpuCamera {
            center: frame.camera.center,
            half_extents: [half_height * aspect, half_height],
        };
        self.queue
            .write_buffer(&self.camera_buffer, 0, bytemuck::bytes_of(&camera));

        let instances: Vec<GpuInstance> = frame
            .quads
            .iter()
            .map(|quad| GpuInstance {
                center_size: [quad.center[0], quad.center[1], quad.size[0], quad.size[1]],
                rotation: [quad.rotation, 0.0, 0.0, 0.0],
                color: quad.color,
            })
            .collect();

        self.ensure_instance_capacity(instances.len());
        if !instances.is_empty() {
            self.queue
                .write_buffer(&self.instance_buffer, 0, bytemuck::cast_slice(&instances));
        }

        // Every batch's sprites go into **one** buffer, laid out back to back in batch order, and
        // each batch then draws its own slice of it. So the number of buffer writes per frame is
        // one regardless of how many batches there are -- the batches only decide how many times
        // the texture bind group changes, which is the cost ADR 0023 is actually about.
        //
        // `first` is recorded alongside each batch here rather than recomputed in the pass, because
        // a running offset maintained in two places is exactly the sort of thing that drifts.
        let mut sprites: Vec<GpuSprite> = Vec::with_capacity(frame.sprite_count());
        let mut draws: Vec<(&str, std::ops::Range<u32>)> = Vec::with_capacity(frame.batches.len());
        for batch in &frame.batches {
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

        self.ensure_sprite_capacity(sprites.len());
        if !sprites.is_empty() {
            self.queue
                .write_buffer(&self.sprite_buffer, 0, bytemuck::cast_slice(&sprites));
        }

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("amadeo frame encoder"),
            });

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("amadeo quad pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: f64::from(frame.clear_color[0]),
                            g: f64::from(frame.clear_color[1]),
                            b: f64::from(frame.clear_color[2]),
                            a: f64::from(frame.clear_color[3]),
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                multiview_mask: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            // The pass still runs with zero instances: clearing the screen is the point, otherwise
            // the previous frame's image would persist.
            if !instances.is_empty() {
                pass.set_pipeline(&self.pipeline);
                pass.set_bind_group(0, &self.camera_bind_group, &[]);
                pass.set_vertex_buffer(0, self.instance_buffer.slice(..));
                // Four vertices (one strip), once per instance. One draw call for the whole frame.
                pass.draw(0..4, 0..instances.len() as u32);
            }

            // Sprites draw after quads, so a textured sprite sits over an untextured rectangle at
            // the same position. `SortOrder` governs order *within* each of the two, and the two
            // passes do not interleave -- worth knowing, and the reason a background drawn as a
            // `Quad` behind sprites works without any sort order at all.
            if !sprites.is_empty() {
                pass.set_pipeline(&self.sprite_pipeline);
                pass.set_bind_group(0, &self.camera_bind_group, &[]);
                pass.set_vertex_buffer(0, self.sprite_buffer.slice(..));

                for (texture_id, range) in &draws {
                    // A batch naming a texture that was never uploaded is skipped rather than
                    // drawn untextured. It should not happen -- `Renderer::upload_frame_textures`
                    // uploads at least a placeholder for every batch before this runs -- and
                    // binding the previous batch's texture instead would draw the wrong picture
                    // silently, which is worse than a gap.
                    let Some(texture) = self.textures.get(*texture_id) else {
                        continue;
                    };
                    pass.set_bind_group(1, &texture.bind_group, &[]);
                    // The one state change per batch that the whole batcher exists to minimise.
                    pass.draw(0..4, range.clone());
                }
            }
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        // Presentation moved onto the queue in wgpu 30; it used to be a method on the texture.
        self.queue.present(surface_texture);
        Ok(())
    }
}
