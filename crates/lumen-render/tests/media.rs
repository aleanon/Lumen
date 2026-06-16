//! T6.3: deterministic media — procedural video frame golden + audio synth.
use lumen_core::Color;
use lumen_render::media::{sine, CaptureSource, TestPattern, VideoSource};
use lumen_render::RgbaImage;
use std::path::PathBuf;

fn golden(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden/cpu")
        .join(format!("{name}.png"))
}

#[test]
fn video_frame_is_deterministic_golden() {
    let f1 = TestPattern.frame_at(1.0, 48, 32);
    assert_eq!((f1.width(), f1.height()), (48, 32));
    // Same time -> identical frame; different time -> different frame.
    assert_eq!(f1.pixels(), TestPattern.frame_at(1.0, 48, 32).pixels());
    assert_ne!(f1.pixels(), TestPattern.frame_at(2.0, 48, 32).pixels());

    let path = golden("video_t1");
    if std::env::var_os("LUMEN_UPDATE_GOLDENS").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, f1.to_png()).unwrap();
        return;
    }
    let expected = RgbaImage::from_png(&std::fs::read(&path).unwrap()).unwrap();
    let _ = Color::WHITE;
    assert_eq!(f1.diff_count(&expected), 0, "video frame golden mismatch");
}

#[test]
fn audio_synth_is_deterministic() {
    let a = sine(440.0, 0.5, 48_000);
    assert_eq!(a.samples.len(), 24_000);
    assert!((a.duration() - 0.5).abs() < 1e-6);
    assert_eq!(a.samples[0], 0.0); // sin(0) = 0
    assert_eq!(a, sine(440.0, 0.5, 48_000));
    assert_ne!(CaptureSource::Camera, CaptureSource::Microphone);
}
