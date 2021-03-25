use anyhow::{Context, Result, anyhow}; // TODO: Replace with proper errors
use evdi::prelude::*;
use ffmpeg_sys_next::*;
use std::ffi::CString;
use std::ptr;
use tokio::net::TcpStream;
use tokio::io::AsyncWriteExt;
use std::os::raw::c_int;

// TODO: Use multiple buffers

pub struct Encoder {
    ctx: *mut AVCodecContext,
    frame: *mut AVFrame,
    pkt: *mut AVPacket,
    mode: Mode,
}

impl Encoder {
    pub fn new(mode: Mode) -> Result<Self> {
        // NOTE: Based on <https://github.com/FFmpeg/FFmpeg/blob/master/doc/examples/encode_video.c>

        // Use NonNull

        let codec_id = AVCodecID::AV_CODEC_ID_HEVC;
        let codec = unsafe {
            let codec = avcodec_find_encoder(codec_id);
            if codec.is_null() {
                None
            } else {
                Some(codec)
            }
        }.with_context(|| format!("Required codec {:?} not available, check your ffmpeg installation", codec_id))?;

        let ctx = unsafe {
            let ctx = avcodec_alloc_context3(codec);
            if ctx.is_null() {
                None
            } else {
                Some(ctx)
            }
        }.context("Failed to allocate context")?;

        let pixel_format = {
            pixel_format_from_fourcc(&mode.pixel_format?)
                .context("Unsupported pixel format")?
        };

        unsafe {
            (*ctx).width = mode.width as i32;
            (*ctx).height = mode.height as i32;
            (*ctx).time_base = AVRational { num: 1, den: 25 };
            (*ctx).framerate = AVRational { num: 25, den: 1 };
            // Number of frames between I-frames. We set to very high because we never need to seek
            (*ctx).gop_size = 100;
            (*ctx).max_b_frames = 1;
            (*ctx).pix_fmt = pixel_format;

            // For more options see <https://ffmpeg.org/doxygen/trunk/group__lavc__core.html#ga11f785a188d7d9df71621001465b0f1d>
            if av_opt_set(
                (*ctx).priv_data,
                CString::new("preset")?.as_ptr(),
                CString::new("high")?.as_ptr(),
                0,
            ) != 0 {
                return Err(anyhow!("Failed to set opt"));
            }
        }

        unsafe {
            if avcodec_open2(ctx, codec, ptr::null_mut()) < 0 {
                return Err(anyhow!("Failed to open codec"));
            }
        }

        // TODO: Wrap evdi buf instead
        // See <https://stackoverflow.com/a/51423289>
        let frame = unsafe {
            let frame = av_frame_alloc();
            if frame.is_null() {
                None
            } else {
                Some(frame)
            }
        }.context("Failed to allocate frame")?;

        unsafe {
            (*frame).format = pixel_format as i32;
            (*frame).width = (*ctx).width;
            (*frame).height = (*ctx).height;
        }

        unsafe {
            if av_frame_get_buffer(frame, 0) < 0 {
                return Err(anyhow!("Could not allocate video frame data"));
            }
        }

        let pkt = unsafe {
            let pkt = av_packet_alloc();
            if pkt.is_null() {
                None
            } else {
                Some(pkt)
            }
        }.context("Failed to allocate packet")?;

        Ok(Self {
            ctx,
            frame,
            pkt,
            mode,
        })
    }

    pub async fn encode_frame_to(&mut self, buf: &Buffer, out: &mut TcpStream) -> Result<()> {
        unsafe {
            let depth = self.mode.bits_per_pixel as usize;
            let mut planes: Vec<&mut [u8]> = Vec::with_capacity(depth);
            let mut plane_strides: Vec<usize> = Vec::with_capacity(depth);
            for i in 0..depth {
                planes.push(&mut *ptr::slice_from_raw_parts_mut(
                    (*self.frame).data[i],
                    (*self.frame).linesize[i] as usize * buf.height,
                ));

                plane_strides.push((*self.frame).linesize[i] as usize);
            }

            // TODO: Set up buffers so we don't need to do all this converting
            for (y, row_with_junk) in buf.bytes().chunks_exact(buf.stride).enumerate() {
                let row = &row_with_junk[0..buf.width];
                for (x, pixel) in row.chunks_exact(depth).enumerate() {
                    for plane_i in 0..depth {
                        planes[plane_i][y * plane_strides[plane_i] + x] = pixel[plane_i];
                    }
                }
            }

            // Presentation time stamp
            (*self.frame).pts += 1;

            if avcodec_send_frame(self.ctx, self.frame) < 0 {
                return Err(anyhow!("Error sending frame for encoding"));
            }

            loop {
                let ret = avcodec_receive_packet(self.ctx, self.pkt);
                if ret == av_error(EAGAIN) || ret == AVERROR_EOF {
                    return Ok(());
                } else if ret < 0 {
                    return Err(anyhow!("Error during encoding"));
                }

                let data = &*ptr::slice_from_raw_parts((*self.pkt).data, (*self.pkt).size as usize);
                out.write_all(data).await?;
                av_packet_unref(self.pkt);
            }
        }
    }
}

impl Drop for Encoder {
    fn drop(&mut self) {
        unsafe {
            avcodec_free_context(&mut self.ctx as _);
            av_frame_free(&mut self.frame as _);
            av_packet_free(&mut self.pkt as _);
        }
    }
}

fn pixel_format_from_fourcc(format: &DrmFormat) -> Option<AVPixelFormat> {
    // TODO: Support more
    match format {
        DrmFormat::Xbgr8888 => Some(AV_PIX_FMT_0BGR32),
        _ => None
    }
}

/// Copy of the macro AVERROR
fn av_error(i: c_int) -> c_int {
    -i
}

#[cfg(test)]
pub mod tests {
    use crate::prelude::*;
    use evdi::prelude::*;
    use std::fs::File;
    use std::{fs, io};
    use std::path::Path;
    use std::io::{Write, Error};

    #[ignore]
    #[ltest(atest)]
    async fn generate_sample_data() {
        let config = DeviceConfig::sample();
        let mut handle = DeviceNode::get().unwrap().open().unwrap().connect(&config);
        let mode = handle.events.await_mode(TIMEOUT).await.unwrap();
        let buf_id = handle.new_buffer(&mode);

        for _ in 0..200 {
           handle.request_update(buf_id, TIMEOUT).await.unwrap();
        }

        let dir_name = format!("sample_data/framebufs_{}x{}", config.width_pixels, config.height_pixels);
        let dir = Path::new(&dir_name);

        if let Err(err) = fs::create_dir(dir) {
            if err.kind() != io::ErrorKind::AlreadyExists {
                Err(err).unwrap()
            }
        }

        for n in 0..10 {
            handle.request_update(buf_id, TIMEOUT).await.unwrap();
            let mut f = File::create(dir.join(format!("{}.framebuf", n))).unwrap();
            f.write_all(handle.get_buffer(buf_id).unwrap().bytes()).unwrap();
        }
    }
}
