#![feature(async_closure)]
#![feature(type_alias_impl_trait)]
#![feature(array_methods)]
#![feature(c_variadic)]

use std::sync::Once;
use std::ffi::{c_void, CStr};
use ffmpeg_sys_next as av;
use std::os::raw::{c_char, c_int};

pub const VERSION: &str = built_info::PKG_VERSION;

pub mod control_plane;
pub mod data_plane;
pub mod prelude;

use prelude::*;
use ffmpeg_sys_next::__va_list_tag;

pub mod built_info {
    include!(concat!(env!("OUT_DIR"), "/built.rs"));
}

const AV_LOGS_SETUP: Once = Once::new();
pub(crate) fn ensure_av_logs_setup() {
    AV_LOGS_SETUP.call_once(|| {
        unsafe {
            debug!("Setup av logs");
            av::av_log_set_level(av::AV_LOG_DEBUG);
            av::av_log_set_callback(Some(av_logs_cb));
        }
    });
}

unsafe extern "C" fn av_logs_cb(_: *mut c_void, level: c_int, fmt: *const c_char, args: *mut __va_list_tag) {
    let mut buf = vec![0u8; 1000];
    let buf_ptr = buf.as_mut_ptr().cast();
    libc::snprintf(buf_ptr, buf.len(), fmt, args);

    // Safety: we know the length of the string is sane, and that we own the underlying data
    //  We copy the data immediately, so we don't have to worry about the lifetime.
    let str = CStr::from_ptr(buf_ptr).to_string_lossy().into_owned();

    dispatch_av_log(level, str);
}

fn dispatch_av_log(level: c_int, msg: String) {
    match level {
        av::AV_LOG_QUIET => (),
        av::AV_LOG_TRACE => trace!(av_level=level, "{}", msg),
        av::AV_LOG_DEBUG => trace!(av_level=level, "{}", msg),
        av::AV_LOG_VERBOSE => debug!(av_level=level, "{}", msg),
        av::AV_LOG_INFO => info!(av_level=level, "{}", msg),
        av::AV_LOG_WARNING => warn!(av_level=level, "{}", msg),
        av::AV_LOG_ERROR | av::AV_LOG_FATAL | av::AV_LOG_PANIC => error!(av_level=level, "{}", msg),
        _ => {
            warn!(?level, "Unexpected ffmpeg log level, assuming translates to WARN");
            warn!(av_level=level, "{}", msg);
        }
    }
}
