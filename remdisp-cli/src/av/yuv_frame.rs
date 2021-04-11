use crate::prelude::*;
use std::convert::TryInto;
use std::os::raw::c_int;
use std::slice;

#[derive(Derivative)]
#[derivative(Debug)]
pub struct YuvFrame<'a> {
    pub y_linesize: usize,
    pub uv_linesize: usize,
    pub height: usize,
    #[derivative(Debug = "ignore")]
    pub y: &'a [u8],
    #[derivative(Debug = "ignore")]
    pub u: &'a [u8],
    #[derivative(Debug = "ignore")]
    pub v: &'a [u8],
}

impl<'a> YuvFrame<'a> {
    /// # Safety
    /// Ensure you constrain lifetime properly. Assumes it is sound to have a shared
    /// reference to data in the AVFrame for the lifetime.
    pub unsafe fn from_sys(sys: &'a ffmpeg_sys_next::AVFrame) -> Self {
        assert_eq!(
            sys.format,
            ffmpeg_sys_next::AVPixelFormat::AV_PIX_FMT_YUV420P as c_int,
            "Only YUV420P supported"
        );

        let y_linesize: usize = sys.linesize[0]
            .try_into()
            .expect("Can fit y linesize in usize");

        let u_linesize: usize = sys.linesize[1]
            .try_into()
            .expect("Can fit u linesize in usize");

        let v_linesize: usize = sys.linesize[2]
            .try_into()
            .expect("Can fit u linesize in usize");

        debug_assert_eq!(u_linesize, v_linesize);
        let uv_linesize = u_linesize;

        let height: usize = sys.height.try_into().expect("Can fit height in usize");

        // Safety: Lifetime is constrained to lifetime of borrow of frame
        let y = slice::from_raw_parts(sys.data[0], y_linesize * height);
        let u = slice::from_raw_parts(sys.data[1], uv_linesize * height / 2);
        let v = slice::from_raw_parts(sys.data[2], uv_linesize * height / 2);

        Self {
            y_linesize,
            uv_linesize,
            height,
            y,
            u,
            v,
        }
    }
}
