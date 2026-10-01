//! TRUEOS vGPU context.

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
use super::gl;
use super::surface::Surface;

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

        if config.display().connection != self.connection {
            return Err(ErrorKind::BadMatch.into());
        }
        if context_attributes.shared_context.is_some() || context_attributes.profile.is_some() {
            return Err(ErrorKind::NotSupported(
                "context sharing and desktop profiles are not supported",
            )
            .into());
        }
        if matches!(context_attributes.api, Some(ContextApi::Gles(Some(version)))
            if version != crate::context::Version::new(2, 0))
        {
            return Err(ErrorKind::NotSupported("TRUEOS profile 0 requires GLES 2.0").into());
        }
        let runtime = gl::Context::new().map_err(super::error_from_rc)?;
        let inner = ContextInner { display: self.clone(), config: config.clone(), runtime };

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
        RawContext::TrueOs(self.inner.runtime.id())
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
        RawContext::TrueOs(self.inner.runtime.id())
    }
}

impl Sealed for PossiblyCurrentContext {}

pub(crate) struct ContextInner {
    display: Display,
    config: Config,
    pub(super) runtime: gl::Context,
}

impl ContextInner {
    fn make_current<T: SurfaceTypeTrait>(&self, surface: &Surface<T>) -> Result<()> {
        if self.display.connection != surface.display().connection {
            return Err(ErrorKind::BadMatch.into());
        }
        self.runtime.make_current(&surface.runtime).map_err(super::error_from_rc)
    }

    fn make_not_current(&self) -> Result<()> {
        self.runtime.make_not_current().map_err(super::error_from_rc)
    }

    fn is_current(&self) -> bool {
        self.runtime.is_current()
    }

    fn context_api(&self) -> ContextApi {
        ContextApi::Gles(Some(crate::context::Version::new(2, 0)))
    }
}

impl fmt::Debug for ContextInner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Context")
            .field("config", &self.config)
            .field("runtime", &self.runtime)
            .finish()
    }
}
