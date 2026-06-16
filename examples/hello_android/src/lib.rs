//! `hello_android` — the M3 hello app as a NativeActivity cdylib (T3.1).
//!
//! On the host this is an ordinary library exporting [`app`]; for Android it
//! also exposes the `android_main` entry the native-activity glue calls.

use lumen::{widgets, BuildCx, Element};

/// The hello app: a label and a counter button.
pub fn app(cx: &mut BuildCx) -> Element {
    let count = cx.signal("count", || 0i32);
    let v = count.get(cx.runtime());
    widgets::column(vec![
        widgets::text(format!("Hello Lumen — {v}")).id("hello"),
        widgets::button("Tap", move |rt| count.update(rt, |c| *c += 1)).id("tap"),
    ])
    .id("screen")
}

/// NativeActivity entry point (called by the android-activity glue). The
/// `AndroidApp` parameter is not C-FFI-safe by Rust's lint, but this is the
/// signature the glue requires, so the lint is allowed here.
#[cfg(target_os = "android")]
#[no_mangle]
#[allow(improper_ctypes_definitions)]
extern "C" fn android_main(android: android_activity::AndroidApp) {
    lumen_shell_android::run(android, app);
}
