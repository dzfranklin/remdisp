use std::{mem, ptr};
use std::os::raw::c_int;

use evdi::prelude::Mode;
use ffmpeg_sys_next as sys;
use tokio::io::{AsyncWrite, AsyncWriteExt};

use crate::av::{AvError, converter::{Converter, ConverterError}, ensure_av_logs_setup};
use crate::av;
use crate::prelude::*;

mod codec_options;

#[derive(Debug)]
pub struct Encoder {
    ctx: ptr::NonNull<sys::AVCodecContext>,
    pkt: ptr::NonNull<sys::AVPacket>,
    mode: Mode,
    converter: Converter,
    /// Presentation timestamp
    pts: i64,
}

/// Currently re-using after flushing not supported
impl Encoder {
    // See <https://ffmpeg.org/doxygen/4.0/group__lavc__encdec.html>
    // and <https://github.com/FFmpeg/FFmpeg/blob/master/doc/examples/encode_video.c>

    #[instrument(err)]
    pub fn new(mode: Mode) -> Result<Self, AvError> {
        ensure_av_logs_setup();

        let codec_id = sys::AVCodecID::AV_CODEC_ID_H264;

        let codec = unsafe {
            nonnull_or!(sys::avcodec_find_encoder(codec_id), AvError::CodecUnavailable(codec_id))
        }?;
        debug!(?codec_id, "Found codec");

        let mut ctx = unsafe {
            nonnull_or!(sys::avcodec_alloc_context3(codec.as_ptr()), AvError::CreateContext)
        }?;
        debug!("Found codec context");

        let supported_formats = Self::supported_formats(codec);
        let target_src_format = *supported_formats.get(0)
            .ok_or(AvError::CodecSupportsNoFormats(codec_id))?;
        debug!(?target_src_format, ?codec, ?supported_formats, "Target src format");

        unsafe {
            let ctx = ctx.as_mut();
            ctx.width = mode.width as i32;
            ctx.height = mode.height as i32;
            ctx.pix_fmt = target_src_format;
            ctx.time_base = sys::AVRational { num: 1, den: 25 };

            let options = codec_options::Options::from(ctx.priv_data);
            debug!(?options, "Options supported by codec");

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
            let status = sys::avcodec_open2(ctx.as_ptr(), codec.as_ptr(), ptr::null_mut());
            if status < 0 {
                return Err(AvError::OpenContext(status));
            }
        }
        debug!("Opened codec context");

        let pkt = unsafe {
            nonnull_or!(sys::av_packet_alloc(), AvError::AllocatePacket)
        }?;

        Ok(Self {
            ctx,
            pkt,
            mode,
            converter,
            pts: 0,
        })
    }

    /// For possible options see the CLI docs of the encoder.
    /// See <https://trac.ffmpeg.org/wiki/Encode/H.264>
    /// See also <https://superuser.com/questions/490683/cheat-sheets-and-presets-settings-that-actually-work-with-ffmpeg-1-0>
    fn set_opt(ctx: &mut sys::AVCodecContext, name: &[u8], value: &[u8]) -> Result<(), AvError> {
        unsafe {
            let status = sys::av_opt_set(
                ctx.priv_data,
                name.as_ptr().cast(),
                value.as_ptr().cast(),
                0,
            );
            if status == 0 {
                Ok(())
            } else {
                Err(AvError::ConfigureContext)
            }
        }
    }

    fn supported_formats(codec: ptr::NonNull<sys::AVCodec>) -> Vec<sys::AVPixelFormat> {
        let mut formats = vec![];
        unsafe {
            let mut head = codec.as_ref().pix_fmts;
            while !head.is_null() && *head != sys::AVPixelFormat::AV_PIX_FMT_NONE {
                formats.push(*head);
                head = head.add(1)
            }
        }
        formats
    }

    #[instrument(err, skip(bytes))]
    pub fn send_frame(&mut self, bytes: &[u8]) -> Result<(), AvError> {
        let frame = self.converter.convert(bytes);
        unsafe {
            frame.pts = self.pts;
            self.pts += 1;

            let status = sys::avcodec_send_frame(self.ctx.as_ptr(), frame);
            if status < 0 {
                return Err(AvError::SendForEncoding(status));
            }
        }
        Ok(())
    }

    #[instrument(err)]
    pub fn flush(&mut self) -> Result<(), AvError> {
        unsafe {
            // TODO: docs: it is recommended that AVPackets and AVFrames are refcounted, or libavcodec might have to copy the input dat
            // NOTE: docs: In theory, sending input can result in EAGAIN - this should happen only
            // if not all output was received. You can use this to structure alternative decode or
            // encode loops other than the one suggested above. For example, you could try sending
            // new input on each iteration, and try to receive output if that returns EAGAIN.
            // See <https://ffmpeg.org/doxygen/4.0/group__lavc__encdec.html>
            let status = sys::avcodec_send_frame(self.ctx.as_ptr(), ptr::null());
            if status < 0 {
                Err(AvError::Flush(status))
            } else {
                Ok(())
            }
        }
    }

