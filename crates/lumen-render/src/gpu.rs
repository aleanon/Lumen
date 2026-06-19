//! The GPU backend (wgpu, ADR-001), offscreen.
//!
//! Renders the display list to an offscreen texture and reads it back to an
//! [`RgbaImage`], with no window or display required. M0 implements the two
//! command classes app UIs use — solid [`DrawCmd::Rect`] fills and
//! [`DrawCmd::Image`] blits — which is enough to render `examples/hello` on the
//! GPU and to parity-test against the CPU reference renderer (05 §4). Gradients,
//! paths, layers, glyph runs, and shaders on the GPU are later work.

use crate::display_list::{Brush, DisplayList, DrawCmd};
use crate::image::RgbaImage;
use lumen_core::Color;
use std::borrow::Cow;

/// A headless wgpu renderer.
pub struct GpuRenderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    rect_pipeline: wgpu::RenderPipeline,
    image_pipeline: wgpu::RenderPipeline,
    image_bgl: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct RectInstance {
    rect: [f32; 4],
    color: [f32; 4],
}

const TARGET_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;

/// The GPU backend as a runtime-selectable [`Renderer`](crate::Renderer) (A1).
/// Covers the command set the offscreen backend supports (solid rects + image
/// blits — which includes rasterized text/shadow images); paths/gradients/layers
/// and HiDPI scaling on the GPU are follow-on, so it renders at 1:1.
impl crate::Renderer for GpuRenderer {
    fn render_frame(
        &mut self,
        list: &DisplayList,
        width: u32,
        height: u32,
        _scale: f64,
        background: Color,
    ) -> RgbaImage {
        self.render(list, width, height, background)
    }

    fn name(&self) -> &'static str {
        "gpu"
    }
}

