//! The GPU backend (wgpu, ADR-001), offscreen.
//!
//! Renders the display list to an offscreen texture and reads it back to an
//! [`RgbaImage`], with no window or display required. Supported commands:
//! solid-fill [`DrawCmd::Rect`] — square or rounded, with an optional centered
//! border, via a rounded-box SDF with 1px analytic AA (R1.2); solid-fill/stroke
//! [`DrawCmd::Path`] tessellated by `lyon` with MSAA edge AA (R1.3);
//! gradient-filled rects (linear/radial/conic) via an Oklab ramp texture (R1.4);
//! [`DrawCmd::PushLayer`]/[`DrawCmd::PopLayer`] via recursive render-to-texture
//! compositing with rounded-rect clip + group opacity (R1.5);
//! [`DrawCmd::BackdropFilter`] (glass) via a 3-box blur + saturated rounded
//! composite (the layer pass is split to read prior content); and
//! [`DrawCmd::Image`] blits (nearest or bilinear), all honoring a HiDPI `scale`
//! (R1.6). Draws within a layer follow display-list order. Glyph runs are R3.
//!
//! The target is `Rgba8UnormSrgb`, so the GPU composites in **linear light** (the
//! hardware decodes on read, blends, and encodes on write) — the physically
//! correct blend, and what the live-window agent reads back. The deterministic
//! `TinySkia` reference blends in **gamma**, so GPU and CPU agree on opaque,
//! non-AA, nearest-sampled content and *intentionally* differ on blended /
//! anti-aliased pixels. `tests/cpu_vs_gpu` asserts exact parity for the former
//! and treats the latter as informational; see the decision log.

use crate::display_list::{Brush, CornerRadii, DisplayList, DrawCmd, FillOrStroke, Filter};
use crate::image::RgbaImage;
use lumen_core::Color;
use std::borrow::Cow;

/// A headless wgpu renderer.
pub struct Wgpu {
    device: wgpu::Device,
    queue: wgpu::Queue,
    rect_pipeline: wgpu::RenderPipeline,
    image_pipeline: wgpu::RenderPipeline,
    path_pipeline: wgpu::RenderPipeline,
    gradient_pipeline: wgpu::RenderPipeline,
    composite_pipeline: wgpu::RenderPipeline,
    /// 1-D box-blur pass (group0 = params, group1 = source texture). R1 backdrop.
    blur_pipeline: wgpu::RenderPipeline,
    blur_params_bgl: wgpu::BindGroupLayout,
    /// Composites a blurred backdrop within a rounded clip, with saturation.
    backdrop_pipeline: wgpu::RenderPipeline,
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
    /// Corner radii `[tl, tr, br, bl]` — the fill is clipped to the rounded rect.
    radii: [f32; 4],
}

/// Texels per gradient ramp texture (1-D). Dense enough that linear filtering
/// reproduces the CPU's Oklab ramp within the gradient tolerance.
const RAMP_TEXELS: u32 = 512;

/// A layer-composite instance (R1.5): draws a child layer's texture over the
/// parent, masked to a rounded-rect clip and scaled by group opacity.
#[repr(C)]
#[derive(Clone, Copy)]
struct CompositeInstance {
    /// Clip rect `[x0, y0, w, h]` (the composite quad); full frame if no clip.
    rect: [f32; 4],
    /// Clip corner radii `[tl, tr, br, bl]`.
    radii: [f32; 4],
    /// `[opacity, has_clip (0/1), _, _]`.
    params: [f32; 4],
}

/// GPU resources that must outlive `queue.submit` (textures referenced by the
/// recorded command buffer). Dropped after submit returns.
#[derive(Default)]
struct KeepAlive {
    textures: Vec<wgpu::Texture>,
    views: Vec<wgpu::TextureView>,
    buffers: Vec<wgpu::Buffer>,
    binds: Vec<wgpu::BindGroup>,
}

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

/// One ordered draw within a layer (R1: draws are emitted in display-list order,
/// batching only *consecutive* solid rects and *consecutive* paths, so a rect
/// authored after an image paints on top of it).
enum LayerDraw {
    Rects {
        buf: wgpu::Buffer,
        count: u32,
    },
    Paths {
        vbuf: wgpu::Buffer,
        ibuf: wgpu::Buffer,
        indices: u32,
    },
    Gradient {
        buf: wgpu::Buffer,
        bind: wgpu::BindGroup,
    },
    Image {
        buf: wgpu::Buffer,
        bind: wgpu::BindGroup,
    },
    Composite {
        buf: wgpu::Buffer,
        bind: wgpu::BindGroup,
    },
    /// Glass `backdrop-filter`: blur everything drawn so far within the rounded
    /// region and composite it back (R1). Handled by splitting the layer pass.
    Backdrop {
        rect: kurbo::Rect,
        radii: CornerRadii,
        blur: f32,
        saturate: f32,
    },
}

