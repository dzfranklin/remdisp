use std::{io, mem, ptr};
use std::io::Write;
use std::os::raw::c_int;

use evdi::prelude::Mode;
use ffmpeg_sys_next as av;
use ffmpeg_sys_next::EAGAIN;
use tokio::io::{AsyncWrite, AsyncWriteExt};

use converter::{Converter, ConverterError};

use crate::prelude::*;

mod converter;
mod codec_options;

#[derive(Debug)]
pub struct Encoder {
    ctx: ptr::NonNull<av::AVCodecContext>,
    pkt: ptr::NonNull<av::AVPacket>,
    mode: Mode,
    converter: Converter,
}

impl Encoder {
    // See <https://ffmpeg.org/doxygen/4.0/group__lavc__encdec.html>
    // and <https://github.com/FFmpeg/FFmpeg/blob/master/doc/examples/encode_video.c>

    #[instrument]
    pub fn new(mode: Mode) -> Result<Self, EncoderError> {
        ensure_av_logs_setup();

        let codec_id = av::AVCodecID::AV_CODEC_ID_HEVC;

        let codec = unsafe {
            let codec = av::avcodec_find_encoder(codec_id);
            ptr::NonNull::new(codec).ok_or(EncoderError::CodecUnavailable(codec_id))
        }?;
        debug!(?codec_id, "Found codec");

        let mut ctx = unsafe {
            let ctx = av::avcodec_alloc_context3(codec.as_ptr());
            ptr::NonNull::new(ctx).ok_or(EncoderError::CreateContext)
        }?;
        debug!("Found codec context");

        let supported_formats = Self::supported_formats(codec);
        let target_src_format = *supported_formats.get(0)
            .ok_or(EncoderError::CodecSupportsNoFormats(codec_id))?;
        debug!(?target_src_format, ?codec, ?supported_formats, "Target src format");

        unsafe {
            let ctx = ctx.as_mut();
            ctx.width = mode.width as i32;
            ctx.height = mode.height as i32;
            ctx.pix_fmt = target_src_format;
            ctx.time_base = av::AVRational { num: 1, den: 25 };
            ctx.framerate = av::AVRational { num: 25, den: 1 };

            let options = codec_options::Options::from(ctx.priv_data);
            debug!(?options, "Options supported by codec");

            // Disable most logs because there is no way to prevent them from going to stdout
            Self::set_opt(ctx, b"x265-params\0", b"log-level=warning\0")?;

            // Tuning
            // On the quality - speed - size tradeoff we pick high quality high size

            // Number of frames between I-frames. We set to very high because we never need to seek
            ctx.gop_size = 100;
            ctx.max_b_frames = 1;
            Self::set_opt(ctx, b"preset\0", b"ultrafast\0")?;
            // Constant rate factor, i.e. quality (0..51.0, lower better quality)
            Self::set_opt(ctx, b"crf\0", b"0\0")?;
        }
        debug!("Configured codec context");

        let converter = Converter::new(mode, target_src_format)?;

        unsafe {
            let status = av::avcodec_open2(ctx.as_ptr(), codec.as_ptr(), ptr::null_mut());
            if status < 0 {
                return Err(EncoderError::OpenContext(status));
            }
        }
        debug!("Opened codec context");

        let pkt = unsafe {
            let pkt = av::av_packet_alloc();
            ptr::NonNull::new(pkt).ok_or(EncoderError::AllocatePacket)
        }?;

        Ok(Self {
            ctx,
            pkt,
            mode,
            converter,
        })
    }

    /// For possible options see the CLI docs of the encoder.
    /// For hevc: https://x265.readthedocs.io/en/latest/cli.html
    fn set_opt(ctx: &mut av::AVCodecContext, name: &[u8], value: &[u8]) -> Result<(), EncoderError> {
        unsafe {
            let status = av::av_opt_set(
                ctx.priv_data,
                name.as_ptr().cast(),
                value.as_ptr().cast(),
                0,
            );
            if status == 0 {
                Ok(())
            } else {
                Err(EncoderError::ConfigureContext)
            }
        }
    }

    fn supported_formats(codec: ptr::NonNull<av::AVCodec>) -> Vec<av::AVPixelFormat> {
        let mut formats = vec![];
        unsafe {
            let mut head = codec.as_ref().pix_fmts;
            while !head.is_null() && *head != av::AVPixelFormat::AV_PIX_FMT_NONE {
                formats.push(*head);
                head = head.add(mem::size_of::<*const av::AVPixelFormat>())
            }
        }
        formats
    }

