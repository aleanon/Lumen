//! The GPU backend (wgpu, ADR-001), offscreen.
//!
//! Renders the display list to an offscreen texture and reads it back to an
//! [`RgbaImage`], with no window or display required. Supported commands:
//! solid-fill [`DrawCmd::Rect`] — square or rounded, with an optional centered
//! border, via a rounded-box SDF with 1px analytic AA (R1.2); solid-fill/stroke
//! [`DrawCmd::Path`] tessellated by `lyon` with MSAA edge AA (R1.3);
//! gradient-filled rects (linear/radial/conic) via an Oklab ramp texture (R1.4);
//! and [`DrawCmd::Image`] blits. Layers, glyph runs, and shaders on the GPU are
//! later R1 sub-phases. Parity with the CPU reference is gated by
//! `tests/cpu_vs_gpu` (05 §4).

use crate::display_list::{Brush, DisplayList, DrawCmd, FillOrStroke};
use crate::image::RgbaImage;
use lumen_core::Color;
use std::borrow::Cow;

/// A headless wgpu renderer.
pub struct GpuRenderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    rect_pipeline: wgpu::RenderPipeline,
    image_pipeline: wgpu::RenderPipeline,
    path_pipeline: wgpu::RenderPipeline,
    gradient_pipeline: wgpu::RenderPipeline,
    image_bgl: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    /// Linear, clamp-to-edge sampler for 1-D gradient ramp textures (R1.4).
    ramp_sampler: wgpu::Sampler,
    /// MSAA sample count for the offscreen target (4/2/1, whatever the adapter
    /// supports for `TARGET_FORMAT`). Gives anti-aliasing to tessellated paths
    /// (R1.3); the SDF rect fill is alpha-coverage based and unaffected.
    sample_count: u32,
}

/// One tessellated path vertex (logical px position + straight-alpha color).
#[repr(C)]
#[derive(Clone, Copy)]
struct PathVertex {
    pos: [f32; 2],
    color: [f32; 4],
}

/// A gradient-filled rect instance (R1.4). The ramp colors live in a per-instance
/// 1-D texture; this carries only the rect and the spatial mapping.
#[repr(C)]
#[derive(Clone, Copy)]
struct GradInstance {
    /// `[x0, y0, width, height]`.
    rect: [f32; 4],
    /// linear: `[start.x, start.y, end.x, end.y]`; radial: `[cx, cy, radius, _]`;
    /// conic: `[cx, cy, start_angle, _]`.
    g0: [f32; 4],
    /// `[kind (0=linear,1=radial,2=conic), spread (0=pad,1=repeat,2=reflect), _, _]`.
    meta: [f32; 4],
}

/// Texels per gradient ramp texture (1-D). Dense enough that linear filtering
/// reproduces the CPU's Oklab ramp within the gradient tolerance.
const RAMP_TEXELS: u32 = 512;

#[repr(C)]
#[derive(Clone, Copy)]
struct RectInstance {
    /// `[x0, y0, width, height]` in logical px.
    rect: [f32; 4],
    /// Fill color (straight alpha).
    color: [f32; 4],
    /// Corner radii `[tl, tr, br, bl]` in logical px.
    radii: [f32; 4],
    /// Border color (straight alpha); ignored when `misc.x == 0`.
    bcolor: [f32; 4],
    /// `[border_width, 0, 0, 0]`.
    misc: [f32; 4],
}

impl RectInstance {
    /// A plain instance (square corners, no border) — used for image blits.
    fn plain(rect: [f32; 4], color: [f32; 4]) -> RectInstance {
        RectInstance {
            rect,
            color,
            radii: [0.0; 4],
            bcolor: [0.0; 4],
            misc: [0.0; 4],
        }
    }
}

const TARGET_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;

