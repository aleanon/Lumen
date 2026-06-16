//! ShaderWidget (T4.1): a WGSL fragment shader rendered to an image, with a CPU
//! solid-fill fallback when no GPU is available, hot reload, and `E0201`
//! diagnostics that keep the last good frame on a broken edit.

use crate::widgets;
use crate::Element;
use lumen_core::{Color, Diagnostic};
use lumen_render::gpu::{GpuRenderer, ShaderUniforms};
use lumen_render::RgbaImage;

/// A stateful shader surface. Build it once, then `set_source`/`set_time`/
/// `set_params` to drive it; `element()` embeds the latest frame.
pub struct ShaderWidget {
    gpu: Option<GpuRenderer>,
    width: u32,
    height: u32,
    fallback: Color,
    uniforms: ShaderUniforms,
    image: RgbaImage,
    diagnostic: Option<Diagnostic>,
    source: Option<String>,
}

impl ShaderWidget {
    /// Create a `width`×`height` shader surface. Without a GPU adapter the
    /// surface is a solid `fallback` fill.
    pub fn new(width: u32, height: u32, fallback: Color) -> ShaderWidget {
        ShaderWidget {
            gpu: GpuRenderer::new(),
            width,
            height,
            fallback,
            uniforms: ShaderUniforms::default(),
            image: solid(width, height, fallback),
            diagnostic: None,
            source: None,
        }
    }

    /// Whether a GPU adapter is available (otherwise the CPU fallback is used).
    pub fn has_gpu(&self) -> bool {
        self.gpu.is_some()
    }

    /// Set/replace the fragment shader (hot reload). On a WGSL compile error
    /// returns the `E0201` diagnostic and keeps the previous frame.
    pub fn set_source(&mut self, fragment: &str) -> Option<Diagnostic> {
        self.source = Some(fragment.to_string());
        self.render()
    }

    /// Advance the shader clock (`u.time`) and re-render.
    pub fn set_time(&mut self, time: f32) -> Option<Diagnostic> {
        self.uniforms.time = time;
        self.render()
    }

    /// Set the four free `u.params` and re-render.
    pub fn set_params(&mut self, params: [f32; 4]) -> Option<Diagnostic> {
        self.uniforms.params = params;
        self.render()
    }

    fn render(&mut self) -> Option<Diagnostic> {
        let src = self.source.clone()?;
        match &self.gpu {
            Some(gpu) => match gpu.render_shader(&src, self.uniforms, self.width, self.height) {
                Ok(img) => {
                    self.image = img;
                    self.diagnostic = None;
                    None
                }
                // Keep the last good frame; surface the diagnostic.
                Err(d) => {
                    self.diagnostic = Some(d.clone());
                    Some(d)
                }
            },
            None => {
                self.image = solid(self.width, self.height, self.fallback);
                None
            }
        }
    }

    /// The latest rendered frame.
    pub fn image(&self) -> &RgbaImage {
        &self.image
    }

    /// The diagnostic from the most recent failed compile, if any.
    pub fn diagnostic(&self) -> Option<&Diagnostic> {
        self.diagnostic.as_ref()
    }

    /// An [`Element`] embedding the latest frame.
    pub fn element(&self) -> Element {
        widgets::image(self.image.clone())
    }
}

fn solid(w: u32, h: u32, c: Color) -> RgbaImage {
    let p = c.to_srgb8();
    let mut px = Vec::with_capacity((w * h * 4) as usize);
    for _ in 0..(w * h) {
        px.extend_from_slice(&p);
    }
    RgbaImage::from_raw(w, h, px)
}
