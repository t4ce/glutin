//! TRUEOS vGPU context.

use std::cell::Cell;
use std::fmt;

use crate::config::GetGlConfig;
use crate::context::{
    AsRawContext, ContextApi, ContextAttributes, Priority, RawContext, Robustness,
};
use crate::display::GetGlDisplay;
use crate::error::{ErrorKind, Result};
use crate::prelude::*;
use crate::private::Sealed;
use crate::surface::SurfaceTypeTrait;

use super::config::Config;
use super::display::Display;
use super::surface::Surface;
use super::vcabi;

thread_local! {
    /// The `device` handle of the TRUEOS context that is current on this thread, if any.
    static CURRENT_DEVICE: Cell<Option<u64>> = const { Cell::new(None) };
}

impl Display {
    pub(crate) unsafe fn create_context(
        &self,
        config: &Config,
        context_attributes: &ContextAttributes,
    ) -> Result<NotCurrentContext> {
        if matches!(context_attributes.api, Some(ContextApi::OpenGl(_))) {
            return Err(ErrorKind::NotSupported("only gles is supported with TRUEOS").into());
        }

        if context_attributes.robustness != Robustness::NotRobust {
            return Err(ErrorKind::NotSupported("robustness is not supported with TRUEOS").into());
        }

        let mut device = 0u64;
        super::check_rc(unsafe {
            vcabi::trueos_cabi_vgpu_open(super::CAPABILITIES_RENDER_PRESENT, &mut device)
        })?;

        let inner = ContextInner {
            display: self.clone(),
            config: config.clone(),
            device,
            current_surface: Cell::new(None),
        };

        Ok(NotCurrentContext { inner })
    }
}

/// A TRUEOS vGPU context that is known to be not current on the current thread.
#[derive(Debug)]
pub struct NotCurrentContext {
    pub(crate) inner: ContextInner,
}

impl NotCurrentGlContext for NotCurrentContext {
    type PossiblyCurrentContext = PossiblyCurrentContext;
    type Surface<T: SurfaceTypeTrait> = Surface<T>;

    fn treat_as_possibly_current(self) -> PossiblyCurrentContext {
        PossiblyCurrentContext { inner: self.inner, _nosendsync: std::marker::PhantomData }
    }

    fn make_current<T: SurfaceTypeTrait>(
        self,
        surface: &Self::Surface<T>,
    ) -> Result<Self::PossiblyCurrentContext> {
        self.inner.make_current(surface)?;
        Ok(PossiblyCurrentContext { inner: self.inner, _nosendsync: std::marker::PhantomData })
    }

    fn make_current_draw_read<T: SurfaceTypeTrait>(
        self,
        _surface_draw: &Self::Surface<T>,
        _surface_read: &Self::Surface<T>,
    ) -> Result<Self::PossiblyCurrentContext> {
        Err(ErrorKind::NotSupported("make current draw read isn't supported with TRUEOS").into())
    }

    fn make_current_surfaceless(self) -> Result<PossiblyCurrentContext> {
        Err(ErrorKind::NotSupported("surfaceless contexts are not supported with TRUEOS").into())
    }
}

impl GlContext for NotCurrentContext {
    fn context_api(&self) -> ContextApi {
        self.inner.context_api()
    }

    fn priority(&self) -> Priority {
        Priority::Medium
    }
}

impl GetGlConfig for NotCurrentContext {
    type Target = Config;

    fn config(&self) -> Self::Target {
        self.inner.config.clone()
    }
}

impl GetGlDisplay for NotCurrentContext {
    type Target = Display;

    fn display(&self) -> Self::Target {
        self.inner.display.clone()
    }
}

impl AsRawContext for NotCurrentContext {
    fn raw_context(&self) -> RawContext {
        RawContext::TrueOs(self.inner.device)
    }
}

impl Sealed for NotCurrentContext {}