impl GpuRenderer {
    /// Create a headless renderer, or `None` if no adapter is available.
    pub fn new() -> Option<GpuRenderer> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });
        let adapter = block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: None,
            force_fallback_adapter: false,
        }))?;
        let (device, queue) = block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("lumen-gpu"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::downlevel_defaults(),
                memory_hints: wgpu::MemoryHints::default(),
            },
            None,
        ))
        .ok()?;

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("lumen-shaders"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(SHADER)),
        });

        let viewport_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("viewport"),
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
        let image_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("image"),
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
            ],
        });

        let blend = Some(wgpu::BlendState::ALPHA_BLENDING);
        let target = wgpu::ColorTargetState {
            format: TARGET_FORMAT,
            blend,
            write_mask: wgpu::ColorWrites::ALL,
        };

        let rect_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("rect-layout"),
            bind_group_layouts: &[&viewport_bgl],
            push_constant_ranges: &[],
        });
        let rect_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("rect"),
            layout: Some(&rect_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "rect_vs",
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<RectInstance>() as u64,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32x4],
                }],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "rect_fs",
                targets: &[Some(target.clone())],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        let image_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("image-layout"),
            bind_group_layouts: &[&viewport_bgl, &image_bgl],
            push_constant_ranges: &[],
        });
        let image_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("image"),
            layout: Some(&image_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "image_vs",
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<RectInstance>() as u64,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32x4],
                }],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "image_fs",
                targets: &[Some(target)],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("nearest"),
            ..Default::default()
        });

        Some(GpuRenderer {
            device,
            queue,
            rect_pipeline,
            image_pipeline,
            image_bgl,
            sampler,
        })
    }

    /// Render `list` to a `width`×`height` image over `background`.
    pub fn render(
        &self,
        list: &DisplayList,
        width: u32,
        height: u32,
        background: Color,
    ) -> RgbaImage {
        use wgpu::util::DeviceExt;
        let device = &self.device;

        let target = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("target"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: TARGET_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = target.create_view(&Default::default());

        let viewport = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("viewport"),
            contents: bytemuck_lite::bytes_of(&[width as f32, height as f32, 0.0, 0.0]),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let viewport_bgl = self.rect_pipeline.get_bind_group_layout(0);
        let viewport_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("viewport-bg"),
            layout: &viewport_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: viewport.as_entire_binding(),
            }],
        });

        // Collect rect instances; images are drawn individually (own textures).
        let mut rects: Vec<RectInstance> = Vec::new();
        struct ImageDraw {
            instance: RectInstance,
            bind: wgpu::BindGroup,
        }
        let mut images: Vec<ImageDraw> = Vec::new();
        for cmd in &list.cmds {
            match cmd {
                DrawCmd::Rect {
                    rect,
                    brush: Brush::Solid(c),
                    ..
                } => rects.push(RectInstance {
                    rect: [
                        rect.x0 as f32,
                        rect.y0 as f32,
                        rect.width() as f32,
                        rect.height() as f32,
                    ],
                    color: [c.r, c.g, c.b, c.a],
                }),
                DrawCmd::Image { id, dst_rect, .. } => {
                    if let Some(img) = list.images.get(id.0 as usize) {
                        let bind = self.upload_image(img);
                        images.push(ImageDraw {
                            instance: RectInstance {
                                rect: [
                                    dst_rect.x0 as f32,
                                    dst_rect.y0 as f32,
                                    dst_rect.width() as f32,
                                    dst_rect.height() as f32,
                                ],
                                color: [1.0, 1.0, 1.0, 1.0],
                            },
                            bind,
                        });
                    }
                }
                _ => { /* gradients/paths/layers/glyphs/shader: GPU later */ }
            }
        }

        let rect_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("rects"),
            contents: if rects.is_empty() {
                bytemuck_lite::cast_slice(&[RectInstance {
                    rect: [0.0; 4],
                    color: [0.0; 4],
                }])
            } else {
                bytemuck_lite::cast_slice(&rects)
            },
            usage: wgpu::BufferUsages::VERTEX,
        });

        // Per-image instance buffers, created up front so they outlive the pass.
        let image_buffers: Vec<wgpu::Buffer> = images
            .iter()
            .map(|img| {
                device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("img-instance"),
                    contents: bytemuck_lite::bytes_of(&img.instance),
                    usage: wgpu::BufferUsages::VERTEX,
                })
            })
            .collect();

        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let bg = [
                background.r as f64,
                background.g as f64,
                background.b as f64,
                background.a as f64,
            ];
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("main"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: bg[0],
                            g: bg[1],
                            b: bg[2],
                            a: bg[3],
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            if !rects.is_empty() {
                pass.set_pipeline(&self.rect_pipeline);
                pass.set_bind_group(0, &viewport_bg, &[]);
                pass.set_vertex_buffer(0, rect_buf.slice(..));
                pass.draw(0..6, 0..rects.len() as u32);
            }
            if !images.is_empty() {
                pass.set_pipeline(&self.image_pipeline);
                pass.set_bind_group(0, &viewport_bg, &[]);
                for (img, buf) in images.iter().zip(&image_buffers) {
                    pass.set_bind_group(1, &img.bind, &[]);
                    pass.set_vertex_buffer(0, buf.slice(..));
                    pass.draw(0..6, 0..1);
                }
            }
        }

        // Readback.
        let bpr = padded_bytes_per_row(width);
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("readback"),
            size: (bpr * height) as u64,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        encoder.copy_texture_to_buffer(
            wgpu::ImageCopyTexture {
                texture: &target,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::ImageCopyBuffer {
                buffer: &readback,
                layout: wgpu::ImageDataLayout {
                    offset: 0,
                    bytes_per_row: Some(bpr),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        self.queue.submit(Some(encoder.finish()));

        let slice = readback.slice(..);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        self.device.poll(wgpu::Maintain::Wait);
        let data = slice.get_mapped_range();
        let mut pixels = Vec::with_capacity((width * height * 4) as usize);
        for row in 0..height {
            let start = (row * bpr) as usize;
            pixels.extend_from_slice(&data[start..start + (width * 4) as usize]);
        }
        drop(data);
        readback.unmap();
        RgbaImage::from_raw(width, height, pixels)
    }

    fn upload_image(&self, img: &RgbaImage) -> wgpu::BindGroup {
        let size = wgpu::Extent3d {
            width: img.width(),
            height: img.height(),
            depth_or_array_layers: 1,
        };
        let tex = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("img"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: TARGET_FORMAT,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        self.queue.write_texture(
            wgpu::ImageCopyTexture {
                texture: &tex,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            img.pixels(),
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(img.width() * 4),
                rows_per_image: Some(img.height()),
            },
            size,
        );
        let view = tex.create_view(&Default::default());
        self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("img-bg"),
            layout: &self.image_bgl,
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
        })
    }

    /// Render a WGSL fragment shader to an [`RgbaImage`] (T4.1 ShaderWidget).
    ///
    /// `fragment` must define `@fragment fn fs_main(@location(0) uv: vec2<f32>)
    /// -> @location(0) vec4<f32>` and may read the bound `u: Uniforms`
    /// (`resolution`, `time`, `params`). On a WGSL compile/validation error the
    /// returned `Err` carries an `E0201` diagnostic and no pipeline is built.
    pub fn render_shader(
        &self,
        fragment: &str,
        uniforms: ShaderUniforms,
        width: u32,
        height: u32,
    ) -> Result<RgbaImage, lumen_core::Diagnostic> {
        let src = format!("{SHADER_HEADER}\n{fragment}");

        // Capture WGSL validation errors instead of panicking, so a broken edit
        // becomes a diagnostic and the caller keeps the previous pipeline.
        self.device.push_error_scope(wgpu::ErrorFilter::Validation);
        let module = self
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("user-shader"),
                source: wgpu::ShaderSource::Wgsl(Cow::Owned(src)),
            });
        if let Some(err) = block_on(self.device.pop_error_scope()) {
            return Err(lumen_core::Diagnostic::new(
                lumen_core::codes::E0201,
                err.to_string(),
            ));
        }

        let ubgl = self
            .device
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("shader-uniforms"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });
        let layout = self
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("shader-layout"),
                bind_group_layouts: &[&ubgl],
                push_constant_ranges: &[],
            });
        self.device.push_error_scope(wgpu::ErrorFilter::Validation);
        let pipeline = self
            .device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("shader-pipeline"),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &module,
                    entry_point: "vs_main",
                    buffers: &[],
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &module,
                    entry_point: "fs_main",
                    targets: &[Some(wgpu::ColorTargetState {
                        format: TARGET_FORMAT,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: Default::default(),
                }),
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview: None,
                cache: None,
            });
        if let Some(err) = block_on(self.device.pop_error_scope()) {
            return Err(lumen_core::Diagnostic::new(
                lumen_core::codes::E0201,
                err.to_string(),
            ));
        }

        // Uniform buffer: resolution(vec2), time(f32), _pad, params(vec4).
        let data: [f32; 8] = [
            width as f32,
            height as f32,
            uniforms.time,
            0.0,
            uniforms.params[0],
            uniforms.params[1],
            uniforms.params[2],
            uniforms.params[3],
        ];
        let ubuf = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("u"),
            size: std::mem::size_of_val(&data) as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.queue.write_buffer(&ubuf, 0, f32s_as_bytes(&data));
        let ubg = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ubg"),
            layout: &ubgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: ubuf.as_entire_binding(),
            }],
        });

        let target = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("shader-target"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: TARGET_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = target.create_view(&Default::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("shader-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &ubg, &[]);
            pass.draw(0..3, 0..1);
        }

        let bpr = padded_bytes_per_row(width);
        let readback = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("shader-readback"),
            size: (bpr * height) as u64,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        encoder.copy_texture_to_buffer(
            wgpu::ImageCopyTexture {
                texture: &target,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::ImageCopyBuffer {
                buffer: &readback,
                layout: wgpu::ImageDataLayout {
                    offset: 0,
                    bytes_per_row: Some(bpr),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        self.queue.submit(Some(encoder.finish()));

        let slice = readback.slice(..);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        self.device.poll(wgpu::Maintain::Wait);
        let mapped = slice.get_mapped_range();
        let mut pixels = Vec::with_capacity((width * height * 4) as usize);
        for row in 0..height {
            let start = (row * bpr) as usize;
            pixels.extend_from_slice(&mapped[start..start + (width * 4) as usize]);
        }
        drop(mapped);
        readback.unmap();
        Ok(RgbaImage::from_raw(width, height, pixels))
    }
}

/// Typed shader uniforms (T4.1): `time` plus four free `params`. `resolution` is
/// supplied automatically from the render size.
#[derive(Clone, Copy, Default)]
pub struct ShaderUniforms {
    /// Seconds since start (drives animation).
    pub time: f32,
    /// Four user parameters, bound as `u.params`.
    pub params: [f32; 4],
}

/// Common WGSL prelude prepended to every ShaderWidget fragment: the `Uniforms`
/// binding and a fullscreen-triangle vertex shader exposing `uv` in `[0,1]`.
const SHADER_HEADER: &str = r#"
struct Uniforms { resolution: vec2<f32>, time: f32, _pad: f32, params: vec4<f32>, };
@group(0) @binding(0) var<uniform> u: Uniforms;
struct VsOut { @builtin(position) pos: vec4<f32>, @location(0) uv: vec2<f32>, };
@vertex fn vs_main(@builtin(vertex_index) i: u32) -> VsOut {
    var p = array<vec2<f32>, 3>(vec2<f32>(-1.0, -1.0), vec2<f32>(3.0, -1.0), vec2<f32>(-1.0, 3.0));
    var o: VsOut;
    o.pos = vec4<f32>(p[i], 0.0, 1.0);
    o.uv = p[i] * 0.5 + 0.5;
    return o;
}
"#;

/// Reinterpret an f32 slice as bytes for buffer uploads (avoids a bytemuck dep).
fn f32s_as_bytes(data: &[f32]) -> &[u8] {
    // SAFETY: f32 has no padding/invalid bit patterns; length is exact.
    unsafe { std::slice::from_raw_parts(data.as_ptr() as *const u8, std::mem::size_of_val(data)) }
}

fn padded_bytes_per_row(width: u32) -> u32 {
    let unpadded = width * 4;
    let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    unpadded.div_ceil(align) * align
}

/// Minimal block-on for wgpu's native futures (they resolve without an external
/// executor on native backends). Avoids a `pollster` dependency.
fn block_on<F: std::future::Future>(fut: F) -> F::Output {
    let waker = std::task::Waker::noop();
    let mut cx = std::task::Context::from_waker(waker);
    let mut fut = std::pin::pin!(fut);
    loop {
        if let std::task::Poll::Ready(v) = fut.as_mut().poll(&mut cx) {
            return v;
        }
    }
}

const SHADER: &str = r#"
struct Viewport { size: vec2<f32>, _pad: vec2<f32> };
@group(0) @binding(0) var<uniform> viewport: Viewport;

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) uv: vec2<f32>,
};

// Unit quad corner for vertex index (two triangles).
fn corner(i: u32) -> vec2<f32> {
    var c = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 0.0), vec2<f32>(0.0, 1.0),
        vec2<f32>(0.0, 1.0), vec2<f32>(1.0, 0.0), vec2<f32>(1.0, 1.0),
    );
    return c[i];
}

