//! `just run integration` — a host app that OWNS winit + wgpu (an animated
//! gradient scene) and embeds a Lumen `Headless` as a HUD texture in the
//! corner: clicks inside the HUD region forward into Lumen's input queue;
//! everything else stays the host's.
use lumen_core::events::{Event, PointerEvent};
use lumen_core::geometry::{Point, Size};
use std::sync::Arc;
use winit::application::ApplicationHandler;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

const HUD_W: u32 = 260;
const HUD_H: u32 = 140;
const HUD_POS: (f64, f64) = (20.0, 20.0);

struct Host {
    window: Option<Arc<Window>>,
    gpu: Option<Gpu>,
    hud: Option<lumen_widgets::Headless>,
    cursor: Point,
    t0: std::time::Instant,
}

struct Gpu {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    hud_tex: wgpu::Texture,
    pipeline: wgpu::RenderPipeline,
    bind: wgpu::BindGroup,
}

const SHADER: &str = r#"
struct VsOut { @builtin(position) pos: vec4<f32>, @location(0) uv: vec2<f32> };
@vertex
fn vs(@builtin(vertex_index) i: u32) -> VsOut {
    var p = array<vec2<f32>, 3>(vec2(-1.0, -3.0), vec2(-1.0, 1.0), vec2(3.0, 1.0));
    var out: VsOut;
    out.pos = vec4(p[i], 0.0, 1.0);
    out.uv = (p[i] + vec2(1.0, 1.0)) * 0.5;
    return out;
}
@group(0) @binding(0) var t: texture_2d<f32>;
@group(0) @binding(1) var s: sampler;
@fragment
fn fs(in: VsOut) -> @location(0) vec4<f32> {
    return textureSample(t, s, vec2(in.uv.x, 1.0 - in.uv.y));
}
"#;

impl ApplicationHandler for Host {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let window = Arc::new(
            el.create_window(
                Window::default_attributes()
                    .with_title("host app (Lumen embedded)")
                    .with_inner_size(winit::dpi::LogicalSize::new(800.0, 500.0)),
            )
            .expect("window"),
        );
        // The HOST's wgpu stack — Lumen never sees it.
        let instance = wgpu::Instance::default();
        let surface = instance.create_surface(window.clone()).expect("surface");
        let adapter = pollster_block(instance.request_adapter(&wgpu::RequestAdapterOptions {
            compatible_surface: Some(&surface),
            ..Default::default()
        }))
        .expect("adapter");
        let (device, queue) =
            pollster_block(adapter.request_device(&Default::default(), None)).expect("device");
        let size = window.inner_size();
        let config = surface
            .get_default_config(&adapter, size.width.max(1), size.height.max(1))
            .expect("config");
        surface.configure(&device, &config);