/// Flush the pending run of consecutive solid rects into an ordered op.
fn flush_rects(device: &wgpu::Device, ops: &mut Vec<LayerDraw>, pend: &mut Vec<RectInstance>) {
    use wgpu::util::DeviceExt;
    if pend.is_empty() {
        return;
    }
    let buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("rects"),
        contents: bytemuck_lite::cast_slice(pend),
        usage: wgpu::BufferUsages::VERTEX,
    });
    ops.push(LayerDraw::Rects {
        buf,
        count: pend.len() as u32,
    });
    pend.clear();
}

/// Flush the pending run of consecutive tessellated paths into an ordered op.
fn flush_paths(device: &wgpu::Device, ops: &mut Vec<LayerDraw>, pend: &mut PathGeometry) {
    use wgpu::util::DeviceExt;
    if pend.vertices.is_empty() {
        return;
    }
    let vbuf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("path-verts"),
        contents: bytemuck_lite::cast_slice(&pend.vertices),
        usage: wgpu::BufferUsages::VERTEX,
    });
    let ibuf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("path-idx"),
        contents: bytemuck_lite::cast_slice(&pend.indices),
        usage: wgpu::BufferUsages::INDEX,
    });
    ops.push(LayerDraw::Paths {
        vbuf,
        ibuf,
        indices: pend.indices.len() as u32,
    });
    *pend = PathGeometry::default();
}

// An sRGB target: the hardware decodes on read, blends in **linear light**, and
// encodes on write — the physically-correct compositing the GPU is built for.
// Solid fragments output linear color (the hardware encodes); image/ramp/child
// textures are sRGB so sampling decodes to linear. Readback returns the stored
// sRGB bytes (RGBA, no swizzle) — exactly what the live-window agent sees. The
// CPU reference stays gamma-space, so GPU and CPU agree on opaque, non-AA content
// and *intentionally* differ on blended/anti-aliased pixels (linear vs gamma).
const TARGET_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;

/// The GPU backend as a runtime-selectable [`Renderer`](crate::Renderer) (A1).
/// Covers the command set the offscreen backend supports (solid rects — square
/// or rounded with a centered border — and image blits, which include
/// rasterized text/shadow sprites); paths/gradients/layers and HiDPI scaling on
/// the GPU are follow-on, so it renders at 1:1.
impl crate::Renderer for Wgpu {
    fn render_frame(
        &mut self,
        list: &DisplayList,
        width: u32,
        height: u32,
        scale: f64,
        background: Color,
    ) -> RgbaImage {
        self.render_at_scale(list, width, height, scale, background)
    }

    fn name(&self) -> &'static str {
        "gpu"
    }
}

