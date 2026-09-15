//! TRUEOS vGPU UI4 config picking.

use std::iter;
use std::sync::Arc;

use raw_window_handle::RawWindowHandle;

use crate::config::{
    Api, AsRawConfig, ColorBufferType, ConfigSurfaceTypes, ConfigTemplate, GlConfig, RawConfig,
};
use crate::display::GetGlDisplay;
use crate::error::Result;
use crate::private::Sealed;

use super::display::Display;

impl Display {
    pub(crate) unsafe fn find_configs(
        &self,
        template: ConfigTemplate,
    ) -> Result<Box<dyn Iterator<Item = Config> + '_>> {
        if !template_is_satisfiable(&template) {
            return Ok(Box::new(iter::empty()));
        }

        let inner = Arc::new(ConfigInner { display: self.clone() });
        Ok(Box::new(iter::once(Config { inner })))
    }
}

/// The TRUEOS vGPU host only ever exposes a single configuration: an
/// RGBA8/alpha8 window surface with no depth, stencil, or multisampling.
/// `find_configs` therefore filters the requested template against that fixed
/// configuration instead of enumerating anything.
fn template_is_satisfiable(template: &ConfigTemplate) -> bool {
    matches!(template.color_buffer_type, ColorBufferType::Rgb { r_size: 8, g_size: 8, b_size: 8 })
        && template.alpha_size <= 8
        && template.depth_size == 0
        && template.stencil_size == 0
        && template.num_samples.is_none()
        && !template.float_pixels
        && !template.single_buffering
        && template.stereoscopy != Some(true)
        && template.hardware_accelerated != Some(false)
        && template.config_surface_types.contains(ConfigSurfaceTypes::WINDOW)
        && (template.config_surface_types & !ConfigSurfaceTypes::WINDOW).is_empty()
        && template.api.is_none_or(|api| api.contains(Api::GLES2))
        && matches!(template.native_window, None | Some(RawWindowHandle::Trueos(_)))
}

/// The TRUEOS vGPU config.
#[derive(Debug, Clone)]
pub struct Config {
    pub(crate) inner: Arc<ConfigInner>,
}

impl PartialEq for Config {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.inner, &other.inner)
    }
}

impl Eq for Config {}

#[derive(Debug)]
pub(crate) struct ConfigInner {
    display: Display,
}

impl GlConfig for Config {
    fn color_buffer_type(&self) -> Option<ColorBufferType> {
        Some(ColorBufferType::Rgb { r_size: 8, g_size: 8, b_size: 8 })
    }

    fn float_pixels(&self) -> bool {
        false
    }

    fn alpha_size(&self) -> u8 {
        8
    }

    fn depth_size(&self) -> u8 {
        0
    }

    fn stencil_size(&self) -> u8 {
        0
    }

    fn num_samples(&self) -> u8 {
        0
    }

    fn srgb_capable(&self) -> bool {
        true
    }

    fn hardware_accelerated(&self) -> bool {
        true
    }

    fn config_surface_types(&self) -> ConfigSurfaceTypes {
        ConfigSurfaceTypes::WINDOW
    }

    fn supports_transparency(&self) -> Option<bool> {
        Some(true)
    }

    fn api(&self) -> Api {
        Api::GLES2
    }
}

impl GetGlDisplay for Config {
    type Target = Display;

    fn display(&self) -> Self::Target {
        self.inner.display.clone()
    }
}

impl AsRawConfig for Config {
    fn raw_config(&self) -> RawConfig {
        RawConfig::TrueOs(0)
    }
}

impl Sealed for Config {}