fn place(rect: vec4<f32>, c: vec2<f32>) -> vec4<f32> {
    let px = rect.xy + c * rect.zw;
    let ndc = vec2<f32>(px.x / viewport.size.x * 2.0 - 1.0,
                        1.0 - px.y / viewport.size.y * 2.0);
    return vec4<f32>(ndc, 0.0, 1.0);
}

@vertex
fn rect_vs(@builtin(vertex_index) vi: u32,
           @location(0) rect: vec4<f32>,
           @location(1) color: vec4<f32>) -> VsOut {
    let c = corner(vi);
    var o: VsOut;
    o.pos = place(rect, c);
    o.color = color;
    o.uv = c;
    return o;
}

@fragment
fn rect_fs(in: VsOut) -> @location(0) vec4<f32> {
    return in.color;
}

@group(1) @binding(0) var img_tex: texture_2d<f32>;
@group(1) @binding(1) var img_samp: sampler;

@vertex
fn image_vs(@builtin(vertex_index) vi: u32,
            @location(0) rect: vec4<f32>,
            @location(1) color: vec4<f32>) -> VsOut {
    let c = corner(vi);
    var o: VsOut;
    o.pos = place(rect, c);
    o.color = color;
    o.uv = c;
    return o;
}

@fragment
fn image_fs(in: VsOut) -> @location(0) vec4<f32> {
    return textureSample(img_tex, img_samp, in.uv) * in.color;
}
"#;

/// A tiny `Pod`/bytemuck stand-in so we don't add a dependency: these helpers
/// transmute plain-old-data structs to bytes. Sound because the types are
/// `#[repr(C)]` and contain only `f32` arrays.
mod bytemuck_lite {
    /// View a `T` as bytes.
    pub fn bytes_of<T: Copy>(t: &T) -> &[u8] {
        unsafe { std::slice::from_raw_parts(t as *const T as *const u8, std::mem::size_of::<T>()) }
    }
    /// View a slice of `T` as bytes.
    pub fn cast_slice<T: Copy>(s: &[T]) -> &[u8] {
        unsafe { std::slice::from_raw_parts(s.as_ptr() as *const u8, std::mem::size_of_val(s)) }
    }
}