impl Wgpu {
    /// Create a headless renderer, or `None` if no adapter is available.
    pub fn new() -> Option<Wgpu> {
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
                // FRAGMENT too: the composite shader reads viewport.size (R1.5).
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
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
                    attributes: &wgpu::vertex_attr_array![
                        0 => Float32x4, 1 => Float32x4, 2 => Float32x4, 3 => Float32x4
                    ],
                }],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "gradient_fs",
                targets: &[Some(target.clone())],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample,
            multiview: None,
            cache: None,
        });

        // Layer-composite pipeline (R1.5): child texture + rounded clip + opacity.
        let composite_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("composite-layout"),
            bind_group_layouts: &[&viewport_bgl, &image_bgl],
            push_constant_ranges: &[],
        });
        let composite_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("composite"),
            layout: Some(&composite_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "composite_vs",
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<CompositeInstance>() as u64,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32x4, 2 => Float32x4],
                }],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                // Child layers are stored premultiplied (alpha-blended over a
                // transparent clear), so composite with premultiplied src-over.
                entry_point: "composite_fs",
                targets: &[Some(wgpu::ColorTargetState {
                    format: TARGET_FORMAT,
                    blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample,
            multiview: None,
            cache: None,
        });

        // Box-blur pipeline (R1 backdrop): fullscreen pass averaging 2r+1 texels
        // along one axis. group0 = params (dir+radius), group1 = source texture.
        let blur_params_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("blur-params"),
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
        let blur_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("blur-layout"),
            bind_group_layouts: &[&blur_params_bgl, &image_bgl],
            push_constant_ranges: &[],
        });
        let blur_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("blur"),
            layout: Some(&blur_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "fullscreen_vs",
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "blur_fs",
                // Renders into a single-sample ping-pong target, overwriting.
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

        // Backdrop-composite pipeline: draws the blurred backdrop within a rounded
        // clip (+saturation) into the (possibly MSAA) layer attachment.
        let backdrop_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("backdrop-layout"),
            bind_group_layouts: &[&viewport_bgl, &image_bgl],
            push_constant_ranges: &[],
        });
        let backdrop_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("backdrop"),
            layout: Some(&backdrop_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "backdrop_vs",
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<CompositeInstance>() as u64,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32x4, 2 => Float32x4],
                }],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "backdrop_fs",
                targets: &[Some(wgpu::ColorTargetState {
                    format: TARGET_FORMAT,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
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

        Some(Wgpu {
            device,
            queue,
            rect_pipeline,
            image_pipeline,
            path_pipeline,
            gradient_pipeline,
            composite_pipeline,
            blur_pipeline,
            blur_params_bgl,
            backdrop_pipeline,
            image_bgl,
            sampler,
            ramp_sampler,
            sample_count,
        })
    }

    /// Render `list` (logical-px) to a `width`×`height` *physical* image over
    /// `background`, at 1:1 scale.
    pub fn render(
        &self,
        list: &DisplayList,
        width: u32,
        height: u32,
        background: Color,
    ) -> RgbaImage {
        self.render_at_scale(list, width, height, 1.0, background)
    }

    /// Render `list` (logical-px) to a `width`×`height` *physical* image,
    /// scaling logical coordinates by `scale` (HiDPI, R1.6).
    pub fn render_at_scale(
        &self,
        list: &DisplayList,
        width: u32,
        height: u32,
        scale: f64,
        background: Color,
    ) -> RgbaImage {
        use wgpu::util::DeviceExt;
        let device = &self.device;
        let mut keep = KeepAlive::default();
        let mut encoder = device.create_command_encoder(&Default::default());

        // Shared viewport uniform: physical size + the logical→physical scale.
        let viewport = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("viewport"),
            contents: bytemuck_lite::bytes_of(&[width as f32, height as f32, scale as f32, 0.0]),
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

        // The whole list is the root layer, cleared to the opaque background.
        let root = self.encode_layer(
            device,
            &mut encoder,
            &mut keep,
            &viewport_bg,
            width,
            height,
            &list.cmds,
            list,
            Some(background),
            scale,
        );

        // Readback from the resolved root texture.
        let bpr = padded_bytes_per_row(width);
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("readback"),
            size: (bpr * height) as u64,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        encoder.copy_texture_to_buffer(
            wgpu::ImageCopyTexture {
                texture: &root,
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
        drop(keep);

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

    /// Render one layer (a slice of commands) into its own resolved single-sample
    /// texture (R1.5). Nested `PushLayer`/`PopLayer` spans recurse first (their
    /// passes are recorded into `encoder` before this layer's), then composite
    /// over this layer with a rounded-rect clip + group opacity.
    ///
    /// `clear` is the opaque background for the root; child layers clear to
    /// transparent. Resources are parked in `keep` so they outlive submit.
    #[allow(clippy::too_many_arguments)]
    fn encode_layer(
        &self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        keep: &mut KeepAlive,
        viewport_bg: &wgpu::BindGroup,
        width: u32,
        height: u32,
        cmds: &[DrawCmd],
        list: &DisplayList,
        clear: Option<Color>,
        scale: f64,
    ) -> wgpu::Texture {
        use wgpu::util::DeviceExt;

        // --- collect ordered draw ops (display-list order within the layer) -
        let mut ops: Vec<LayerDraw> = Vec::new();
        let mut pend_rects: Vec<RectInstance> = Vec::new();
        let mut pend_paths = PathGeometry::default();

        let mut i = 0;
        while i < cmds.len() {
            match &cmds[i] {
                DrawCmd::PushLayer { clip, opacity, .. } => {
                    flush_rects(device, &mut ops, &mut pend_rects);
                    flush_paths(device, &mut ops, &mut pend_paths);
                    // Find the matching PopLayer (accounting for nesting).
                    let start = i + 1;
                    let mut depth = 1;
                    let mut j = start;
                    while j < cmds.len() {
                        match &cmds[j] {
                            DrawCmd::PushLayer { .. } => depth += 1,
                            DrawCmd::PopLayer => {
                                depth -= 1;
                                if depth == 0 {
                                    break;
                                }
                            }
                            _ => {}
                        }
                        j += 1;
                    }
                    let inner = &cmds[start..j.min(cmds.len())];
                    let child = self.encode_layer(
                        device,
                        encoder,
                        keep,
                        viewport_bg,
                        width,
                        height,
                        inner,
                        list,
                        None,
                        scale,
                    );
                    let child_view = child.create_view(&Default::default());
                    let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
                        label: Some("composite-bg"),
                        layout: &self.image_bgl,
                        entries: &[
                            wgpu::BindGroupEntry {
                                binding: 0,
                                resource: wgpu::BindingResource::TextureView(&child_view),
                            },
                            wgpu::BindGroupEntry {
                                binding: 1,
                                resource: wgpu::BindingResource::Sampler(&self.sampler),
                            },
                        ],
                    });
                    let (rect, radii, has_clip) = match clip {
                        Some(rr) => (
                            [
                                rr.rect.x0 as f32,
                                rr.rect.y0 as f32,
                                rr.rect.width() as f32,
                                rr.rect.height() as f32,
                            ],
                            [
                                rr.radii.tl as f32,
                                rr.radii.tr as f32,
                                rr.radii.br as f32,
                                rr.radii.bl as f32,
                            ],
                            1.0,
                        ),
                        None => ([0.0, 0.0, width as f32, height as f32], [0.0; 4], 0.0),
                    };
                    let buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("composite-instance"),
                        contents: bytemuck_lite::bytes_of(&CompositeInstance {
                            rect,
                            radii,
                            params: [*opacity, has_clip, 0.0, 0.0],
                        }),
                        usage: wgpu::BufferUsages::VERTEX,
                    });
                    ops.push(LayerDraw::Composite { buf, bind });
                    keep.textures.push(child);
                    keep.views.push(child_view);
                    i = j + 1;
                }
                DrawCmd::PopLayer => i += 1, // unmatched; ignore
                DrawCmd::Path {
                    path,
                    brush: Brush::Solid(c),
                    style,
                } => {
                    flush_rects(device, &mut ops, &mut pend_rects);
                    pend_paths.add(path, [c.r, c.g, c.b, c.a], *style);
                    i += 1;
                }
                DrawCmd::Rect {
                    rect,
                    brush:
                        brush @ (Brush::LinearGradient { .. }
                        | Brush::RadialGradient { .. }
                        | Brush::ConicGradient { .. }),
                    radii,
                    ..
                } => {
                    flush_rects(device, &mut ops, &mut pend_rects);
                    flush_paths(device, &mut ops, &mut pend_paths);
                    if let Some((instance, stops)) = grad_instance(rect, radii, brush) {
                        let bind = self.upload_ramp(stops);
                        let buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: Some("grad-instance"),
                            contents: bytemuck_lite::bytes_of(&instance),
                            usage: wgpu::BufferUsages::VERTEX,
                        });
                        ops.push(LayerDraw::Gradient { buf, bind });
                    }
                    i += 1;
                }
                DrawCmd::Rect {
                    rect,
                    brush: Brush::Solid(c),
                    radii,
                    border,
                } => {
                    flush_paths(device, &mut ops, &mut pend_paths);
                    let (bcolor, bw) = match border {
                        Some(b) => ([b.color.r, b.color.g, b.color.b, b.color.a], b.width as f32),
                        None => ([0.0; 4], 0.0),
                    };
                    pend_rects.push(RectInstance {
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
                    i += 1;
                }
                DrawCmd::Image {
                    id,
                    dst_rect,
                    quality,
                    ..
                } => {
                    if let Some(img) = list.images.get(id.0 as usize) {
                        flush_rects(device, &mut ops, &mut pend_rects);
                        flush_paths(device, &mut ops, &mut pend_paths);
                        let bind = self.upload_image(img, *quality);
                        let buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: Some("img-instance"),
                            contents: bytemuck_lite::bytes_of(&RectInstance::plain(
                                [
                                    dst_rect.x0 as f32,
                                    dst_rect.y0 as f32,
                                    dst_rect.width() as f32,
                                    dst_rect.height() as f32,
                                ],
                                [1.0, 1.0, 1.0, 1.0],
                            )),
                            usage: wgpu::BufferUsages::VERTEX,
                        });
                        ops.push(LayerDraw::Image { buf, bind });
                    }
                    i += 1;
                }
                // CPU draws a Shader as its deterministic fallback solid rect.
                DrawCmd::Shader { rect, uniforms, .. } => {
                    flush_paths(device, &mut ops, &mut pend_paths);
                    let c = uniforms.fallback;
                    pend_rects.push(RectInstance::plain(
                        [
                            rect.x0 as f32,
                            rect.y0 as f32,
                            rect.width() as f32,
                            rect.height() as f32,
                        ],
                        [c.r, c.g, c.b, c.a],
                    ));
                    i += 1;
                }
                DrawCmd::BackdropFilter {
                    rect,
                    radii,
                    blur,
                    saturate,
                } => {
                    flush_rects(device, &mut ops, &mut pend_rects);
                    flush_paths(device, &mut ops, &mut pend_paths);
                    ops.push(LayerDraw::Backdrop {
                        rect: *rect,
                        radii: *radii,
                        blur: *blur,
                        saturate: *saturate,
                    });
                    i += 1;
                }
                // GlyphRun (R3): not on the GPU yet.
                _ => i += 1,
            }
        }
        flush_rects(device, &mut ops, &mut pend_rects);
        flush_paths(device, &mut ops, &mut pend_paths);
        let has_backdrop = ops
            .iter()
            .any(|op| matches!(op, LayerDraw::Backdrop { .. }));

        // --- this layer's target. MSAA (when available) gives tessellated
        // paths their edge AA; the single-sample `resolved` is the resolve target
        // (and what a parent composite / backdrop blur samples).
        let resolved = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("layer"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: TARGET_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let resolved_view = resolved.create_view(&Default::default());
        let msaa_tex = (self.sample_count > 1).then(|| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some("layer-msaa"),
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
        // Each pass renders into the MSAA attachment (when present) and resolves
        // into `resolved`; without MSAA it renders straight into `resolved`.
        let (attach_view, resolve_target): (&wgpu::TextureView, Option<&wgpu::TextureView>) =
            match &msaa_view {
                Some(v) => (v, Some(&resolved_view)),
                None => (&resolved_view, None),
            };

        let c = clear.unwrap_or(Color::TRANSPARENT);
        let clear_color = wgpu::Color {
            r: c.r as f64,
            g: c.g as f64,
            b: c.b as f64,
            a: c.a as f64,
        };

        if !has_backdrop {
            // Fast path: a single pass for the whole layer.
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("layer-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: attach_view,
                    resolve_target,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(clear_color),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            for op in &ops {
                self.draw_op(&mut pass, viewport_bg, op);
            }
        } else {
            // Backdrop path: split into segments at each Backdrop op. Render a
            // segment (resolving into `resolved`), blur the resolved content, then
            // start the next segment by compositing that blurred backdrop in.
            let n = ops.len();
            let mut seg_start = 0usize;
            let mut first = true;
            // Composite resources for a backdrop, drawn at the start of the next
            // segment (its blurred backdrop was prepared from the prior resolve).
            let mut pending: Option<(wgpu::BindGroup, wgpu::Buffer)> = None;
            let mut k = 0usize;
            loop {
                let at_backdrop = k < n && matches!(ops[k], LayerDraw::Backdrop { .. });
                if at_backdrop || k == n {
                    {
                        let load = if first {
                            wgpu::LoadOp::Clear(clear_color)
                        } else {
                            wgpu::LoadOp::Load
                        };
                        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                            label: Some("layer-seg"),
                            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                                view: attach_view,
                                resolve_target,
                                ops: wgpu::Operations {
                                    load,
                                    store: wgpu::StoreOp::Store,
                                },
                            })],
                            depth_stencil_attachment: None,
                            timestamp_writes: None,
                            occlusion_query_set: None,
                        });
                        // Composite the previous backdrop's blurred content first.
                        if let Some((bind, buf)) = &pending {
                            pass.set_pipeline(&self.backdrop_pipeline);
                            pass.set_bind_group(0, viewport_bg, &[]);
                            pass.set_bind_group(1, bind, &[]);
                            pass.set_vertex_buffer(0, buf.slice(..));
                            pass.draw(0..6, 0..1);
                        }
                        for op in &ops[seg_start..k] {
                            self.draw_op(&mut pass, viewport_bg, op);
                        }
                    }
                    if let Some((bind, buf)) = pending.take() {
                        keep.binds.push(bind);
                        keep.buffers.push(buf);
                    }
                    first = false;
                    if k == n {
                        break;
                    }
                    if let LayerDraw::Backdrop {
                        rect,
                        radii,
                        blur,
                        saturate,
                    } = &ops[k]
                    {
                        // Prepare the blurred-backdrop composite from the content
                        // resolved so far; it is drawn at the next segment's start.
                        pending = Some(self.prepare_backdrop(
                            device, encoder, keep, &resolved, width, height, scale, *rect, *radii,
                            *blur, *saturate,
                        ));
                    }
                    seg_start = k + 1;
                }
                k += 1;
            }
        }

        // Park everything the recorded passes referenced until after submit.
        for op in ops {
            match op {
                LayerDraw::Rects { buf, .. } => keep.buffers.push(buf),
                LayerDraw::Paths { vbuf, ibuf, .. } => {
                    keep.buffers.push(vbuf);
                    keep.buffers.push(ibuf);
                }
                LayerDraw::Gradient { buf, bind }
                | LayerDraw::Image { buf, bind }
                | LayerDraw::Composite { buf, bind } => {
                    keep.buffers.push(buf);
                    keep.binds.push(bind);
                }
                LayerDraw::Backdrop { .. } => {}
            }
        }
        if let Some(t) = msaa_tex {
            keep.textures.push(t);
        }
        keep.views.extend(msaa_view);
        keep.views.push(resolved_view);
        resolved
    }

    /// Issue one ordered draw into `pass` (R1 — display-list order). `Backdrop`
    /// is handled by pass-splitting, not here.
    fn draw_op<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        viewport_bg: &'a wgpu::BindGroup,
        op: &'a LayerDraw,
    ) {
        match op {
            LayerDraw::Rects { buf, count } => {
                pass.set_pipeline(&self.rect_pipeline);
                pass.set_bind_group(0, viewport_bg, &[]);
                pass.set_vertex_buffer(0, buf.slice(..));
                pass.draw(0..6, 0..*count);
            }
            LayerDraw::Paths {
                vbuf,
                ibuf,
                indices,
            } => {
                pass.set_pipeline(&self.path_pipeline);
                pass.set_bind_group(0, viewport_bg, &[]);
                pass.set_vertex_buffer(0, vbuf.slice(..));
                pass.set_index_buffer(ibuf.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..*indices, 0, 0..1);
            }
            LayerDraw::Gradient { buf, bind } => {
                pass.set_pipeline(&self.gradient_pipeline);
                pass.set_bind_group(0, viewport_bg, &[]);
                pass.set_bind_group(1, bind, &[]);
                pass.set_vertex_buffer(0, buf.slice(..));
                pass.draw(0..6, 0..1);
            }
            LayerDraw::Image { buf, bind } => {
                pass.set_pipeline(&self.image_pipeline);
                pass.set_bind_group(0, viewport_bg, &[]);
                pass.set_bind_group(1, bind, &[]);
                pass.set_vertex_buffer(0, buf.slice(..));
                pass.draw(0..6, 0..1);
            }
            LayerDraw::Composite { buf, bind } => {
                pass.set_pipeline(&self.composite_pipeline);
                pass.set_bind_group(0, viewport_bg, &[]);
                pass.set_bind_group(1, bind, &[]);
                pass.set_vertex_buffer(0, buf.slice(..));
                pass.draw(0..6, 0..1);
            }
            LayerDraw::Backdrop { .. } => {}
        }
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

    /// Blur the `resolved` content within the backdrop region (3 box passes per
    /// axis, matching `RgbaImage::blurred`) and prepare the composite that draws
    /// it back within the rounded clip with saturation. Returns the composite
    /// bind group + instance buffer (drawn by the caller at the next segment's
    /// start); all blur intermediates are parked in `keep`. (R1 glass.)
    #[allow(clippy::too_many_arguments)]
    fn prepare_backdrop(
        &self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        keep: &mut KeepAlive,
        resolved: &wgpu::Texture,
        width: u32,
        height: u32,
        scale: f64,
        rect: kurbo::Rect,
        radii: CornerRadii,
        blur: f32,
        saturate: f32,
    ) -> (wgpu::BindGroup, wgpu::Buffer) {
        use wgpu::util::DeviceExt;
        let r_px = (blur as f64 * scale).round().max(0.0) as f32;

        let mk_tex = |label| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: TARGET_FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
        };
        let tex_a = mk_tex("blur-a");
        let tex_b = mk_tex("blur-b");
        let view_a = tex_a.create_view(&Default::default());
        let view_b = tex_b.create_view(&Default::default());
        let src_view = resolved.create_view(&Default::default());

        let param = |dir: [f32; 2]| {
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("blur-params"),
                contents: f32s_as_bytes(&[dir[0], dir[1], r_px, 0.0]),
                usage: wgpu::BufferUsages::UNIFORM,
            })
        };
        let h_buf = param([1.0, 0.0]);
        let v_buf = param([0.0, 1.0]);
        let params_bind = |buf: &wgpu::Buffer| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("blur-params-bg"),
                layout: &self.blur_params_bgl,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: buf.as_entire_binding(),
                }],
            })
        };
        let h_params = params_bind(&h_buf);
        let v_params = params_bind(&v_buf);
        let src_bind = |view: &wgpu::TextureView| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("blur-src"),
                layout: &self.image_bgl,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                ],
            })
        };
        let bind_resolved = src_bind(&src_view);
        let bind_a = src_bind(&view_a);
        let bind_b = src_bind(&view_b);

        // 3× (horizontal then vertical) box passes, ping-ponging a↔b, ending in b.
        let mut blur_pass =
            |dst: &wgpu::TextureView, params: &wgpu::BindGroup, src: &wgpu::BindGroup| {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("blur-pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: dst,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });
                pass.set_pipeline(&self.blur_pipeline);
                pass.set_bind_group(0, params, &[]);
                pass.set_bind_group(1, src, &[]);
                pass.draw(0..3, 0..1);
            };
        blur_pass(&view_a, &h_params, &bind_resolved);
        blur_pass(&view_b, &v_params, &bind_a);
        blur_pass(&view_a, &h_params, &bind_b);
        blur_pass(&view_b, &v_params, &bind_a);
        blur_pass(&view_a, &h_params, &bind_b);
        blur_pass(&view_b, &v_params, &bind_a);
        // blurred = tex_b.

        let composite_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("backdrop-bg"),
            layout: &self.image_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view_b),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        });
        let instance = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("backdrop-instance"),
            contents: bytemuck_lite::bytes_of(&CompositeInstance {
                rect: [
                    rect.x0 as f32,
                    rect.y0 as f32,
                    rect.width() as f32,
                    rect.height() as f32,
                ],
                radii: [
                    radii.tl as f32,
                    radii.tr as f32,
                    radii.br as f32,
                    radii.bl as f32,
                ],
                params: [saturate, 0.0, 0.0, 0.0],
            }),
            usage: wgpu::BufferUsages::VERTEX,
        });

        // Park blur intermediates (the recorded passes reference them).
        keep.textures.push(tex_a);
        keep.textures.push(tex_b);
        keep.views.push(view_a);
        keep.views.push(view_b);
        keep.views.push(src_view);
        keep.buffers.push(h_buf);
        keep.buffers.push(v_buf);
        keep.binds.push(h_params);
        keep.binds.push(v_params);
        keep.binds.push(bind_resolved);
        keep.binds.push(bind_a);
        keep.binds.push(bind_b);
        (composite_bind, instance)
    }

    fn upload_image(&self, img: &RgbaImage, quality: Filter) -> wgpu::BindGroup {
        // Nearest for crisp/pixel-art (and cached text/shadow sprites, which are
        // drawn 1:1); bilinear for smoothly-scaled images. Both filter in gamma
        // space (the texture holds sRGB bytes), matching the CPU reference.
        let sampler = match quality {
            Filter::Nearest => &self.sampler,
            Filter::Bilinear => &self.ramp_sampler,
        };
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
                    resource: wgpu::BindingResource::Sampler(sampler),
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
    radii: &crate::display_list::CornerRadii,
    brush: &'a Brush,
) -> Option<(GradInstance, &'a [crate::display_list::GradientStop])> {
    let r = [
        rect.x0 as f32,
        rect.y0 as f32,
        rect.width() as f32,
        rect.height() as f32,
    ];
    let rad = [
        radii.tl as f32,
        radii.tr as f32,
        radii.br as f32,
        radii.bl as f32,
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
                radii: rad,
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
                radii: rad,
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
                radii: rad,
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
struct Viewport { size: vec2<f32>, scale: f32, _pad: f32 };
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
    // Logical → physical (× scale) → NDC.
    let dev = px * viewport.scale;
    let ndc = vec2<f32>(dev.x / viewport.size.x * 2.0 - 1.0,
                        1.0 - dev.y / viewport.size.y * 2.0);
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
    // Fill covers the path interior (sd < 0), AA over a 1px ramp. Colors are
    // linear; the sRGB target encodes on write and blends in linear light.
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
    @location(3) center: vec2<f32>,
    @location(4) half: vec2<f32>,
    @location(5) radii: vec4<f32>,
};

@vertex
fn gradient_vs(@builtin(vertex_index) vi: u32,
               @location(0) rect: vec4<f32>,
               @location(1) g0: vec4<f32>,
               @location(2) gmeta: vec4<f32>,
               @location(3) radii: vec4<f32>) -> GradVsOut {
    let c = corner(vi);
    let px = rect.xy + c * rect.zw;
    var o: GradVsOut;
    o.pos = to_ndc(px);
    o.wpx = px;
    o.g0 = g0;
    o.gmeta = gmeta;
    o.center = rect.xy + rect.zw * 0.5;
    o.half = rect.zw * 0.5;
    o.radii = radii;
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
    let color = textureSample(img_tex, img_samp, vec2<f32>(clamp(t, 0.0, 1.0), 0.5));
    // Clip the fill to the rounded rect. Skip entirely when square so the edges
    // stay crisp (the quad already bounds them); only rounded corners need the
    // SDF coverage (1px AA), which keeps square gradients byte-identical.
    var cov = 1.0;
    if (max(max(in.radii.x, in.radii.y), max(in.radii.z, in.radii.w)) > 0.0) {
        let sd = sd_round_box(in.wpx - in.center, in.half, in.radii);
        cov = clamp(0.5 - sd, 0.0, 1.0);
    }
    return vec4<f32>(color.rgb, color.a * cov);
}

// --- layer compositing (R1.5) -----------------------------------------------
// Samples a child layer (group 1, premultiplied) over the parent, masked to a
// rounded-rect clip and scaled by group opacity.

struct CompVsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) wpx: vec2<f32>,
    @location(1) rect: vec4<f32>,
    @location(2) radii: vec4<f32>,
    @location(3) params: vec4<f32>,
};

