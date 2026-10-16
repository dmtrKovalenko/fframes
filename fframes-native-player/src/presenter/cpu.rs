use std::num::NonZeroU32;
use std::rc::Rc;

use fframes_skia_renderer::skia_safe::{AlphaType, Canvas, ColorType, ImageInfo, surfaces};
use winit::window::Window;

use super::WindowTarget;
use crate::error::{PlayerError, PlayerResult};

/// Skia draws on the CPU into the pixel buffer of the window.
pub(super) struct CpuTarget {
    surface: softbuffer::Surface<Rc<Window>, Rc<Window>>,
    size: (u32, u32),
}

impl CpuTarget {
    pub(super) fn new(window: Rc<Window>) -> PlayerResult<Self> {
        let context = softbuffer::Context::new(window.clone())?;
        let surface = softbuffer::Surface::new(&context, window)?;

        Ok(Self {
            surface,
            size: (0, 0),
        })
    }
}

impl WindowTarget for CpuTarget {
    fn is_gpu(&self) -> bool {
        false
    }

    fn resize(&mut self, width: u32, height: u32) -> PlayerResult<()> {
        let (Some(nonzero_width), Some(nonzero_height)) =
            (NonZeroU32::new(width), NonZeroU32::new(height))
        else {
            return Ok(());
        };

        self.surface.resize(nonzero_width, nonzero_height)?;
        self.size = (width, height);
        Ok(())
    }

    fn present(&mut self, draw: &mut dyn FnMut(&Canvas)) -> PlayerResult<()> {
        let (width, height) = self.size;
        if width == 0 || height == 0 {
            return Ok(());
        }

        let mut buffer = self.surface.buffer_mut()?;
        // BGRA in memory is `0xAARRGGBB` as a little-endian `u32`, which is what softbuffer
        // expects (it ignores the alpha byte).
        let info = ImageInfo::new(
            (width as i32, height as i32),
            ColorType::BGRA8888,
            AlphaType::Premul,
            None,
        );
        let pixels = fframes::bytemuck::cast_slice_mut::<u32, u8>(&mut buffer);
        let mut surface = surfaces::wrap_pixels(&info, pixels, width as usize * 4, None)
            .ok_or_else(|| {
                PlayerError::Skia(format!(
                    "can not draw into the {width}x{height} window buffer"
                ))
            })?;

        draw(surface.canvas());
        drop(surface);

        buffer.present()?;
        Ok(())
    }
}
