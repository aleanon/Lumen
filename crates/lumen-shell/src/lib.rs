//! `lumen-shell` — the winit desktop shell (02 §8 `App::run`).
//!
//! Opens a window, drives the headless runtime each frame, and presents the
//! rendered frame to a wgpu surface. Input is translated to lumen [`Event`]s and
//! injected through the one input queue. Redraws are event-driven (idle ⇒ no
//! frames). Mobile shells arrive in M3.
//!
//! `App::run` is provided as an extension trait ([`RunExt`]) because `App` lives
//! in `lumen-widgets` (below this crate); the `lumen` facade re-exports it.

use kurbo::{Point, Size, Vec2};
use lumen_core::events::{
    Event, Key, KeyEvent, Modifiers, NamedKey, PointerButton, PointerEvent, PointerKind, WheelEvent,
};
use lumen_render::RgbaImage;
use lumen_widgets::{App, Headless};
use std::sync::Arc;
use std::time::{Duration, Instant};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, StartCause, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

/// Extension trait adding `run()` to [`App`] (02 §8).
pub trait RunExt {
    /// Open a window and run the app to completion (blocks until close).
    fn run(self, size: Size);
}

impl RunExt for App {
    fn run(self, size: Size) {
        run(self, size);
    }
}

/// Open a window and run `app` at `size`.
pub fn run(app: App, size: Size) {
    let event_loop = EventLoop::new().expect("event loop");
    let mut shell = Shell {
        app: Some(app),
        size,
        headless: None,
        window: None,
        presenter: None,
        cursor: Point::ZERO,
        last_frame: Instant::now(),
    };
    event_loop.run_app(&mut shell).expect("run app");
}

struct Shell {
    app: Option<App>,
    size: Size,
    headless: Option<Headless>,
    window: Option<Arc<Window>>,
    presenter: Option<Presenter>,
    cursor: Point,
    /// Wall-clock time of the previous presented frame; the delta drives the
    /// runtime's virtual clock. The shell is the *only* place wall time enters.
    last_frame: Instant,
}

impl ApplicationHandler for Shell {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attrs = Window::default_attributes()
            .with_title("Lumen")
            .with_inner_size(winit::dpi::LogicalSize::new(
                self.size.width,
                self.size.height,
            ));
        let window = Arc::new(el.create_window(attrs).expect("window"));
        let presenter = Presenter::new(window.clone());
        let app = self.app.take().expect("app");
        self.headless = Some(app.run_headless(self.size));
        self.presenter = Some(presenter);
        window.request_redraw(); // paint the first frame
        self.window = Some(window);
        self.last_frame = Instant::now();
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => el.exit(),
            WindowEvent::Resized(s) => {
                if let Some(p) = &mut self.presenter {
                    p.resize(s.width.max(1), s.height.max(1));
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = Point::new(position.x, position.y);
                self.inject(Event::PointerMove(PointerEvent::at(self.cursor)));
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let pe = PointerEvent {
                    pos: self.cursor,
                    button: map_button(button),
                    pointer: PointerKind::Mouse,
                    modifiers: Modifiers::empty(),
                    click_count: 1,
                };
                self.inject(if state == ElementState::Pressed {
                    Event::PointerDown(pe)
                } else {
                    Event::PointerUp(pe)
                });
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let d = match delta {
                    MouseScrollDelta::LineDelta(x, y) => {
                        Vec2::new(x as f64 * 40.0, y as f64 * 40.0)
                    }
                    MouseScrollDelta::PixelDelta(p) => Vec2::new(p.x, p.y),
                };
                self.inject(Event::Wheel(WheelEvent {
                    pos: self.cursor,
                    delta: d,
                    modifiers: Modifiers::empty(),
                }));
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if let Some(k) = map_key(&event.logical_key) {
                    let ke = KeyEvent {
                        key: k,
                        modifiers: Modifiers::empty(),
                        repeat: event.repeat,
                    };
                    self.inject(if event.state == ElementState::Pressed {
                        Event::KeyDown(ke)
                    } else {
                        Event::KeyUp(ke)
                    });
                }
            }
            WindowEvent::RedrawRequested => {
                if let (Some(h), Some(p)) = (&mut self.headless, &mut self.presenter) {
                    let now = Instant::now();
                    let elapsed_ms = (now - self.last_frame).as_secs_f64() * 1000.0;
                    self.last_frame = now;
                    // Advance the virtual clock by real elapsed time, then pump.
                    // Clamp the step so a sleep/background pause becomes one
                    // bounded jump rather than a long skip (since the UI renders
                    // as a function of now_ms(), there is no tick backlog to
                    // replay — just a single catch-up frame).
                    h.advance_clock(elapsed_ms.min(1000.0));
                    h.pump();
                    let frame = h.screenshot();
                    p.present(&frame);
                }
            }
            _ => {}
        }
    }

    /// A `WaitUntil` deadline elapsed: a one-shot wake (e.g. a delayed reveal)
    /// is due, so ask for the frame that will reflect it.
    fn new_events(&mut self, _el: &ActiveEventLoop, cause: StartCause) {
        if matches!(cause, StartCause::ResumeTimeReached { .. }) {
            if let Some(w) = &self.window {
                w.request_redraw();
            }
        }
    }

    /// Decide how to wait for the next frame from what the UI asked for, so an
    /// idle UI costs zero frames while an animating one runs free.
    fn about_to_wait(&mut self, el: &ActiveEventLoop) {
        let Some(h) = &self.headless else { return };
        match h.next_deadline() {
            // Idle: sleep until the next OS event (input/resize/close).
            None => el.set_control_flow(ControlFlow::Wait),
            // Continuous animation: keep producing frames back-to-back.
            Some(t) if t <= h.now_ms() => {
                el.set_control_flow(ControlFlow::Poll);
                if let Some(w) = &self.window {
                    w.request_redraw();
                }
            }
            // One-shot wake: sleep until the (virtual==real) deadline.
            Some(t) => {
                let dt = (t - h.now_ms()).max(0.0);
                el.set_control_flow(ControlFlow::WaitUntil(
                    Instant::now() + Duration::from_secs_f64(dt / 1000.0),
                ));
            }
        }
    }
}

