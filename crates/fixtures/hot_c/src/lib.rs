//! Hot-patch fixture v1 (the "before edit" component cdylib).
use std::ffi::c_char;

/// ABI hash (compiler + core fingerprints, simulated). Different -> ABI-incompatible (tier-3).
#[no_mangle]
pub extern "C" fn lumen_abi_hash() -> u64 {
    0xDEAD_BEEF_DEAD_BEEF
}

/// The component's `build()` output (a label prefix). v1 says "Count".
#[no_mangle]
pub extern "C" fn lumen_build_label() -> *const c_char {
    c"Count".as_ptr()
}