    #[instrument(err, skip(out))]
    pub async fn receive_available<W: AsyncWrite + Unpin>(&mut self, mut out: W) -> Result<(), AvError> {
        loop {
            unsafe {
                // NOTE: I'm not sure if this receives packets in order. If not, we have a problem,
                // since we don't pass through any sort of timestamp (pts, dts, etc).
                let status = sys::avcodec_receive_packet(self.ctx.as_ptr(), self.pkt.as_ptr());
                if status == av::to_av_error(sys::EAGAIN) || status == sys::AVERROR_EOF {
                    return Ok(());
                } else if status < 0 {
                    return Err(AvError::Encode);
                }
            };

            // The docs mention something called "muxing", which is apparently writing packets to
            // a file. We don't need headers to tell the other side what sort of format, so I don't
            // think we need that?
            // See <https://ffmpeg.org/doxygen/3.2/group__lavf__encoding.html#details>

            let pkt_ref = unsafe { self.pkt.as_ref() };

            debug!(
                pts=pkt_ref.pts,
                dts=pkt_ref.dts,
                size=pkt_ref.size,
                stream_index=pkt_ref.stream_index,
                flags=pkt_ref.flags,
                duration=pkt_ref.duration,
                pos=pkt_ref.pos,
                convergence_duration=pkt_ref.convergence_duration,
                "Received packet");

            let data = unsafe {
                &*ptr::slice_from_raw_parts(pkt_ref.data, pkt_ref.size as usize)
            };

            out.write_all(data).await?;
        }
    }
}

impl Drop for Encoder {
    fn drop(&mut self) {
        unsafe {
            sys::avcodec_free_context(&mut self.ctx.as_ptr());
            sys::av_packet_free(&mut self.pkt.as_ptr());
        }
    }
}

#[cfg(test)]
pub mod tests {
    use std::{fs, io};
    use std::fs::File;
    use std::io::{Read, Write};
    use std::path::Path;

    use evdi::prelude::*;
    use lazy_static::lazy_static;

    use crate::prelude::*;

    use super::*;

    fn encoder_fixture() -> Encoder {
        let mode = mode_fixture();
        Encoder::new(mode).unwrap()
    }

    lazy_static! {
        static ref FRAMEBUFS_DIR: &'static Path = &Path::new("sample_data/evdi_framebufs");
    }

    pub(crate) fn mode_fixture() -> Mode {
        let mode_f = File::open(FRAMEBUFS_DIR.join("mode.json"))
            .expect("Do you need to run generate_sample_data?");
        serde_json::from_reader(mode_f).unwrap()
    }

    pub(crate) fn framebuf_fixture(n: u32) -> Vec<u8> {
        let mut buf = vec![];
        File::open(format!("sample_data/evdi_framebufs/{}.framebuf", n))
            .expect("Nonexistent framebuf data")
            .read_to_end(&mut buf).unwrap();
        buf
    }

    #[ltest]
    fn can_create() {
        let _encoder = encoder_fixture();
    }

    async fn encode_to<W: AsyncWrite + Unpin>(mut out: W) {
        let mut encoder = encoder_fixture();

        for iter in 0..30 {
            let n = iter % 10;
            info!("Encoding framebuf {}", n);
            let bytes = framebuf_fixture(n);
            encoder.send_frame(&bytes).unwrap();
            encoder.receive_available(&mut out).await.unwrap()
        }

        encoder.flush().unwrap();
        encoder.receive_available(&mut out).await.unwrap()
    }

    #[ltest(atest)]
    async fn encode_frames() {
        let mut out = vec![];
        encode_to(&mut out).await;
    }

    #[ignore]
    #[ltest(atest)]
    async fn output_video_to_file_for_manual_check() {
        let mut out = tokio::fs::File::create("TEMP_video.h264").await.unwrap();
        encode_to(&mut out).await;
    }

    #[ignore]
    #[ltest(atest)]
    async fn generate_sample_h264() {
        let mut out = tokio::fs::File::create("sample_data/sample.h264").await.unwrap();
        encode_to(&mut out).await;
    }

    #[ignore]
    #[ltest(atest)]
    async fn generate_sample_framebufs() {
        let config = DeviceConfig::sample();
        let mut handle = DeviceNode::get().unwrap().open().unwrap().connect(&config);
        let mode = handle.events.await_mode(TIMEOUT).await.unwrap();
        let buf_id = handle.new_buffer(&mode);

        if let Err(err) = fs::create_dir(*FRAMEBUFS_DIR) {
            if err.kind() != io::ErrorKind::AlreadyExists {
                Err(err).unwrap()
            }
        }

        let mode_data = serde_json::to_vec(&mode).unwrap();
        File::create(FRAMEBUFS_DIR.join("mode.json")).unwrap().write_all(&mode_data).unwrap();

        for _ in 0..200 {
            handle.request_update(buf_id, TIMEOUT).await.unwrap();
        }

        for n in 0..10 {
            handle.request_update(buf_id, TIMEOUT).await.unwrap();
            let mut f = File::create(FRAMEBUFS_DIR.join(format!("{}.framebuf", n))).unwrap();
            f.write_all(handle.get_buffer(buf_id).unwrap().bytes()).unwrap();
        }
    }
}
