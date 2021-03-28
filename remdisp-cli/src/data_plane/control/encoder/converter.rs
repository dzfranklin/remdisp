use std::{mem, ptr};
use std::ops::BitAnd;
use std::os::raw::c_int;
use std::ptr::NonNull;

use evdi::prelude::{DrmFormat, Mode, UnrecognizedFourcc};
use ffmpeg_sys_next as av;
use thiserror::Error;

use crate::prelude::*;

const ALIGNMENT: i32 = 32;

#[derive(Derivative)]
#[derivative(Debug)]
pub struct Converter {
    ctx: NonNull<av::SwsContext>,
    #[derivative(Debug = "ignore")]
    src_frame: NonNull<av::AVFrame>,
    #[derivative(Debug = "ignore")]
    dst_frame: NonNull<av::AVFrame>,
    src_format: av::AVPixelFormat,
    dst_format: av::AVPixelFormat,
    #[derivative(Debug = "ignore")]
    dst_buf: Box<[u8]>,
    width: i32,
    height: i32,
    src_stride: i32,
}

/// Convert raw buffers into a single-plane format
impl Converter {
    /// Only src formats with a single plane are supported
    #[instrument(skip(src))]
    pub fn new(src: Mode, dst: av::AVPixelFormat) -> Result<Self, ConverterError> {
        ensure_av_logs_setup();

        let width = src.width as i32;
        let height = src.height as i32;
        let src_stride = src.stride() as i32;
        let format = src.pixel_format
            .map_err(|err| ConverterError::UnrecognizedDrmFormat(err))?;
        let src = Self::pixel_format_for(format)?;

        unsafe {
            if av::sws_isSupportedInput(src) == 0 {
                return Err(ConverterError::UnsupportedSrcFormat(src));
            }

            if (*av::av_pix_fmt_desc_get(src)).flags.bitand(av::AV_PIX_FMT_FLAG_PLANAR as u64) != 0 {
                return Err(ConverterError::PlanarSrc(src));
            }

            if av::sws_isSupportedOutput(dst) == 0 {
                return Err(ConverterError::UnsupportedDstFormat(dst));
            }
        }

        let ctx = unsafe {
            let ptr = av::sws_getContext(
                width,
                height,
                src,
                width,
                height,
                dst,
                // Chosen based on vibe from <http://prog3.com/sbdm/blog/aoshilang2249/article/details/40347457>
                av::SWS_FAST_BILINEAR,
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null_mut(),
            );
            NonNull::new(ptr).ok_or(ConverterError::CreateContext)?
        };

        let mut src_frame = unsafe {
            let ptr = av::av_frame_alloc();
            NonNull::new(ptr).ok_or(ConverterError::AllocFrame)?
        };

        let mut dst_frame = unsafe {
            let ptr = av::av_frame_alloc();
            NonNull::new(ptr).ok_or(ConverterError::AllocFrame)?
        };

        let mut dst_buf = unsafe {
            let size = av::av_image_get_buffer_size(dst, width, height, ALIGNMENT);
            vec![0u8; size as usize].into_boxed_slice()
        };

        unsafe {
            let frame = dst_frame.as_mut();

            frame.width = width;
            frame.height = height;
            frame.format = dst as i32;

            av::av_image_fill_arrays(
                frame.data.as_mut_ptr(),
                frame.linesize.as_mut_ptr(),
                dst_buf.as_mut_ptr(),
                dst,
                frame.width,
                frame.height,
                ALIGNMENT,
            );
        }

        unsafe {
            let frame = src_frame.as_mut();
            frame.width = width;
            frame.height = height;
            frame.format = src as i32;
        }

        Ok(Self { ctx, src_frame, dst_frame, dst_buf, src_format: src, dst_format: dst, width, height, src_stride })
    }

    /// Caller should not change width, height, format, data, or linesize of frame.
    #[instrument(skip(src))]
    pub fn convert(&mut self, src: &[u8]) -> &mut av::AVFrame {
        assert_eq!(src.len(), (self.src_stride * self.height) as usize, "Invalid src length");

        unsafe {
            let src_frame = self.src_frame.as_mut();
            let dst_frame = self.dst_frame.as_mut();

            src_frame.linesize[0] = self.src_stride;
            src_frame.data[0] = src.as_ptr() as *mut _;

            let output_height = av::sws_scale(
                self.ctx.as_ptr(),
                src_frame.data.as_ptr().cast(),
                src_frame.linesize.as_ptr(),
                0, // Start at the start of the image
                self.height,
                dst_frame.data.as_ptr(),
                dst_frame.linesize.as_ptr(),
            );
            assert_eq!(output_height, self.height, "sws_scale returned unexpected output height");
        }

        unsafe { self.dst_frame.as_mut() }
    }