/// The GPU backend as a runtime-selectable [`Renderer`](crate::Renderer) (A1).
/// Covers the command set the offscreen backend supports (solid rects — square
/// or rounded with a centered border — and image blits, which include
/// rasterized text/shadow sprites); paths/gradients/layers and HiDPI scaling on
/// the GPU are follow-on, so it renders at 1:1.
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

        // Pick the best MSAA level the adapter supports for our target format
        // (paths get geometry AA from it). downlevel hardware may only do 1×.
        let flags = adapter.get_texture_format_features(TARGET_FORMAT).flags;
        let sample_count = [4u32, 2, 1]
            .into_iter()
            .find(|&n| flags.sample_count_supported(n))
            .unwrap_or(1);
        let multisample = wgpu::MultisampleState {
            count: sample_count,
            ..Default::default()
        };

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
                    attributes: &wgpu::vertex_attr_array![
                        0 => Float32x4, 1 => Float32x4, 2 => Float32x4,
                        3 => Float32x4, 4 => Float32x4
                    ],
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
            multisample,
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
                targets: &[Some(target.clone())],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample,
            multiview: None,
            cache: None,
        });

        // Tessellated-path pipeline (R1.3): non-instanced (pos, color) triangles.
        let path_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("path-layout"),
            bind_group_layouts: &[&viewport_bgl],
            push_constant_ranges: &[],
        });
        let path_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("path"),
            layout: Some(&path_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "path_vs",
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<PathVertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x4],
                }],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "path_fs",
                targets: &[Some(target.clone())],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample,
            multiview: None,
            cache: None,
        });

        // Gradient pipeline (R1.4): per-instance rect + ramp texture; the
        // fragment computes the spatial parameter and samples the ramp.
        let gradient_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("gradient-layout"),
            bind_group_layouts: &[&viewport_bgl, &image_bgl],
            push_constant_ranges: &[],
        });
        let gradient_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("gradient"),
            layout: Some(&gradient_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "gradient_vs",
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<GradInstance>() as u64,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32x4, 2 => Float32x4],
                }],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "gradient_fs",
                targets: &[Some(target)],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample,
            multiview: None,
            cache: None,
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("nearest"),
            ..Default::default()
        });
        let ramp_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("ramp-linear"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        Some(GpuRenderer {
            device,
            queue,
            rect_pipeline,
            image_pipeline,
            path_pipeline,
            gradient_pipeline,
            image_bgl,
            sampler,
            ramp_sampler,
            sample_count,
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

        // Single-sample resolve target — the texture we read back.
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

        // When MSAA is available, draw into a multisampled attachment and resolve
        // into `target`; otherwise draw straight into `target`.
        let msaa_tex = (self.sample_count > 1).then(|| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some("msaa"),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: self.sample_count,
                dimension: wgpu::TextureDimension::D2,
                format: TARGET_FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            })
        });
        let msaa_view = msaa_tex
            .as_ref()
            .map(|t| t.create_view(&Default::default()));
        let (attach_view, resolve_target) = match &msaa_view {
            Some(v) => (v, Some(&view)),
            None => (&view, None),
        };

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

        // Collect rect instances; images are drawn individually (own textures);
        // paths are tessellated into one shared vertex/index buffer.
        let mut rects: Vec<RectInstance> = Vec::new();
        struct ImageDraw {
            instance: RectInstance,
            bind: wgpu::BindGroup,
        }
        let mut images: Vec<ImageDraw> = Vec::new();
        let mut path_geo = PathGeometry::default();
        struct GradDraw {
            instance: GradInstance,
            bind: wgpu::BindGroup,
        }
        let mut gradients: Vec<GradDraw> = Vec::new();
        for cmd in &list.cmds {
            match cmd {
                // Solid-fill/stroke paths → lyon triangles (R1.3). Gradient
                // paths are R1.4.
                DrawCmd::Path {
                    path,
                    brush: Brush::Solid(c),
                    style,
                } => path_geo.add(path, [c.r, c.g, c.b, c.a], *style),
                // Gradient-filled rects → ramp-texture sampling (R1.4). Square
                // corners only for now (corpus); rounded gradient rects later.
                DrawCmd::Rect {
                    rect,
                    brush:
                        brush @ (Brush::LinearGradient { .. }
                        | Brush::RadialGradient { .. }
                        | Brush::ConicGradient { .. }),
                    ..
                } => {
                    if let Some((instance, stops)) = grad_instance(rect, brush) {
                        let bind = self.upload_ramp(stops);
                        gradients.push(GradDraw { instance, bind });
                    }
                }
                // Solid-fill rects (square or rounded, with optional centered
                // border) go through the rounded-box SDF pipeline (R1.2).
                // Gradient-filled rects are R1.4.
                DrawCmd::Rect {
                    rect,
                    brush: Brush::Solid(c),
                    radii,
                    border,
                } => {
                    let (bcolor, bw) = match border {
                        Some(b) => ([b.color.r, b.color.g, b.color.b, b.color.a], b.width as f32),
                        None => ([0.0; 4], 0.0),
                    };
                    rects.push(RectInstance {
                        rect: [
                            rect.x0 as f32,
                            rect.y0 as f32,
                            rect.width() as f32,
                            rect.height() as f32,
                        ],
                        color: [c.r, c.g, c.b, c.a],
                        radii: [
                            radii.tl as f32,
                            radii.tr as f32,
                            radii.br as f32,
                            radii.bl as f32,
                        ],
                        bcolor,
                        misc: [bw, 0.0, 0.0, 0.0],
                    });
                }
                DrawCmd::Image { id, dst_rect, .. } => {
                    if let Some(img) = list.images.get(id.0 as usize) {
                        let bind = self.upload_image(img);
                        images.push(ImageDraw {
                            instance: RectInstance::plain(
                                [
                                    dst_rect.x0 as f32,
                                    dst_rect.y0 as f32,
                                    dst_rect.width() as f32,
                                    dst_rect.height() as f32,
                                ],
                                [1.0, 1.0, 1.0, 1.0],
                            ),
                            bind,
                        });
                    }
                }
                _ => { /* gradients/paths/layers/glyphs/shader: GPU later */ }
            }
        }

        // A non-empty buffer is required even with zero rects (draw count 0).
        let empty = [RectInstance::plain([0.0; 4], [0.0; 4])];
        let rect_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("rects"),
            contents: if rects.is_empty() {
                bytemuck_lite::cast_slice(&empty)
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

        // Tessellated-path buffers (empty when no paths).
        let path_vbuf = (!path_geo.vertices.is_empty()).then(|| {
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("path-verts"),
                contents: bytemuck_lite::cast_slice(&path_geo.vertices),
                usage: wgpu::BufferUsages::VERTEX,
            })
        });
        let path_ibuf = (!path_geo.indices.is_empty()).then(|| {
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("path-idx"),
                contents: bytemuck_lite::cast_slice(&path_geo.indices),
                usage: wgpu::BufferUsages::INDEX,
            })
        });

        // Per-gradient instance buffers, created up front so they outlive the pass.
        let gradient_buffers: Vec<wgpu::Buffer> = gradients
            .iter()
            .map(|g| {
                device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("grad-instance"),
                    contents: bytemuck_lite::bytes_of(&g.instance),
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
                    view: attach_view,
                    resolve_target,
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
            if !gradients.is_empty() {
                pass.set_pipeline(&self.gradient_pipeline);
                pass.set_bind_group(0, &viewport_bg, &[]);
                for (g, buf) in gradients.iter().zip(&gradient_buffers) {
                    pass.set_bind_group(1, &g.bind, &[]);
                    pass.set_vertex_buffer(0, buf.slice(..));
                    pass.draw(0..6, 0..1);
                }
            }
            if let (Some(vbuf), Some(ibuf)) = (&path_vbuf, &path_ibuf) {
                pass.set_pipeline(&self.path_pipeline);
                pass.set_bind_group(0, &viewport_bg, &[]);
                pass.set_vertex_buffer(0, vbuf.slice(..));
                pass.set_index_buffer(ibuf.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..path_geo.indices.len() as u32, 0, 0..1);
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

    /// Bake a gradient's stops into a 1-D ramp texture (Oklab, shared with the
    /// CPU sampler) and bind it with the linear ramp sampler.
    fn upload_ramp(&self, stops: &[crate::display_list::GradientStop]) -> wgpu::BindGroup {
        let texels = crate::gradient::bake_ramp(stops, RAMP_TEXELS);
        let size = wgpu::Extent3d {
            width: RAMP_TEXELS,
            height: 1,
            depth_or_array_layers: 1,
        };
        let tex = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("ramp"),
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
            &texels,
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(RAMP_TEXELS * 4),
                rows_per_image: Some(1),
            },
            size,
        );
        let view = tex.create_view(&Default::default());
        self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ramp-bg"),
            layout: &self.image_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.ramp_sampler),
                },
            ],
        })
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

