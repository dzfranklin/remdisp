use sdl2::render::TextureValueError;
use sdl2::video::WindowBuildError;
use sdl2::IntegerOrSdlError;

use crate::av::yuv_frame::YuvFrame;
use crate::display::info::DisplayInfo;
use crate::prelude::*;
use std::fmt::{Debug, Formatter};

/// Permitted flow
/// - create
/// - zero or more update
/// - either Drop, or close and restart at create
pub trait Window: Debug {
    /// Do any initialization needed and display the window to the user.
    fn create(&mut self) -> Result<DisplayInfo, WindowError>;

    /// Update the window with new pixels. Must be called after create.
    fn update(&mut self, frame: YuvFrame) -> Result<(), WindowError>;

    fn close(&mut self) -> Result<(), WindowError>;
}

pub struct SdlWindow {
    created: Option<CreatedSdlWindow>,
}

#[derive(Error, Debug)]
pub enum WindowError {
    #[error("Sdl error: {0}")]
    Sdl(String),
}

struct CreatedSdlWindow {
    ctx: sdl2::Sdl,
    video: sdl2::VideoSubsystem,
    canvas: sdl2::render::WindowCanvas,
    texture: sdl2::render::Texture,
    pixel_buf: Vec<u8>,
}

impl SdlWindow {
    pub fn new() -> Self {
        Self { created: None }
    }

    fn expect_created(&mut self) -> &mut CreatedSdlWindow {
        self.created
            .as_mut()
            .expect("Must create window before calling update")
    }
}

impl Default for SdlWindow {
    fn default() -> Self {
        Self::new()
    }
}

impl Window for SdlWindow {
    // Inspired by <https://github.com/FFmpeg/FFmpeg/blob/master/fftools/ffplay.c>
    // and <http://slouken.blogspot.com/2011/02/streaming-textures-with-sdl-13.html>
    // and <https://github.com/libsdl-org/SDL/blob/main/test/teststreaming.c>

    fn create(&mut self) -> Result<DisplayInfo, WindowError> {
        assert!(self.created.is_none(), "Already created");

        let ctx = sdl2::init()?;
        let video = ctx.video()?;

        let num_displays = video.num_video_displays()?;
        info!("{} displays connected", num_displays);

        let bounds = video.display_bounds(0)?;
        let width = bounds.w as u32;
        let height = bounds.h as u32;

        let mut window = video
            .window("Remote Display", 0, 0)
            .fullscreen_desktop()
            .build()?;
        window.show();

        let mut canvas = window.into_canvas().build()?;
        canvas.set_logical_size(width, height)?;

        // Format corresponds to AV YUV240P
        // See <https://github.com/FFmpeg/FFmpeg/blob/master/fftools/ffplay.c#L391>
        let format = sdl2::pixels::PixelFormatEnum::IYUV;

        let texture_creator = canvas.texture_creator();
        let texture = texture_creator.create_texture(
            Some(format),
            sdl2::render::TextureAccess::Streaming,
            width,
            height,
        )?;

        self.created = Some(CreatedSdlWindow {
            ctx,
            video,
            canvas,
            texture,
            pixel_buf: Vec::new(),
        });

        Ok(DisplayInfo {
            edid: vec![],
            width_pixels: width,
            height_pixels: height,
        })
    }

    fn update(&mut self, frame: YuvFrame) -> Result<(), WindowError> {
        let this = self.expect_created();

        let uv_pitch = frame.uv_linesize;
        // let uv_pitch = frame.uv_linesize * 2;

        this.texture.update_yuv(
            None,
            &frame.y,
            frame.y_linesize,
            &frame.u,
            uv_pitch,
            &frame.v,
            uv_pitch,
        )?;

        this.canvas.clear();
        this.canvas.copy(&this.texture, None, None)?; // None means entire window
        this.canvas.present();

        Ok(())
    }

    fn close(&mut self) -> Result<(), WindowError> {
        // Dropping closes window
        self.created.take().expect("Must be created to destroy");
        Ok(())
    }
}

impl Debug for SdlWindow {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SdlWindow").finish_non_exhaustive()
    }
}

impl WindowError {
    fn overflow_panic() -> ! {
        panic!("Integer overflow converting for sdl ffi")
    }
}

impl From<WindowError> for tonic::Status {
    fn from(err: WindowError) -> Self {
        tonic::Status::unavailable(format!("{:?}", err))
    }
}

impl From<String> for WindowError {
    fn from(sdl_err: String) -> Self {
        WindowError::Sdl(sdl_err)
    }
}

impl From<sdl2::render::UpdateTextureYUVError> for WindowError {
    fn from(err: sdl2::render::UpdateTextureYUVError) -> Self {
        Self::Sdl(format!("{}", err))
    }
}

impl From<sdl2::video::WindowBuildError> for WindowError {
    fn from(err: WindowBuildError) -> Self {
        match err {
            WindowBuildError::HeightOverflows(_) | WindowBuildError::WidthOverflows(_) => {
                Self::overflow_panic()
            }
            WindowBuildError::InvalidTitle(_) => panic!("Null in requested window title"),
            WindowBuildError::SdlError(msg) => Self::Sdl(msg),
        }
    }
}

impl From<sdl2::IntegerOrSdlError> for WindowError {
    fn from(err: IntegerOrSdlError) -> Self {
        match err {
            IntegerOrSdlError::IntegerOverflows(_, _) => Self::overflow_panic(),
            IntegerOrSdlError::SdlError(msg) => Self::Sdl(msg),
        }
    }
}

impl From<sdl2::render::TextureValueError> for WindowError {
    fn from(err: TextureValueError) -> Self {
        match err {
            TextureValueError::WidthOverflows(_) | TextureValueError::HeightOverflows(_) => {
                Self::overflow_panic()
            }
            TextureValueError::WidthMustBeMultipleOfTwoForFormat(_, _) => {
                WindowError::Sdl("Width must be multiple of two for format".to_string())
            }
            TextureValueError::SdlError(msg) => Self::Sdl(msg),
        }
    }
}
