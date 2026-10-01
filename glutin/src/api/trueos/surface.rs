//! A TRUEOS vGPU UI4 window surface.

use std::fmt;
use std::marker::PhantomData;
use std::num::NonZeroU32;

use raw_window_handle::RawWindowHandle;

use crate::config::GetGlConfig;
use crate::display::GetGlDisplay;
use crate::error::{ErrorKind, Result};
use crate::private::Sealed;
use crate::surface::{
    AsRawSurface, GlSurface, PbufferSurface, PixmapSurface, RawSurface, SurfaceAttributes,
    SurfaceTypeTrait, SwapInterval, WindowSurface,
};

use super::config::Config;
use super::context::PossiblyCurrentContext;
use super::display::Display;
use super::{gl, vcabi};

impl Display {
    pub(crate) unsafe fn create_pixmap_surface(
        &self,
        _config: &Config,
        _surface_attributes: &SurfaceAttributes<PixmapSurface>,
    ) -> Result<Surface<PixmapSurface>> {
        Err(ErrorKind::NotSupported("pixmaps are not supported with TRUEOS").into())
    }

    pub(crate) unsafe fn create_pbuffer_surface(
        &self,
        _config: &Config,
        _surface_attributes: &SurfaceAttributes<PbufferSurface>,
    ) -> Result<Surface<PbufferSurface>> {
        Err(ErrorKind::NotSupported("pbuffers are not supported with TRUEOS").into())
    }

    pub(crate) unsafe fn create_window_surface(
        &self,
        config: &Config,
        surface_attributes: &SurfaceAttributes<WindowSurface>,
    ) -> Result<Surface<WindowSurface>> {
        let window_id = match surface_attributes.raw_window_handle {
            Some(RawWindowHandle::Trueos(window)) => window.window,
            _ => {
                return Err(
                    ErrorKind::NotSupported("provided native window is not supported").into()
                );
            },
        };

        if config.display().connection != self.connection {
            return Err(ErrorKind::BadMatch.into());
        }
        super::check_rc(unsafe {
            vcabi::trueos_cabi_ui4_display_validate_window_v1(
                self.connection.get(),
                window_id.get(),
            )
        })?;
        if surface_attributes.srgb == Some(true) || surface_attributes.single_buffer {
            return Err(ErrorKind::NotSupported(
                "sRGB conversion and single buffering are not supported",
            )
            .into());
        }
        let width = surface_attributes.width.ok_or(ErrorKind::BadSurface)?.get();
        let height = surface_attributes.height.ok_or(ErrorKind::BadSurface)?.get();
        Ok(Surface {
            display: self.clone(),
            config: config.clone(),
            window_id,
            runtime: gl::Surface::new(window_id.get(), width, height),
            _ty: PhantomData,
        })
    }
}

/// A TRUEOS vGPU surface backed by a UI4 window.
pub struct Surface<T: SurfaceTypeTrait> {
    display: Display,
    config: Config,
    window_id: NonZeroU32,
    pub(super) runtime: gl::Surface,
    _ty: PhantomData<T>,
}

impl<T: SurfaceTypeTrait> GlSurface<T> for Surface<T> {
    type Context = PossiblyCurrentContext;
    type SurfaceType = T;

    fn buffer_age(&self) -> u32 {
        0
    }

    fn width(&self) -> Option<u32> {
        Some(self.runtime.size().0)
    }

    fn height(&self) -> Option<u32> {
        Some(self.runtime.size().1)
    }

    fn is_single_buffered(&self) -> bool {
        false
    }

    fn swap_buffers(&self, context: &Self::Context) -> Result<()> {
        context.inner.runtime.swap(&self.runtime).map_err(super::error_from_rc)
    }

    fn set_swap_interval(&self, context: &Self::Context, interval: SwapInterval) -> Result<()> {
        if !self.is_current(context) {
            return Err(ErrorKind::BadContext.into());
        }
        match interval {
            SwapInterval::Wait(value) if value.get() == 1 => Ok(()),
            _ => Err(ErrorKind::NotSupported("TRUEOS supports fixed FIFO presentation").into()),
        }
    }

    fn is_current(&self, context: &Self::Context) -> bool {
        context.inner.runtime.is_surface_current(&self.runtime)
    }

    fn is_current_draw(&self, context: &Self::Context) -> bool {
        self.is_current(context)
    }

    fn is_current_read(&self, context: &Self::Context) -> bool {
        self.is_current(context)
    }

    fn resize(&self, context: &Self::Context, width: NonZeroU32, height: NonZeroU32) {
        if let Err(error) = context.inner.runtime.resize(&self.runtime, width.get(), height.get()) {
            context.inner.runtime.record_failure(error);
        }
    }
}

impl<T: SurfaceTypeTrait> GetGlConfig for Surface<T> {
    type Target = Config;

    fn config(&self) -> Self::Target {
        self.config.clone()
    }
}

impl<T: SurfaceTypeTrait> GetGlDisplay for Surface<T> {
    type Target = Display;

    fn display(&self) -> Self::Target {
        self.display.clone()
    }
}

impl<T: SurfaceTypeTrait> AsRawSurface for Surface<T> {
    fn raw_surface(&self) -> RawSurface {
        RawSurface::TrueOs(self.window_id.get() as u64)
    }
}

impl<T: SurfaceTypeTrait> fmt::Debug for Surface<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Surface")
            .field("config", &self.config)
            .field("window_id", &self.window_id)
            .field("type", &T::surface_type())
            .finish()
    }
}

impl<T: SurfaceTypeTrait> Sealed for Surface<T> {}