/// Accumulated triangle geometry for all `DrawCmd::Path`s in a frame (R1.3).
#[derive(Default)]
struct PathGeometry {
    vertices: Vec<PathVertex>,
    indices: Vec<u32>,
}

impl PathGeometry {
    /// Tessellate one kurbo path (filled or stroked) into the shared buffers.
    fn add(&mut self, path: &kurbo::BezPath, color: [f32; 4], style: FillOrStroke) {
        use lyon::tessellation::{
            BuffersBuilder, FillOptions, FillTessellator, FillVertex, StrokeOptions,
            StrokeTessellator, StrokeVertex, VertexBuffers,
        };
        let lp = to_lyon_path(path);
        let mut buf: VertexBuffers<PathVertex, u32> = VertexBuffers::new();
        let ok = match style {
            FillOrStroke::Fill => {
                let opts = FillOptions::tolerance(0.05)
                    .with_fill_rule(lyon::tessellation::FillRule::NonZero);
                FillTessellator::new()
                    .tessellate_path(
                        &lp,
                        &opts,
                        &mut BuffersBuilder::new(&mut buf, |v: FillVertex| PathVertex {
                            pos: [v.position().x, v.position().y],
                            color,
                        }),
                    )
                    .is_ok()
            }
            FillOrStroke::Stroke { width } => {
                // tiny-skia defaults: butt caps, miter joins, miter limit 4.
                let opts = StrokeOptions::tolerance(0.05).with_line_width(width as f32);
                StrokeTessellator::new()
                    .tessellate_path(
                        &lp,
                        &opts,
                        &mut BuffersBuilder::new(&mut buf, |v: StrokeVertex| PathVertex {
                            pos: [v.position().x, v.position().y],
                            color,
                        }),
                    )
                    .is_ok()
            }
        };
        if !ok {
            return;
        }
        let base = self.vertices.len() as u32;
        self.vertices.extend_from_slice(&buf.vertices);
        self.indices.extend(buf.indices.iter().map(|i| base + i));
    }
}

