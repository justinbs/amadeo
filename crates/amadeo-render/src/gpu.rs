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
use std::any::Any;

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

/// The camera as the GPU sees it.
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct GpuCamera {
    center: [f32; 2],
    half_extents: [f32; 2],
}

/// How many instances the buffer starts with. Grows as needed; never shrinks.
const INITIAL_INSTANCE_CAPACITY: usize = 256;

/// A wgpu-backed renderer drawing into a window surface.
#[derive(Debug)]
pub struct WgpuBackend {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    pipeline: wgpu::RenderPipeline,
    camera_buffer: wgpu::Buffer,
    camera_bind_group: wgpu::BindGroup,
    instance_buffer: wgpu::Buffer,
    /// How many instances the current buffer can hold.
    instance_capacity: usize,
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

        Ok(Self {
            surface,
            device,
            queue,
            config,
            pipeline,
            camera_buffer,
            camera_bind_group,
            instance_buffer,
            instance_capacity: INITIAL_INSTANCE_CAPACITY,
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
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        // Presentation moved onto the queue in wgpu 30; it used to be a method on the texture.
        self.queue.present(surface_texture);
        Ok(())
    }
}
