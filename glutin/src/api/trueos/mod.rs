//! The TRUEOS Api.
//!
//! This backend targets the TRUEOS vGPU UI4 surface primitive: the host maps
//! a UI4 window directly into an opaque vGPU surface and hands back its
//! handle, dimensions, and pitch.

use crate::error::{Error, ErrorKind, Result};

pub mod config;
pub mod context;
pub mod display;
pub mod surface;

/// Bindings for the subset of the TRUEOS vGPU C ABI used by this backend.
pub(crate) mod vcabi {
    #[repr(C)]
    #[derive(Copy, Clone, Debug, Default)]
    pub struct SurfaceInfo {
        pub surface: u64,
        pub bytes: u64,
        pub width: u32,
        pub height: u32,
        pub pitch: u32,
        pub format: u32,
    }

    pub const SURFACE_FORMAT_RGBA8_UNORM_SRGB: u32 = 1;

    unsafe extern "C" {
        pub fn trueos_cabi_vgpu_open(requested_caps: u64, out_device: *mut u64) -> i32;
        pub fn trueos_cabi_vgpu_close(device: u64) -> i32;
        pub fn trueos_cabi_vgpu_ui4_surface_acquire(
            device: u64,
            window_id: u32,
            out: *mut SurfaceInfo,
        ) -> i32;
        pub fn trueos_cabi_vgpu_ui4_surface_discard(device: u64, surface: u64) -> i32;
    }
}

/// Resolves TRUEOS GL entry points through the host's `trueos_gl` bridge.
pub(crate) mod trueos_gl {
    use std::ffi::{CStr, c_void};

    unsafe extern "C" {
        fn trueos_gl_get_proc_address(name: *const std::ffi::c_char) -> *const c_void;
    }

    pub(crate) fn resolve(name: &CStr) -> *const c_void {
        unsafe { trueos_gl_get_proc_address(name.as_ptr()) }
    }
}

/// `Capabilities::RENDER | Capabilities::PRESENT` from the TRUEOS vGPU Api.
pub(crate) const CAPABILITIES_RENDER_PRESENT: u64 = (1 << 4) | (1 << 6);

/// Map a TRUEOS vGPU C ABI return code to a [`Result`].
pub(crate) fn check_rc(rc: i32) -> Result<()> {
    if rc == 0 {
        return Ok(());
    }

    let kind = match rc {
        -5 => ErrorKind::BadSurface,
        -9 => ErrorKind::BadContext,
        -12 => ErrorKind::OutOfMemory,
        -13 => ErrorKind::BadAccess,
        -16 => ErrorKind::BadAccess,
        -19 => ErrorKind::BadDisplay,
        -32 => ErrorKind::ContextLost,
        -95 => ErrorKind::NotSupported("operation not supported by the TRUEOS vGPU host"),
        _ => ErrorKind::Misc,
    };

    Err(Error::new(Some(rc as i64), None, kind))
}