/// Build a [`GradInstance`] (and borrow the stops to bake) from a gradient brush
/// filling `rect`. Returns `None` for a solid brush.
fn grad_instance<'a>(
    rect: &kurbo::Rect,
    brush: &'a Brush,
) -> Option<(GradInstance, &'a [crate::display_list::GradientStop])> {
    let r = [
        rect.x0 as f32,
        rect.y0 as f32,
        rect.width() as f32,
        rect.height() as f32,
    ];
    let spread = |s: crate::display_list::SpreadMode| match s {
        crate::display_list::SpreadMode::Pad => 0.0,
        crate::display_list::SpreadMode::Repeat => 1.0,
        crate::display_list::SpreadMode::Reflect => 2.0,
    };
    match brush {
        Brush::Solid(_) => None,
        Brush::LinearGradient {
            start,
            end,
            stops,
            spread: sp,
        } => Some((
            GradInstance {
                rect: r,
                g0: [start.x as f32, start.y as f32, end.x as f32, end.y as f32],
                meta: [0.0, spread(*sp), 0.0, 0.0],
            },
            stops,
        )),
        Brush::RadialGradient {
            center,
            radius,
            stops,
            spread: sp,
        } => Some((
            GradInstance {
                rect: r,
                g0: [center.x as f32, center.y as f32, *radius as f32, 0.0],
                meta: [1.0, spread(*sp), 0.0, 0.0],
            },
            stops,
        )),
        Brush::ConicGradient {
            center,
            start_angle,
            stops,
        } => Some((
            GradInstance {
                rect: r,
                g0: [center.x as f32, center.y as f32, *start_angle as f32, 0.0],
                meta: [2.0, 0.0, 0.0, 0.0],
            },
            stops,
        )),
    }
}