@vertex
fn composite_vs(@builtin(vertex_index) vi: u32,
                @location(0) rect: vec4<f32>,
                @location(1) radii: vec4<f32>,
                @location(2) params: vec4<f32>) -> CompVsOut {
    let c = corner(vi);
    let px = rect.xy + c * rect.zw;
    var o: CompVsOut;
    o.pos = to_ndc(px);
    o.wpx = px;
    o.rect = rect;
    o.radii = radii;
    o.params = params;
    return o;
}

@fragment
fn composite_fs(in: CompVsOut) -> @location(0) vec4<f32> {
    // Child stored premultiplied at physical resolution; sample at this pixel.
    let child = textureSample(img_tex, img_samp, in.wpx * viewport.scale / viewport.size);
    var cov = 1.0;
    if (in.params.y > 0.5) {
        let center = in.rect.xy + in.rect.zw * 0.5;
        let half = in.rect.zw * 0.5;
        let sd = sd_round_box(in.wpx - center, half, in.radii);
        cov = clamp(0.5 - sd, 0.0, 1.0);
    }
    let k = cov * in.params.x;          // clip coverage × group opacity
    return child * k;                   // scale premultiplied color + alpha
}

// --- backdrop-filter: box blur + saturated composite (R1 glass) -------------

// Fullscreen triangle; the fragment derives its uv from @builtin(position).
@vertex
fn fullscreen_vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    var p = array<vec2<f32>, 3>(vec2<f32>(-1.0, -1.0), vec2<f32>(3.0, -1.0), vec2<f32>(-1.0, 3.0));
    return vec4<f32>(p[i], 0.0, 1.0);
}