        let hud_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("lumen hud"),
            size: wgpu::Extent3d {
                width: HUD_W,
                height: HUD_H,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("blit"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let sampler = device.create_sampler(&Default::default());
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
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
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(
                        &hud_tex.create_view(&Default::default()),
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });
        let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&layout],
            push_constant_ranges: &[],
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("hud blit"),
            layout: Some(&pl),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs",
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs",
                targets: &[Some(config.format.into())],
                compilation_options: Default::default(),
            }),
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            multiview: None,
            cache: None,
        });

        // Lumen: just a value the host owns.
        self.hud = Some(
            integration::hud_app().run_headless(Size::new(f64::from(HUD_W), f64::from(HUD_H))),
        );
        self.gpu = Some(Gpu {
            surface,
            device,
            queue,
            config,
            hud_tex,
            pipeline,
            bind,
        });
        window.request_redraw();
        self.window = Some(window);
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => el.exit(),
            WindowEvent::Resized(s) => {
                if let Some(g) = &mut self.gpu {
                    g.config.width = s.width.max(1);
                    g.config.height = s.height.max(1);
                    g.surface.configure(&g.device, &g.config);
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = Point::new(position.x, position.y);
            }
            WindowEvent::MouseInput { state, .. } => {
                // Forward clicks inside the HUD region into Lumen's queue.
                let (hx, hy) = HUD_POS;
                let scale = self.window.as_ref().map_or(1.0, |w| w.scale_factor());
                let p = Point::new(self.cursor.x / scale - hx, self.cursor.y / scale - hy);
                if p.x >= 0.0 && p.y >= 0.0 && p.x < f64::from(HUD_W) && p.y < f64::from(HUD_H) {
                    if let Some(hud) = &mut self.hud {
                        let ev = PointerEvent::at(p);
                        hud.inject(if state == ElementState::Pressed {
                            Event::PointerDown(ev)
                        } else {
                            Event::PointerUp(ev)
                        });
                    }
                }
            }
            WindowEvent::RedrawRequested => {
                let (Some(g), Some(hud)) = (&mut self.gpu, &mut self.hud) else {
                    return;
                };
                // 1. Lumen: pump + upload the HUD frame when it changed.
                let stats = hud.pump();
                if stats.painted {
                    let frame = hud.screenshot();
                    g.queue.write_texture(
                        wgpu::ImageCopyTexture {
                            texture: &g.hud_tex,
                            mip_level: 0,
                            origin: wgpu::Origin3d::ZERO,
                            aspect: wgpu::TextureAspect::All,
                        },
                        frame.pixels(),
                        wgpu::ImageDataLayout {
                            offset: 0,
                            bytes_per_row: Some(HUD_W * 4),
                            rows_per_image: Some(HUD_H),
                        },
                        wgpu::Extent3d {
                            width: HUD_W,
                            height: HUD_H,
                            depth_or_array_layers: 1,
                        },
                    );
                }
                // 2. The host's own scene: an animated clear color.
                let t = self.t0.elapsed().as_secs_f64();
                let target = match g.surface.get_current_texture() {
                    Ok(t) => t,
                    Err(_) => return,
                };
                let view = target.texture.create_view(&Default::default());
                let mut enc = g.device.create_command_encoder(&Default::default());
                {
                    let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("host scene"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: &view,
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Clear(wgpu::Color {
                                    r: 0.12 + 0.08 * t.sin(),
                                    g: 0.10,
                                    b: 0.25 + 0.10 * (t * 0.7).cos(),
                                    a: 1.0,
                                }),
                                store: wgpu::StoreOp::Store,
                            },
                        })],
                        ..Default::default()
                    });
                    // 3. The Lumen HUD, blitted into its corner viewport.
                    let scale = self.window.as_ref().map_or(1.0, |w| w.scale_factor()) as f32;
                    pass.set_viewport(
                        HUD_POS.0 as f32 * scale,
                        HUD_POS.1 as f32 * scale,
                        HUD_W as f32 * scale,
                        HUD_H as f32 * scale,
                        0.0,
                        1.0,
                    );
                    pass.set_pipeline(&g.pipeline);
                    pass.set_bind_group(0, &g.bind, &[]);
                    pass.draw(0..3, 0..1);
                }
                g.queue.submit([enc.finish()]);
                target.present();
                if let Some(w) = &self.window {
                    w.request_redraw(); // host animates continuously
                }
            }
            _ => {}
        }
    }
}

/// Tiny blocking executor for the two setup futures (no async runtime dep).
fn pollster_block<F: std::future::Future>(fut: F) -> F::Output {
    use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};
    fn raw() -> RawWaker {
        fn no(_: *const ()) {}
        fn cl(_: *const ()) -> RawWaker {
            raw()
        }
        RawWaker::new(std::ptr::null(), &RawWakerVTable::new(cl, no, no, no))
    }
    let waker = unsafe { Waker::from_raw(raw()) };
    let mut cx = Context::from_waker(&waker);
    let mut fut = std::pin::pin!(fut);
    loop {
        match fut.as_mut().poll(&mut cx) {
            Poll::Ready(v) => return v,
            Poll::Pending => std::thread::yield_now(),
        }
    }
}

fn main() {
    let el = EventLoop::new().expect("event loop");
    el.set_control_flow(ControlFlow::Poll);
    let mut host = Host {
        window: None,
        gpu: None,
        hud: None,
        cursor: Point::ZERO,
        t0: std::time::Instant::now(),
    };
    el.run_app(&mut host).expect("run");
}