/// Convert a kurbo `BezPath` to a lyon `Path`, pairing begin/end per subpath.
fn to_lyon_path(path: &kurbo::BezPath) -> lyon::path::Path {
    use kurbo::PathEl;
    use lyon::geom::point;
    let mut b = lyon::path::Path::builder();
    let mut open = false;
    for el in path.elements() {
        match el {
            PathEl::MoveTo(p) => {
                if open {
                    b.end(false);
                }
                b.begin(point(p.x as f32, p.y as f32));
                open = true;
            }
            PathEl::LineTo(p) => {
                b.line_to(point(p.x as f32, p.y as f32));
            }
            PathEl::QuadTo(c, p) => {
                b.quadratic_bezier_to(point(c.x as f32, c.y as f32), point(p.x as f32, p.y as f32));
            }
            PathEl::CurveTo(c1, c2, p) => {
                b.cubic_bezier_to(
                    point(c1.x as f32, c1.y as f32),
                    point(c2.x as f32, c2.y as f32),
                    point(p.x as f32, p.y as f32),
                );
            }
            PathEl::ClosePath => {
                if open {
                    b.close();
                    open = false;
                }
            }
        }
    }
    if open {
        b.end(false);
    }
    b.build()
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

fn to_ndc(px: vec2<f32>) -> vec4<f32> {
    let ndc = vec2<f32>(px.x / viewport.size.x * 2.0 - 1.0,
                        1.0 - px.y / viewport.size.y * 2.0);
    return vec4<f32>(ndc, 0.0, 1.0);
}

// --- rounded-rect SDF fill + centered border (R1.2) -------------------------

struct RectVsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) wpx: vec2<f32>,
    @location(2) center: vec2<f32>,
    @location(3) half: vec2<f32>,
    @location(4) radii: vec4<f32>,
    @location(5) bcolor: vec4<f32>,
    @location(6) bwidth: f32,
};

@vertex
fn rect_vs(@builtin(vertex_index) vi: u32,
           @location(0) rect: vec4<f32>,
           @location(1) color: vec4<f32>,
           @location(2) radii: vec4<f32>,
           @location(3) bcolor: vec4<f32>,
           @location(4) misc: vec4<f32>) -> RectVsOut {
    let c = corner(vi);
    let bw = misc.x;
    // Inflate the quad so the AA falloff and the outer half of a centered
    // border (which straddles the path edge) are inside the rasterized area.
    let margin = bw * 0.5 + 1.5;
    let origin = rect.xy - vec2<f32>(margin, margin);
    let size = rect.zw + vec2<f32>(margin * 2.0, margin * 2.0);
    let px = origin + c * size;
    var o: RectVsOut;
    o.pos = to_ndc(px);
    o.color = color;
    o.wpx = px;
    o.center = rect.xy + rect.zw * 0.5;
    o.half = rect.zw * 0.5;
    o.radii = radii;
    o.bcolor = bcolor;
    o.bwidth = bw;
    return o;
}

// Signed distance to a rounded box with per-corner radii. `p` is relative to the
// box center; `b` is the half-size; radii order is (tl, tr, br, bl).
fn sd_round_box(p: vec2<f32>, b: vec2<f32>, radii: vec4<f32>) -> f32 {
    let rmax = min(b.x, b.y);
    // Pick the corner radius for this quadrant (y is downward).
    var r: f32;
    if (p.x > 0.0) { r = select(radii.w, radii.z, p.y > 0.0); }   // tr / br
    else           { r = select(radii.x, radii.y, p.y > 0.0); }   // tl / bl
    r = clamp(r, 0.0, rmax);
    let q = abs(p) - b + vec2<f32>(r, r);
    return min(max(q.x, q.y), 0.0) + length(max(q, vec2<f32>(0.0, 0.0))) - r;
}

// Straight-alpha source-over of `src` onto `dst`.
fn over(src: vec4<f32>, dst: vec4<f32>) -> vec4<f32> {
    let a = src.a + dst.a * (1.0 - src.a);
    if (a <= 0.0) { return vec4<f32>(0.0); }
    let rgb = (src.rgb * src.a + dst.rgb * dst.a * (1.0 - src.a)) / a;
    return vec4<f32>(rgb, a);
}