    // Use https://github.com/FFmpeg/FFmpeg/blob/069d2b4a50a6eb2f925f36884e6b9bd9a1e54670/libavdevice/fbdev_common.c#L48
    fn pixel_format_for(format: DrmFormat) -> Result<av::AVPixelFormat, ConverterError> {
        // TODO: Support more
        match format {
            DrmFormat::Xrgb8888 => Ok(av::AV_PIX_FMT_0RGB32),
            _ => {
                Err(ConverterError::UnsupportedDrmFormat(format))
            }
        }
    }
}

impl Drop for Converter {
    fn drop(&mut self) {
        unsafe {
            av::sws_freeContext(self.ctx.as_ptr());
            // NOTE: We don't free the frames because all ffmpeg does in that case is free their
            // data, which we allocate in rust.
            // See <https://github.com/FFmpeg/FFmpeg/blob/069d2b4a50a6eb2f925f36884e6b9bd9a1e54670/libavcodec/avpicture.c#L70>
        }
    }
}

#[derive(Error, Debug)]
pub enum ConverterError {
    #[error("Drm format {0:?} not supported")]
    UnsupportedDrmFormat(DrmFormat),
    #[error(transparent)]
    UnrecognizedDrmFormat(UnrecognizedFourcc),
    #[error("Your build of ffmpeg doesn't support support {0:?} as a source format for sws")]
    UnsupportedSrcFormat(av::AVPixelFormat),
    #[error("Planar src formats {0:?} aren't supported")]
    PlanarSrc(av::AVPixelFormat),
    #[error("Your build of ffmpeg doesn't support support {0:?} as a destination format for sws")]
    UnsupportedDstFormat(av::AVPixelFormat),
    #[error("Failed create SwsContext")]
    CreateContext,
    #[error("Failed to allocate frame")]
    AllocFrame,
}

#[cfg(test)]
pub mod tests {
    use std::fs::File;
    use std::io::Write;
    use std::slice;

    use evdi::DrmFormat;
    use evdi::prelude::Mode;
    use ffmpeg_sys_next as ffi;

    use crate::data_plane::tests::{framebuf_fixture, mode_fixture};
    use crate::prelude::*;

    use super::*;

    #[ltest]
    fn can_create() {
        Converter::new(mode_fixture(), av::AV_PIX_FMT_YUV420P16).unwrap();
    }

    fn converter_fixture(mode: Mode, dst: av::AVPixelFormat) -> Converter {
        Converter::new(mode, dst).unwrap()
    }

    #[ltest]
    fn can_convert_once() {
        let mode = mode_fixture();
        let mut converter = converter_fixture(mode, av::AVPixelFormat::AV_PIX_FMT_YUV410P);

        let src = framebuf_fixture(0);
        let _dst = converter.convert(&src);
    }

    #[ltest]
    fn can_convert_multiple_times() {
        let mode = mode_fixture();
        let mut converter = converter_fixture(mode, av::AVPixelFormat::AV_PIX_FMT_YUV420P);

        for n in 0..9 {
            let src = framebuf_fixture(n);
            let _dst = converter.convert(&src);
        }
    }

    #[ignore]
    #[ltest]
    fn output_yuv_to_file_for_manual_checks() {
        let mode = mode_fixture();
        let mut converter = converter_fixture(mode, av::AVPixelFormat::AV_PIX_FMT_YUV420P);

        let src = framebuf_fixture(0);
        let dst = converter.convert(&src);

        let mut out = File::create("TEMP_out.yuv").unwrap();

        let y_area = mode.height as usize * dst.linesize[0] as usize;
        let y_plane = unsafe { slice::from_raw_parts(dst.data[0], y_area) };
        out.write_all(y_plane).unwrap();

        let uv_area = mode.height as usize * dst.linesize[1] as usize;
        let uv_plane = unsafe { slice::from_raw_parts(dst.data[1], uv_area) };
        out.write_all(uv_plane).unwrap();

        panic!("Manually check with `ffplay -f rawvideo -pixel_format yuv420p -video_size {}x{} -i TEMP_out.yuv`", mode.width, mode.height);
    }
}
