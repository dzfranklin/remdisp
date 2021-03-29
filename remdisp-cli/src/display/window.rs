use crate::prelude::*;
use sdl2::{IntegerOrSdlError, Sdl};
use sdl2::pixels::Color;
use sdl2::render::WindowCanvas;

struct Window {
    ctx: Sdl,
    canvas: WindowCanvas,
}

impl Window {
    pub fn new() -> Result<Self, SdlError> {
        let ctx = sdl2::init()
            .map_err(|err| SdlError::Init(err))?;
        let video = ctx.video()
            .map_err(|err| SdlError::InitVideo(err))?;
        let window = video.window("Remote Display", 100, 100)
            .fullscreen_desktop()
            .build()
            .map_err(|err| SdlError::BuildWindow(err))?;
        let mut canvas = window.into_canvas().build()?;

        canvas.set_draw_color(Color::RGB(255, 255, 255));
        canvas.clear();
        canvas.present();

        Ok(Window { ctx, canvas, })
    }
}

#[derive(Error, Debug)]
enum SdlError {
    #[error("Error initializing sdl: {0}")]
    Init(String),
    #[error("Error initializing sdl video subsystem: {0}")]
    InitVideo(String),
    #[error("Error building sdl window: {0:?}")]
    BuildWindow(#[from] sdl2::video::WindowBuildError),
    #[error("Sdl error: {0}")]
    Generic(String)
}

impl From<sdl2::IntegerOrSdlError> for SdlError {
    fn from(err: IntegerOrSdlError) -> Self {
        match err {
            IntegerOrSdlError::IntegerOverflows(_, _) => panic!("Sdl integer overflow"),
            IntegerOrSdlError::SdlError(err) => SdlError::Generic(err)
        }
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;

    #[ltest]
    fn can_create() {
        let _ = Window::new().unwrap();
    }
}