struct BlurP { dir: vec2<f32>, radius: f32, _pad: f32 };
@group(0) @binding(0) var<uniform> bp: BlurP;
@group(1) @binding(0) var blur_tex: texture_2d<f32>;
@group(1) @binding(1) var blur_samp: sampler;

// One axis of a box blur: average the 2*radius+1 texels along `dir` (edges
// clamp via the sampler), matching one CPU box pass.
@fragment
fn blur_fs(@builtin(position) fc: vec4<f32>) -> @location(0) vec4<f32> {
    let dim = vec2<f32>(textureDimensions(blur_tex));
    let uv = fc.xy / dim;
    let r = i32(bp.radius + 0.5);
    var acc = vec4<f32>(0.0);
    var cnt = 0.0;
    for (var k = -r; k <= r; k = k + 1) {
        let o = bp.dir * f32(k) / dim;
        acc = acc + textureSample(blur_tex, blur_samp, uv + o);
        cnt = cnt + 1.0;
    }
    return acc / cnt;
}

@vertex
fn backdrop_vs(@builtin(vertex_index) vi: u32,
               @location(0) rect: vec4<f32>,
               @location(1) radii: vec4<f32>,
               @location(2) params: vec4<f32>) -> CompVsOut {
    let c = corner(vi);
    let px = rect.xy + c * rect.zw;
    var o: CompVsOut;
    o.pos = to_ndc(px);
    o.wpx = px;
    o.rect = rect;
    o.radii = radii;
    o.params = params;
    return o;
}

@fragment
fn backdrop_fs(in: CompVsOut) -> @location(0) vec4<f32> {
    var rgb = textureSample(img_tex, img_samp, in.wpx * viewport.scale / viewport.size).rgb;
    // Saturation, gamma-space luma (matches RgbaImage::saturate).
    let s = in.params.x;
    let luma = dot(rgb, vec3<f32>(0.299, 0.587, 0.114));
    rgb = clamp(vec3<f32>(luma) + (rgb - vec3<f32>(luma)) * s, vec3<f32>(0.0), vec3<f32>(1.0));
    // Rounded-rect clip coverage (1px AA), straight-alpha source-over.
    let center = in.rect.xy + in.rect.zw * 0.5;
    let half = in.rect.zw * 0.5;
    let sd = sd_round_box(in.wpx - center, half, in.radii);
    let cov = clamp(0.5 - sd, 0.0, 1.0);
    return vec4<f32>(rgb, cov);
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