/// A TRUEOS vGPU context that could be current on the current thread.
#[derive(Debug)]
pub struct PossiblyCurrentContext {
    pub(crate) inner: ContextInner,
    // The context could be current only on the one thread.
    _nosendsync: std::marker::PhantomData<*mut ()>,
}

impl PossiblyCurrentGlContext for PossiblyCurrentContext {
    type NotCurrentContext = NotCurrentContext;
    type Surface<T: SurfaceTypeTrait> = Surface<T>;

    fn is_current(&self) -> bool {
        self.inner.is_current()
    }

    fn make_not_current(self) -> Result<Self::NotCurrentContext> {
        self.make_not_current_in_place()?;
        Ok(NotCurrentContext { inner: self.inner })
    }

    fn make_not_current_in_place(&self) -> Result<()> {
        self.inner.make_not_current()
    }

    fn make_current<T: SurfaceTypeTrait>(&self, surface: &Self::Surface<T>) -> Result<()> {
        self.inner.make_current(surface)
    }

    fn make_current_draw_read<T: SurfaceTypeTrait>(
        &self,
        _surface_draw: &Self::Surface<T>,
        _surface_read: &Self::Surface<T>,
    ) -> Result<()> {
        Err(ErrorKind::NotSupported("make current draw read isn't supported with TRUEOS").into())
    }

    fn make_current_surfaceless(&self) -> Result<()> {
        Err(ErrorKind::NotSupported("surfaceless contexts are not supported with TRUEOS").into())
    }
}

impl GlContext for PossiblyCurrentContext {
    fn context_api(&self) -> ContextApi {
        self.inner.context_api()
    }

    fn priority(&self) -> Priority {
        Priority::Medium
    }
}

impl GetGlConfig for PossiblyCurrentContext {
    type Target = Config;

    fn config(&self) -> Self::Target {
        self.inner.config.clone()
    }
}

impl GetGlDisplay for PossiblyCurrentContext {
    type Target = Display;

    fn display(&self) -> Self::Target {
        self.inner.display.clone()
    }
}

impl AsRawContext for PossiblyCurrentContext {
    fn raw_context(&self) -> RawContext {
        RawContext::TrueOs(self.inner.device)
    }
}

impl Sealed for PossiblyCurrentContext {}

pub(crate) struct ContextInner {
    display: Display,
    config: Config,
    /// The vGPU device handle backing this context, doubling as its raw
    /// context id.
    pub(crate) device: u64,
    /// The vGPU surface handle currently bound to this context, if any.
    current_surface: Cell<Option<u64>>,
}

impl ContextInner {
    fn make_current<T: SurfaceTypeTrait>(&self, surface: &Surface<T>) -> Result<()> {
        let bound = surface.acquire(self.device)?;
        self.current_surface.set(Some(bound));
        CURRENT_DEVICE.with(|current| current.set(Some(self.device)));
        Ok(())
    }

    fn make_not_current(&self) -> Result<()> {
        if let Some(surface) = self.current_surface.take() {
            super::check_rc(unsafe {
                vcabi::trueos_cabi_vgpu_ui4_surface_discard(self.device, surface)
            })?;
        }

        CURRENT_DEVICE.with(|current| {
            if current.get() == Some(self.device) {
                current.set(None);
            }
        });

        Ok(())
    }

    fn is_current(&self) -> bool {
        CURRENT_DEVICE.with(|current| current.get() == Some(self.device))
    }

    pub(crate) fn is_surface_current(&self, surface: u64) -> bool {
        self.is_current() && self.current_surface.get() == Some(surface)
    }

    fn context_api(&self) -> ContextApi {
        ContextApi::Gles(None)
    }
}

impl Drop for ContextInner {
    fn drop(&mut self) {
        unsafe {
            vcabi::trueos_cabi_vgpu_close(self.device);
        }
    }
}

impl fmt::Debug for ContextInner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Context")
            .field("config", &self.config)
            .field("device", &self.device)
            .finish()
    }
}
