//! Media pipeline (T6.3): a **deterministic software path** for CI — procedural
//! video frames clocked to a timestamp and synthesized audio — so media-driven
//! UIs get exact goldens with no hardware codec.
//!
//! Hardware-accelerated video decode and real mic/camera capture are platform
//! work (a thin shell over the same `VideoSource`/`AudioBuffer` model); they are
//! tracked separately. The deterministic model + its frame/audio APIs land here.

use crate::image::RgbaImage;

/// A video source that yields a frame at a given time (seconds).
pub trait VideoSource {
    /// Render the frame at `t` seconds into a `width`×`height` image.
    fn frame_at(&self, t: f64, width: u32, height: u32) -> RgbaImage;
}

/// A deterministic procedural clip (an animated diagonal gradient) — the
/// software decoder used for golden tests.
pub struct TestPattern;

impl VideoSource for TestPattern {
    fn frame_at(&self, t: f64, width: u32, height: u32) -> RgbaImage {
        let mut px = Vec::with_capacity((width * height * 4) as usize);
        for y in 0..height {
            for x in 0..width {
                let phase = (x as f64 + y as f64) * 0.08 + t * 3.0;
                let v = 0.5 + 0.5 * phase.sin();
                let r = (v * 255.0) as u8;
                let g = ((0.5 + 0.5 * (phase + 2.0).sin()) * 255.0) as u8;
                let b = ((0.5 + 0.5 * (phase + 4.0).sin()) * 255.0) as u8;
                px.extend_from_slice(&[r, g, b, 255]);
            }
        }
        RgbaImage::from_raw(width, height, px)
    }
}

/// A mono PCM audio buffer.
#[derive(Clone, Debug, PartialEq)]
pub struct AudioBuffer {
    /// Samples per second.
    pub sample_rate: u32,
    /// Mono samples in `[-1, 1]`.
    pub samples: Vec<f32>,
}

impl AudioBuffer {
    /// Duration in seconds.
    pub fn duration(&self) -> f64 {
        self.samples.len() as f64 / self.sample_rate as f64
    }
}

/// Synthesize a sine tone — deterministic test audio.
pub fn sine(freq: f64, secs: f64, sample_rate: u32) -> AudioBuffer {
    let n = (secs * sample_rate as f64).round() as usize;
    let samples = (0..n)
        .map(|i| (std::f64::consts::TAU * freq * i as f64 / sample_rate as f64).sin() as f32)
        .collect();
    AudioBuffer {
        sample_rate,
        samples,
    }
}

/// A capture source the platform shell binds; agent-observable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaptureSource {
    /// The default camera.
    Camera,
    /// The default microphone.
    Microphone,
}
