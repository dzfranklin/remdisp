use crate::prelude::*;
use cfg_if::cfg_if;
use std::error::Error;

#[derive(Debug)]
pub struct DisplayInfo {
    pub edid: Vec<u8>,
    pub width_pixels: u32,
    pub height_pixels: u32,
}

impl DisplayInfo {
    pub fn read_edid(handle: PlatformWindowHandle) -> Result<Vec<u8>, ReadEdidPlatformError> {
        cfg_if! {
            if #[cfg(target_os = "windows")] {
                Self::read_edid_windows(handle.expect_windows())
            } else if #[cfg(target_os = "linux")] {
                Self::read_edid_x11(handle.expect_linux())
            } else {
                panic!("Only windows and linux supported")
            }
        }
    }

    #[cfg(target_os = "windows")]
    fn read_edid_windows(
        handle: WindowsWindowHandle,
    ) -> Result<Vec<u8>, ReadEdidErrorPlatformError> {
        use monitor_control_win::{Monitor, WinError};
        fn helper(handle: WindowsWindowHandle) -> Result<Vec<u8>, WinError> {
            // TODO: Gracefully handle len != 1
            let monitors = Monitor::intersecting(handle.0.cast())?;
            let monitor = monitors.first().expect("TODO: Fixme");
            monitor.edid()
        }

        helper(handle).map_err(|err| ReadEdidPlatformError(Box::new(err)))
    }

    #[cfg(target_os = "linux")]
    fn read_edid_x11(handle: LinuxWindowHandle) -> Result<Vec<u8>, ReadEdidPlatformError> {
        use xrandr::{XHandle, XrandrError};
        fn helper(_handle: LinuxWindowHandle) -> Result<Vec<u8>, XrandrError> {
            // TODO: Get the output from the window instead of getting the first
            //  monitor's first output
            let monitors = XHandle::open()?.monitors()?;
            let monitor = monitors.first().expect("TODO: Fixme");
            let output = monitor.outputs.first().expect("TODO: Fixme");
            let edid = output.edid().expect("TODO: Fixme");
            Ok(edid)
        }

        helper(handle).map_err(|err| ReadEdidPlatformError(Box::new(err)))
    }
}

#[derive(Debug, Error)]
#[error(transparent)]
pub struct ReadEdidPlatformError(Box<dyn Error + Send + Sync + 'static>);

pub enum PlatformWindowHandle {
    Windows(WindowsWindowHandle),
    Linux(LinuxWindowHandle),
}

impl PlatformWindowHandle {
    pub fn new_windows(ptr: *mut libc::c_void) -> Self {
        Self::Windows(WindowsWindowHandle(ptr))
    }

    pub fn new_linux(ptr: *mut libc::c_void) -> Self {
        Self::Linux(LinuxWindowHandle(ptr))
    }

    pub fn expect_windows(self) -> WindowsWindowHandle {
        match self {
            Self::Windows(handle) => handle,
            _ => panic!("Expected WindowsWindowHandle"),
        }
    }

    pub fn expect_linux(self) -> LinuxWindowHandle {
        match self {
            Self::Linux(handle) => handle,
            _ => panic!("Expected LinuxWindowHandle"),
        }
    }
}

pub struct WindowsWindowHandle(pub *mut libc::c_void);
pub struct LinuxWindowHandle(pub *mut libc::c_void);
