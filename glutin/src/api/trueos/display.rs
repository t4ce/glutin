//! A TRUEOS display.

use std::ffi::{self, CStr};
use std::marker::PhantomData;
use std::num::NonZeroU64;

use raw_window_handle::RawDisplayHandle;

use crate::config::ConfigTemplate;
use crate::context::ContextAttributes;
use crate::display::{AsRawDisplay, DisplayFeatures, RawDisplay};
use crate::error::{ErrorKind, Result};
use crate::prelude::*;
use crate::private::Sealed;
use crate::surface::{PbufferSurface, PixmapSurface, SurfaceAttributes, WindowSurface};

use super::config::Config;
use super::context::NotCurrentContext;
use super::surface::Surface;
use super::trueos_gl;

/// The TRUEOS display.
#[derive(Debug, Clone)]
pub struct Display {
    /// Host-issued UI4 graphics connection capability.
    pub(crate) connection: NonZeroU64,
    _marker: PhantomData<()>,
}

impl Display {
    /// Create TRUEOS display.
    ///
    /// # Safety
    ///
    /// The `connection` capability carried by `display` must remain valid for
    /// the entire lifetime of this object and everything created with it.
    pub unsafe fn new(display: RawDisplayHandle) -> Result<Self> {
        match display {
            RawDisplayHandle::Trueos(handle) => {
                Ok(Display { connection: handle.connection, _marker: PhantomData })
            },
            _ => Err(ErrorKind::NotSupported("provided native display is not supported").into()),
        }
    }
}

impl GlDisplay for Display {
    type Config = Config;
    type NotCurrentContext = NotCurrentContext;
    type PbufferSurface = Surface<PbufferSurface>;
    type PixmapSurface = Surface<PixmapSurface>;
    type WindowSurface = Surface<WindowSurface>;

    unsafe fn find_configs(
        &self,
        template: ConfigTemplate,
    ) -> Result<Box<dyn Iterator<Item = Self::Config> + '_>> {
        unsafe { Self::find_configs(self, template) }
    }

    unsafe fn create_window_surface(
        &self,
        config: &Self::Config,
        surface_attributes: &SurfaceAttributes<WindowSurface>,
    ) -> Result<Self::WindowSurface> {
        unsafe { Self::create_window_surface(self, config, surface_attributes) }
    }

    unsafe fn create_pbuffer_surface(
        &self,
        config: &Self::Config,
        surface_attributes: &SurfaceAttributes<PbufferSurface>,
    ) -> Result<Self::PbufferSurface> {
        unsafe { Self::create_pbuffer_surface(self, config, surface_attributes) }
    }

    unsafe fn create_pixmap_surface(
        &self,
        config: &Self::Config,
        surface_attributes: &SurfaceAttributes<PixmapSurface>,
    ) -> Result<Self::PixmapSurface> {
        unsafe { Self::create_pixmap_surface(self, config, surface_attributes) }
    }

    unsafe fn create_context(
        &self,
        config: &Self::Config,
        context_attributes: &ContextAttributes,
    ) -> Result<Self::NotCurrentContext> {
        unsafe { Self::create_context(self, config, context_attributes) }
    }

    fn get_proc_address(&self, addr: &CStr) -> *const ffi::c_void {
        trueos_gl::resolve(addr)
    }

    fn version_string(&self) -> String {
        String::from("TRUEOS vGPU UI4")
    }

    fn supported_features(&self) -> DisplayFeatures {
        DisplayFeatures::empty()
    }
}

impl AsRawDisplay for Display {
    fn raw_display(&self) -> RawDisplay {
        RawDisplay::TrueOs(self.connection.get())
    }
}

impl Sealed for Display {}