impl Shell {
    fn inject(&mut self, ev: Event) {
        if let Some(h) = &mut self.headless {
            h.inject(ev);
        }
        if let Some(w) = &self.window {
            w.request_redraw(); // event-driven: redraw only after input
        }
    }
}

fn map_button(b: MouseButton) -> PointerButton {
    match b {
        MouseButton::Left => PointerButton::Left,
        MouseButton::Right => PointerButton::Right,
        MouseButton::Middle => PointerButton::Middle,
        MouseButton::Other(n) => PointerButton::Other(n),
        _ => PointerButton::Left,
    }
}

fn map_key(k: &winit::keyboard::Key) -> Option<Key> {
    use winit::keyboard::{Key as WK, NamedKey as WNK};
    match k {
        WK::Named(WNK::Tab) => Some(Key::Named(NamedKey::Tab)),
        WK::Named(WNK::Enter) => Some(Key::Named(NamedKey::Enter)),
        WK::Named(WNK::Space) => Some(Key::Named(NamedKey::Space)),
        WK::Named(WNK::Escape) => Some(Key::Named(NamedKey::Escape)),
        WK::Named(WNK::Backspace) => Some(Key::Named(NamedKey::Backspace)),
        WK::Named(WNK::ArrowLeft) => Some(Key::Named(NamedKey::ArrowLeft)),
        WK::Named(WNK::ArrowRight) => Some(Key::Named(NamedKey::ArrowRight)),
        WK::Named(WNK::ArrowUp) => Some(Key::Named(NamedKey::ArrowUp)),
        WK::Named(WNK::ArrowDown) => Some(Key::Named(NamedKey::ArrowDown)),
        WK::Character(s) => Some(Key::Character(s.as_str().into())),
        _ => None,
    }
}

/// Presents a CPU-rendered frame to a wgpu surface via a fullscreen blit.
struct Presenter {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    pipeline: wgpu::RenderPipeline,
    bgl: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
}

impl Presenter {
    fn new(window: Arc<Window>) -> Presenter {
        let size = window.inner_size();
        let instance = wgpu::Instance::default();
        let surface = instance.create_surface(window).expect("surface");
        let adapter = block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        }))
        .expect("adapter");
        let (device, queue) =
            block_on(adapter.request_device(&Default::default(), None)).expect("device");
        let caps = surface.get_capabilities(&adapter);
        let format = caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or(caps.formats[0]);
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::Fifo, // vsync
            desired_maximum_frame_latency: 2,
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
        };
        surface.configure(&device, &config);

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("blit"),
            source: wgpu::ShaderSource::Wgsl(BLIT.into()),
        });
        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("blit-bgl"),
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
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("blit-layout"),
            bind_group_layouts: &[&bgl],
            push_constant_ranges: &[],
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("blit"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs",
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs",
                targets: &[Some(format.into())],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor::default());
        Presenter {
            surface,
            device,
            queue,
            config,
            pipeline,
            bgl,
            sampler,
        }
    }

    fn resize(&mut self, w: u32, h: u32) {
        self.config.width = w;
        self.config.height = h;
        self.surface.configure(&self.device, &self.config);
    }

    fn present(&mut self, frame: &RgbaImage) {
        let tex = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("frame"),
            size: wgpu::Extent3d {
                width: frame.width(),
                height: frame.height(),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
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
            frame.pixels(),
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(frame.width() * 4),
                rows_per_image: Some(frame.height()),
            },
            wgpu::Extent3d {
                width: frame.width(),
                height: frame.height(),
                depth_or_array_layers: 1,
            },
        );
        let view = tex.create_view(&Default::default());
        let bind = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("blit-bg"),
            layout: &self.bgl,
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

        let Ok(surface_tex) = self.surface.get_current_texture() else {
            return;
        };
        let sview = surface_tex.texture.create_view(&Default::default());
        let mut enc = self.device.create_command_encoder(&Default::default());
        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("blit-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &sview,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::WHITE),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &bind, &[]);
            pass.draw(0..3, 0..1);
        }
        self.queue.submit(Some(enc.finish()));
        surface_tex.present();
    }
}

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

const BLIT: &str = r#"
struct VsOut { @builtin(position) pos: vec4<f32>, @location(0) uv: vec2<f32> };
@vertex
fn vs(@builtin(vertex_index) i: u32) -> VsOut {
    var uv = array<vec2<f32>, 3>(vec2<f32>(0.0,0.0), vec2<f32>(2.0,0.0), vec2<f32>(0.0,2.0));
    var o: VsOut;
    o.uv = uv[i];
    o.pos = vec4<f32>(uv[i] * 2.0 - 1.0, 0.0, 1.0);
    o.pos.y = -o.pos.y;
    return o;
}
@group(0) @binding(0) var t: texture_2d<f32>;
@group(0) @binding(1) var s: sampler;
@fragment
fn fs(in: VsOut) -> @location(0) vec4<f32> {
    return textureSample(t, s, in.uv);
}
"#;