@fragment
fn rect_fs(in: RectVsOut) -> @location(0) vec4<f32> {
    let sd = sd_round_box(in.wpx - in.center, in.half, in.radii);
    // Fill covers the path interior (sd < 0), AA over a 1px ramp.
    let fill_cov = clamp(0.5 - sd, 0.0, 1.0);
    var col = vec4<f32>(in.color.rgb, in.color.a * fill_cov);
    if (in.bwidth > 0.0) {
        // Centered stroke: a band of width bwidth straddling sd == 0.
        let half_bw = in.bwidth * 0.5;
        let stroke_cov = clamp(0.5 - (abs(sd) - half_bw), 0.0, 1.0);
        let stroke = vec4<f32>(in.bcolor.rgb, in.bcolor.a * stroke_cov);
        col = over(stroke, col);
    }
    if (col.a <= 0.0) { discard; }
    return col;
}

@group(1) @binding(0) var img_tex: texture_2d<f32>;
@group(1) @binding(1) var img_samp: sampler;

@vertex
fn image_vs(@builtin(vertex_index) vi: u32,
            @location(0) rect: vec4<f32>,
            @location(1) color: vec4<f32>) -> VsOut {
    let c = corner(vi);
    var o: VsOut;
    o.pos = to_ndc(rect.xy + c * rect.zw);
    o.color = color;
    o.uv = c;
    return o;
}

@fragment
fn image_fs(in: VsOut) -> @location(0) vec4<f32> {
    return textureSample(img_tex, img_samp, in.uv) * in.color;
}

// --- tessellated paths (R1.3) -----------------------------------------------

struct PathVsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) color: vec4<f32>,
};

@vertex
fn path_vs(@location(0) p: vec2<f32>, @location(1) color: vec4<f32>) -> PathVsOut {
    var o: PathVsOut;
    o.pos = to_ndc(p);
    o.color = color;
    return o;
}

@fragment
fn path_fs(in: PathVsOut) -> @location(0) vec4<f32> {
    return in.color;
}

// --- gradients (R1.4) -------------------------------------------------------
// The ramp texture (Oklab-baked on the CPU) is bound in group 1; the fragment
// computes the spatial parameter t and samples it.

struct GradVsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) wpx: vec2<f32>,
    @location(1) g0: vec4<f32>,
    @location(2) gmeta: vec4<f32>,
};

@vertex
fn gradient_vs(@builtin(vertex_index) vi: u32,
               @location(0) rect: vec4<f32>,
               @location(1) g0: vec4<f32>,
               @location(2) gmeta: vec4<f32>) -> GradVsOut {
    let c = corner(vi);
    let px = rect.xy + c * rect.zw;
    var o: GradVsOut;
    o.pos = to_ndc(px);
    o.wpx = px;
    o.g0 = g0;
    o.gmeta = gmeta;
    return o;
}

fn apply_spread(t: f32, spread: f32) -> f32 {
    if (spread < 0.5) {            // pad
        return clamp(t, 0.0, 1.0);
    } else if (spread < 1.5) {     // repeat
        return fract(t);
    } else {                       // reflect
        let m = t - 2.0 * floor(t * 0.5);
        return select(m, 2.0 - m, m > 1.0);
    }
}

@fragment
fn gradient_fs(in: GradVsOut) -> @location(0) vec4<f32> {
    let kind = in.gmeta.x;
    var t: f32;
    if (kind < 0.5) {                       // linear
        let d = in.g0.zw - in.g0.xy;
        t = dot(in.wpx - in.g0.xy, d) / max(dot(d, d), 1e-6);
        t = apply_spread(t, in.gmeta.y);
    } else if (kind < 1.5) {                // radial
        t = length(in.wpx - in.g0.xy) / max(in.g0.z, 1e-6);
        t = apply_spread(t, in.gmeta.y);
    } else {                                // conic
        let a = atan2(in.wpx.y - in.g0.y, in.wpx.x - in.g0.x) - in.g0.z;
        t = fract(a / 6.283185307179586);
    }
    return textureSample(img_tex, img_samp, vec2<f32>(clamp(t, 0.0, 1.0), 0.5));
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
