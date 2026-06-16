//! Android-only implementation: native-activity event loop + software blit.

use android_activity::{AndroidApp, MainEvent, PollEvent};
use lumen::{App, BuildCx, Element, Headless};
use lumen_core::geometry::Size;
use ndk::hardware_buffer_format::HardwareBufferFormat;
use std::rc::Rc;
use std::time::Duration;

/// Run `build` as a Lumen app on this `NativeActivity`, presenting CPU frames to
/// the window. Returns when the activity is destroyed.
pub fn run(android: AndroidApp, build: impl Fn(&mut BuildCx) -> Element + 'static) {
    android_logger::init_once(
        android_logger::Config::default().with_max_level(log::LevelFilter::Info),
    );
    log::info!("lumen android shell starting");

    let build = Rc::new(build);
    let mut headless: Option<Headless> = None;
    let mut size = (0u32, 0u32);
    let mut quit = false;

    while !quit {
        android.poll_events(Some(Duration::from_millis(250)), |event| match event {
            PollEvent::Main(MainEvent::InitWindow { .. })
            | PollEvent::Main(MainEvent::RedrawNeeded { .. })
            | PollEvent::Main(MainEvent::WindowResized { .. }) => {
                present(&android, &build, &mut headless, &mut size);
            }
            PollEvent::Main(MainEvent::TerminateWindow { .. }) => {
                headless = None;
                size = (0, 0);
            }
            PollEvent::Main(MainEvent::Destroy) => quit = true,
            _ => {}
        });
    }
}

fn present(
    android: &AndroidApp,
    build: &Rc<impl Fn(&mut BuildCx) -> Element + 'static>,
    headless: &mut Option<Headless>,
    size: &mut (u32, u32),
) {
    let Some(window) = android.native_window() else {
        return;
    };
    let (w, h) = (window.width() as u32, window.height() as u32);
    if w == 0 || h == 0 {
        return;
    }

    // (Re)build the app whenever the surface size changes.
    if *size != (w, h) || headless.is_none() {
        let build = build.clone();
        *headless =
            Some(App::new(move |cx| (build)(cx)).run_headless(Size::new(w as f64, h as f64)));
        *size = (w, h);
    }
    let hl = headless.as_mut().unwrap();
    hl.pump();
    let frame = hl.screenshot();

    if window
        .set_buffers_geometry(
            w as i32,
            h as i32,
            Some(HardwareBufferFormat::R8G8B8A8_UNORM),
        )
        .is_err()
    {
        log::warn!("set_buffers_geometry failed");
        return;
    }
    let Ok(mut buf) = window.lock(None) else {
        log::warn!("native window lock failed");
        return;
    };

    // Copy our straight-RGBA8 frame row by row, honouring the buffer stride.
    let stride = buf.stride() as usize; // pixels per row
    let dst_w = (buf.width() as usize).min(w as usize);
    let dst_h = (buf.height() as usize).min(h as usize);
    let src = frame.pixels();
    let dst = buf.bits() as *mut u8;
    for y in 0..dst_h {
        let s = y * (w as usize) * 4;
        let d = y * stride * 4;
        // SAFETY: both ranges are within their buffers (dst_w ≤ both widths).
        unsafe {
            std::ptr::copy_nonoverlapping(src.as_ptr().add(s), dst.add(d), dst_w * 4);
        }
    }
}
