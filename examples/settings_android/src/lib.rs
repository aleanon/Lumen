//! `settings_android` — the M3-exit settings app as a NativeActivity cdylib.
//! Runs `settings::build` with its bundled stylesheet; tier-1 reloads override
//! it from the watched `.lss` file.

/// NativeActivity entry point.
#[cfg(target_os = "android")]
#[no_mangle]
#[allow(improper_ctypes_definitions)]
extern "C" fn android_main(android: android_activity::AndroidApp) {
    lumen_shell_android::run_styled(android, settings::build, Some(settings::STYLESHEET));
}