    #[instrument(skip(bytes))]
    pub fn send_frame(&mut self, bytes: &[u8]) -> Result<(), EncoderError> {
        let frame = self.converter.convert(bytes);
        unsafe {
            // Presentation time stamp
            frame.pts += 1;

            let status = av::avcodec_send_frame(self.ctx.as_ptr(), frame);
            if status < 0 {
                return Err(EncoderError::SendForEncoding(status));
            }
        }
        Ok(())
    }

    #[instrument]
    pub fn flush(&mut self) -> Result<(), EncoderError> {
        unsafe {
            // TODO: docs: it is recommended that AVPackets and AVFrames are refcounted, or libavcodec might have to copy the input dat
            // NOTE: docs: In theory, sending input can result in EAGAIN - this should happen only
            // if not all output was received. You can use this to structure alternative decode or
            // encode loops other than the one suggested above. For example, you could try sending
            // new input on each iteration, and try to receive output if that returns EAGAIN.
            // See <https://ffmpeg.org/doxygen/4.0/group__lavc__encdec.html>
            let status = av::avcodec_send_frame(self.ctx.as_ptr(), ptr::null());
            if status < 0 {
                Err(EncoderError::Flush(status))
            } else {
                Ok(())
            }
        }
    }

    #[instrument(skip(out))]
    pub async fn receive_available<W: AsyncWrite + Unpin>(&mut self, mut out: W) -> Result<(), EncoderError> {
        loop {
            unsafe {
                let status = av::avcodec_receive_packet(self.ctx.as_ptr(), self.pkt.as_ptr());
                if status == av_error(EAGAIN) || status == av::AVERROR_EOF {
                    return Ok(());
                } else if status < 0 {
                    return Err(EncoderError::Encode);
                }
            };

            let data = unsafe {
                let pkt = self.pkt.as_ref();
                &*ptr::slice_from_raw_parts(pkt.data, pkt.size as usize)
            };

            out.write_all(data).await?;
        }
    }
}

impl Drop for Encoder {
    fn drop(&mut self) {
        unsafe {
            av::avcodec_free_context(&mut self.ctx.as_ptr());
            av::av_packet_free(&mut self.pkt.as_ptr());
        }
    }
}

/// Copy of the macro AVERROR
fn av_error(i: c_int) -> c_int {
    -i
}

#[derive(Debug, Error)]
pub enum EncoderError {
    #[error("Required codec {0:?} not available, check your ffmpeg installation")]
    CodecUnavailable(av::AVCodecID),
    #[error("Codec {0:?} supports no pixel formats")]
    CodecSupportsNoFormats(av::AVCodecID),
    #[error("Failed to allocate and create encoding context")]
    CreateContext,
    #[error("Failed to configure context")]
    ConfigureContext,
    #[error("Failed to open context: AV_ERROR {0}")]
    OpenContext(i32),
    #[error("Failed to allocate packet")]
    AllocatePacket,
    #[error("Failed to convert to the source format of the encoder")]
    FormatConversion(#[from] ConverterError),
    #[error("Failed to send frame for encoding: AV_ERROR {0}")]
    SendForEncoding(i32),
    #[error("Failed to encode or receive packet")]
    Encode,
    #[error("Failed to flush encoder: AV_ERROR {0}")]
    Flush(i32),
    #[error("Failed to write data")]
    Write(#[from] io::Error),
}

#[cfg(test)]
pub mod tests {
    use evdi::prelude::*;

    use crate::data_plane::tests::{framebuf_fixture, mode_fixture};
    use crate::prelude::*;

    use super::*;

    fn encoder_fixture() -> Encoder {
        let mode = mode_fixture();
        Encoder::new(mode).unwrap()
    }

    #[ltest]
    fn can_create() {
        let _encoder = encoder_fixture();
    }

    #[ltest(atest)]
    async fn encode_frames() {
        let mut encoder = encoder_fixture();
        let mut out = vec![];

        for n in 0..9 {
            debug!("Encoding framebuf {}", n);
            let bytes = framebuf_fixture(n);
            encoder.send_frame(&bytes).unwrap();
            encoder.receive_available(&mut out).await.unwrap()
        }

        encoder.flush().unwrap();
        encoder.receive_available(&mut out).await.unwrap()
    }

    #[ignore]
    #[ltest(atest)]
    async fn output_video_to_file_for_manual_check() {
        let mut encoder = encoder_fixture();
        let mut out = tokio::fs::File::create("TEMP_video.hevc").await.unwrap();

        for n in 0..9 {
            debug!("Encoding framebuf {}", n);
            let bytes = framebuf_fixture(n);
            encoder.send_frame(&bytes).unwrap();
            encoder.receive_available(&mut out).await.unwrap()
        }

        encoder.flush().unwrap();
        encoder.receive_available(&mut out).await.unwrap()
    }
}
