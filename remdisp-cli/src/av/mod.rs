use std::ffi::{c_void, CStr};
use std::io;
use std::os::raw::{c_char, c_int};
use std::sync::Once;

use ffmpeg_sys_next as sys;

use converter::ConverterError;

use crate::prelude::*;

macro_rules! nonnull_or {
    ($maybe_nullptr:expr, $err:expr) => {{
        ptr::NonNull::new($maybe_nullptr).ok_or($err)
    }}
}

pub mod encoder;
pub mod decoder;
mod converter;

static LOG_SETUP: Once = Once::new();
pub(crate) fn ensure_av_logs_setup() {
    LOG_SETUP.call_once(|| {
        unsafe {
            debug!("Setup av logs");
            // Some codecs require their own log configuration, but this isn't an issue with x264.
            sys::av_log_set_level(sys::AV_LOG_DEBUG);
            sys::av_log_set_callback(Some(logs_cb));
        }
    });
}

extern "C" fn logs_cb(_: *mut c_void, level: c_int, fmt: *const c_char, args: *mut sys::__va_list_tag) {
    // Safety: args is valid va_list tag
    let str = unsafe { printf::printf(fmt, args.cast()) };
    dispatch_log(level, str);
}

fn dispatch_log(level: c_int, msg: String) {
    match level {
        sys::AV_LOG_QUIET => (),
        sys::AV_LOG_TRACE => trace!(libav=true, av_level=level, "{}", msg),
        sys::AV_LOG_DEBUG => trace!(libav=true, av_level=level, "{}", msg),
        sys::AV_LOG_VERBOSE => debug!(libav=true, av_level=level, "{}", msg),
        sys::AV_LOG_INFO => info!(libav=true, av_level=level, "{}", msg),
        sys::AV_LOG_WARNING => warn!(libav=true, av_level=level, "{}", msg),
        sys::AV_LOG_ERROR | sys::AV_LOG_FATAL | sys::AV_LOG_PANIC => error!(av_level=level, "{}", msg),
        _ => {
            warn!(?level, "Unexpected ffmpeg log level, assuming translates to WARN");
            warn!(libav=true, av_level=level, "{}", msg);
        }
    }
}

#[derive(Debug, Error)]
pub enum AvError {
    #[error("Required codec {0:?} not available, check your ffmpeg installation")]
    CodecUnavailable(sys::AVCodecID),
    #[error("Codec {0:?} supports no pixel formats")]
    CodecSupportsNoFormats(sys::AVCodecID),
    #[error("Failed to allocate and create encoding context")]
    CreateContext,
    #[error("Failed to configure context")]
    ConfigureContext,
    #[error("Failed to open context: AV_ERROR {0}")]
    OpenContext(i32),
    #[error("Failed to allocate packet")]
    AllocatePacket,
    #[error("Failed to parse to packet: AV_ERROR {0}")]
    ParseToPacket(i32),
    #[error("Failed to send packet for decoding: AV_ERROR {0}")]
    SendForDecoding(i32),
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
    #[error("Failed to initialize parser")]
    CreateParser,
    #[error("Failed to allocate frame")]
    AllocateFrame,
    #[error("Error during decoding: AV_ERROR {0}")]
    InDecoding(i32),
}

/// Copy of the macro AVERROR
fn to_av_error(i: c_int) -> c_int {
    -i
}
