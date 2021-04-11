use std::os::raw::c_int;
use std::{io, ptr};

use ffmpeg_sys_next as sys;
use ffmpeg_sys_next::avcodec_receive_frame;
use tokio::io::{AsyncRead, AsyncReadExt};

use crate::av::yuv_frame::YuvFrame;
use crate::av::{ensure_av_logs_setup, to_av_error, AvError};
use crate::prelude::*;

#[derive(Derivative)]
#[derivative(Debug)]
pub struct Decoder {
    ctx: ptr::NonNull<sys::AVCodecContext>,
    parser: ptr::NonNull<sys::AVCodecParserContext>,
    frame: ptr::NonNull<sys::AVFrame>,
    pkt: ptr::NonNull<sys::AVPacket>,
}

// TODO: Impl debug that looks inside, also for others

/// Currently re-using after flushing not supported
impl Decoder {
    // See <https://www.ffmpeg.org/doxygen/4.0/decode__video_8c_source.html>
    // and <https://github.com/FFmpeg/FFmpeg/blob/master/doc/examples/decode_video.c>

    // Magic number copied from ffmpeg example
    const RECV_BUF_SIZE: usize = 4096;

    #[instrument(err)]
    pub fn new() -> Result<Self, AvError> {
        ensure_av_logs_setup();

        // TODO: Negotiate in hello
        let codec_id = sys::AVCodecID::AV_CODEC_ID_H264;

        let parser = unsafe {
            nonnull_or!(
                sys::av_parser_init(codec_id as c_int),
                AvError::CreateParser
            )
        }?;
        let codec = unsafe {
            nonnull_or!(
                sys::avcodec_find_decoder(codec_id),
                AvError::CodecUnavailable(codec_id)
            )
        }?;
        let ctx = unsafe {
            nonnull_or!(
                sys::avcodec_alloc_context3(codec.as_ptr()),
                AvError::CreateContext
            )
        }?;

        unsafe {
            let status = sys::avcodec_open2(ctx.as_ptr(), codec.as_ptr(), ptr::null_mut());
            if status < 0 {
                return Err(AvError::OpenContext(status));
            }
        }

        let frame = unsafe { nonnull_or!(sys::av_frame_alloc(), AvError::AllocateFrame) }?;

        let pkt = unsafe { nonnull_or!(sys::av_packet_alloc(), AvError::AllocatePacket) }?;

        Ok(Self {
            ctx,
            parser,
            frame,
            pkt,
        })
    }

    #[instrument(err, skip(input, on_frame))]
    pub async fn decode<R, Cb>(&mut self, mut input: R, mut on_frame: Cb) -> Result<(), DecodeError>
    where
        R: AsyncRead + Unpin,
        Cb: for<'a> FnMut(YuvFrame<'a>),
    {
        let mut recv_buf =
            vec![0u8; Self::RECV_BUF_SIZE + sys::AV_INPUT_BUFFER_PADDING_SIZE as usize];

        loop {
            let mut unparsed_start = 0;
            let mut unparsed_size = input.read(&mut recv_buf).await?;
            if unparsed_size == 0 {
                debug!("Nothing more to read, flushing");
                return self.flush(&mut on_frame);
            }

            while unparsed_size > 0 {
                let unparsed_data = &recv_buf[unparsed_start..unparsed_start + unparsed_size];
                let parsed_size = self.parse_into_pkt(&unparsed_data)?;

                if parsed_size == 0 {
                    return Err(DecodeError::ParsedNoData);
                }

                unparsed_start += parsed_size as usize;
                unparsed_size -= parsed_size as usize;

                let pkt_ref = unsafe { self.pkt.as_ref() };
                if pkt_ref.size <= 0 {
                    // Haven't parsed a full packet yet
                    continue;
                }

                debug!(
                    pts = pkt_ref.pts,
                    dts = pkt_ref.dts,
                    size = pkt_ref.size,
                    stream_index = pkt_ref.stream_index,
                    flags = pkt_ref.flags,
                    duration = pkt_ref.duration,
                    pos = pkt_ref.pos,
                    convergence_duration = pkt_ref.convergence_duration,
                    "Parsed packet"
                );

                self.send_for_decoding(self.pkt.as_ptr())?;
                debug!("Sent packet for decoding");

                self.receive_until_empty(&mut on_frame)?;
            }
        }
    }

    /// Returns if a full packet has been parsed
    #[instrument(err, skip(data))]
    fn parse_into_pkt(&mut self, data: &[u8]) -> Result<usize, DecodeError> {
        let pkt_ref = unsafe { self.pkt.as_mut() };

        let ret = unsafe {
            sys::av_parser_parse2(
                self.parser.as_ptr(),
                self.ctx.as_ptr(),
                &mut pkt_ref.data,
                &mut pkt_ref.size,
                data.as_ptr(),
                data.len() as c_int,
                sys::AV_NOPTS_VALUE,
                sys::AV_NOPTS_VALUE,
                0,
            )
        };
        // Ret is either the number of bytes consumed or a negative status code
        if ret < 0 {
            return Err(AvError::ParseToPacket(ret).into());
        }

        Ok(ret as usize)
    }

    #[instrument(err, skip(on_frame))]
    fn flush<Cb>(&mut self, mut on_frame: Cb) -> Result<(), DecodeError>
    where
        Cb: for<'a> FnMut(YuvFrame<'a>),
    {
        self.send_for_decoding(ptr::null_mut())?;

        self.receive_until_empty(&mut on_frame)
    }

    #[instrument(err)]
    fn send_for_decoding(&mut self, pkt: *const sys::AVPacket) -> Result<(), DecodeError> {
        let ret = unsafe { sys::avcodec_send_packet(self.ctx.as_ptr(), pkt) };
        if ret < 0 {
            Err(AvError::SendForDecoding(ret).into())
        } else {
            Ok(())
        }
    }

    #[instrument(err, skip(on_frame))]
    fn receive_until_empty<Cb>(&mut self, mut on_frame: Cb) -> Result<(), DecodeError>
    where
        Cb: for<'a> FnMut(YuvFrame<'a>),
    {
        loop {
            let ret = unsafe { avcodec_receive_frame(self.ctx.as_ptr(), self.frame.as_ptr()) };
            if ret == to_av_error(sys::EAGAIN) {
                debug!("Got EAGAIN from decoder");
                return Ok(());
            } else if ret == sys::AVERROR_EOF {
                debug!("Got EOF from decoder");
                return Ok(());
            } else if ret < 0 {
                return Err(AvError::InDecoding(ret).into());
            } else {
                //  Safety: We mark in the type system that frame is derived from &self, which
                //   means it can't outlive the Drop where we free it and the user can't call functions
                //   that would mutate it while keeping it.
                let frame_ref = unsafe { self.frame.as_ref() };

                debug!(
                    pts=frame_ref.pts,
                    linesize=?frame_ref.linesize,
                    width=frame_ref.width,
                    height=frame_ref.height,
                    nb_samples=frame_ref.nb_samples,
                    key_frame=frame_ref.key_frame,
                    pict_type=?frame_ref.pict_type,
                    quality=0,
                    flags=frame_ref.flags,
                "Decoded frame");

                on_frame(unsafe { YuvFrame::from_sys(frame_ref) });
            }
        }
    }
}

