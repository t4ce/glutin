//! A TRUEOS vGPU UI4 window surface.

use std::cell::Cell;
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
use super::vcabi;

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

        Ok(Surface {
            display: self.clone(),
            config: config.clone(),
            window_id,
            bound: Cell::new(None),
            _ty: PhantomData,
        })
    }
}

/// The vGPU surface acquired for a UI4 window while it is bound to a context.
#[derive(Debug, Clone, Copy)]
struct BoundSurface {
    surface: u64,
    width: u32,
    height: u32,
}

/// A TRUEOS vGPU surface backed by a UI4 window.
pub struct Surface<T: SurfaceTypeTrait> {
    display: Display,
    config: Config,
    window_id: NonZeroU32,
    bound: Cell<Option<BoundSurface>>,
    _ty: PhantomData<T>,
}

impl<T: SurfaceTypeTrait> Surface<T> {
    /// Acquire the vGPU surface backing this window for `device`, returning
    /// its opaque handle.
    pub(crate) fn acquire(&self, device: u64) -> Result<u64> {
        let mut info = vcabi::SurfaceInfo::default();
        super::check_rc(unsafe {
            vcabi::trueos_cabi_vgpu_ui4_surface_acquire(device, self.window_id.get(), &mut info)
        })?;

        if info.surface == 0
            || info.width == 0
            || info.height == 0
            || info.pitch < info.width.saturating_mul(4)
            || info.format != vcabi::SURFACE_FORMAT_RGBA8_UNORM_SRGB
        {
            return Err(ErrorKind::BadSurface.into());
        }

        self.bound.set(Some(BoundSurface {
            surface: info.surface,
            width: info.width,
            height: info.height,
        }));

        Ok(info.surface)
    }
}

impl<T: SurfaceTypeTrait> GlSurface<T> for Surface<T> {
    type Context = PossiblyCurrentContext;
    type SurfaceType = T;

    fn buffer_age(&self) -> u32 {
        0
    }

    fn width(&self) -> Option<u32> {
        self.bound.get().map(|bound| bound.width)
    }

    fn height(&self) -> Option<u32> {
        self.bound.get().map(|bound| bound.height)
    }

    fn is_single_buffered(&self) -> bool {
        false
    }

    fn swap_buffers(&self, _context: &Self::Context) -> Result<()> {
        // Presentation happens as part of the vGPU submit calls that already
        // reference the acquired surface handle directly.
        Ok(())
    }

    fn set_swap_interval(&self, _context: &Self::Context, _interval: SwapInterval) -> Result<()> {
        Err(ErrorKind::NotSupported("swap intervals are not supported with TRUEOS").into())
    }

    fn is_current(&self, context: &Self::Context) -> bool {
        match self.bound.get() {
            Some(bound) => context.inner.is_surface_current(bound.surface),
            None => false,
        }
    }

    fn is_current_draw(&self, context: &Self::Context) -> bool {
        self.is_current(context)
    }

    fn is_current_read(&self, context: &Self::Context) -> bool {
        self.is_current(context)
    }

    fn resize(&self, _context: &Self::Context, _width: NonZeroU32, _height: NonZeroU32) {
        // The UI4 compositor determines the surface size; it is refreshed on
        // the next `make_current` acquisition.
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
