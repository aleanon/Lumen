//! Hot-patch fixture v2 (the "after edit" component cdylib).
use std::ffi::c_char;

/// ABI hash (compiler + core fingerprints, simulated). Stable across v1/v2.
#[no_mangle]
pub extern "C" fn lumen_abi_hash() -> u64 {
    0x1111_2222_3333_4444
}

/// The component's `build()` output (a label prefix). v2 says "Counter".
#[no_mangle]
pub extern "C" fn lumen_build_label() -> *const c_char {
    c"Counter".as_ptr()
}