impl Drop for Decoder {
    fn drop(&mut self) {
        unsafe {
            sys::av_parser_close(self.parser.as_ptr());
            sys::av_frame_free(&mut self.frame.as_ptr());
            sys::av_packet_free(&mut self.pkt.as_ptr());
            sys::avcodec_free_context(&mut self.ctx.as_ptr());
        }
    }
}

#[derive(Debug, Error)]
pub enum DecodeError {
    #[error("IO error reading input")]
    Io(#[from] io::Error),
    #[error("AV error decoding input")]
    Av(#[from] AvError),
    #[error("Parsed no data while trying to parse packets")]
    ParsedNoData,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Serialize;

    fn decoder_fixture() -> Decoder {
        Decoder::new().unwrap()
    }

    #[ltest]
    fn can_create() {
        decoder_fixture();
    }

    #[ltest(atest)]
    async fn can_decode_sample_data() {
        let data = tokio::fs::File::open("sample_data/sample.h264")
            .await
            .unwrap();
        let mut decoder = decoder_fixture();
        let mut frame_count = 0;
        decoder
            .decode(data, |_frame| {
                frame_count += 1;
                info!("Decoded frame {}", frame_count);
            })
            .await
            .unwrap();
        // TODO: We put in 30 frames, but only get out 29. Investigate?
        assert_eq!(frame_count, 29);
    }

    /// Should be kept in sync with examples/test_window
    #[derive(Debug, Serialize)]
    pub struct SampleYuvFrameMeta {
        pub y_linesize: usize,
        pub uv_linesize: usize,
        pub height: usize,
        pub count: usize,
    }

    #[ignore]
    #[atest]
    async fn generate_sample_yuv_frames() {
        use io::Write;

        let data = tokio::fs::File::open("sample_data/sample.h264")
            .await
            .unwrap();

        let mut y_linesize = 0;
        let mut uv_linesize = 0;
        let mut height = 0;

        let mut count = 0;
        decoder_fixture()
            .decode(data, |frame| {
                // We assume all frames have same meta
                y_linesize = frame.y_linesize;
                uv_linesize = frame.uv_linesize;
                height = frame.height;

                let mut f =
                    std::fs::File::create(format!("sample_data/yuv_frames/{}.yuv", count)).unwrap();

                f.write_all(frame.y).unwrap();
                f.write_all(frame.u).unwrap();
                f.write_all(frame.v).unwrap();

                count += 1;
            })
            .await
            .unwrap();

        let mut f = std::fs::File::create("sample_data/yuv_frames/meta.json").unwrap();

        let meta = SampleYuvFrameMeta {
            y_linesize,
            uv_linesize,
            height,
            count,
        };

        serde_json::to_writer(&mut f, &meta).unwrap();
    }
}
