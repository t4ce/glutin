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

use trueos_gl as gl;
use trueos_gl::abi as vcabi;

/// Map a TRUEOS vGPU C ABI return code to a [`Result`].
pub(crate) fn check_rc(rc: i32) -> Result<()> {
    if rc == 0 {
        return Ok(());
    }

    Err(error_from_rc(rc))
}

fn error_from_rc(rc: i32) -> Error {
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

    Error::new(Some(rc as i64), None, kind)
}

#[cfg(test)]
mod tests;
