use std::ffi::c_void;
use std::fmt::{Debug, Display, Formatter};
use std::fs::File;
use std::io::{Read, Write};
use std::mem::forget;
use std::os::raw::{c_int, c_uchar, c_uint};
use std::path::Path;
use std::time::Duration;
use std::{fmt, io, ptr};

use filedescriptor::{poll, pollfd, POLLIN};
use thiserror::Error;

use crate::remote::RemoteConfig;

#[repr(u8)]
#[derive(PartialEq, Debug, Clone, Copy)]
enum EvdiDeviceStatus {
    /// if the device node is EVDI and is available to use.
    Available = 0,
    /// when a node has not been created by EVDI kernel module.
    Unrecognized = 1,
    /// in other cases, e.g. when the device does not exist or cannot be opened to check.
    NotPresent = 2,
}

#[repr(C)]
#[derive(Debug, Default, Clone, Copy, PartialOrd, PartialEq)]
pub struct EvdiRect {
    /// top left x
    x1: c_int,
    /// top left y
    y1: c_int,
    /// bottom right x
    x2: c_int,
    /// bottom right y
    y2: c_int,
}

#[repr(C)]
#[derive(Debug, Clone)]
pub struct EvdiBufferDetails {
    id: c_int,
    raw_buffer: *mut u8,
    width: c_int,
    height: c_int,
    stride: c_int,
    raw_rects: *mut EvdiRect,
    rect_count: c_int,
}

// NOTE: Trick came from <https://doc.rust-lang.org/nomicon/ffi.html#representing-opaque-structs>
#[repr(C)]
#[derive(Debug)]
struct EvdiHandle {
    _private: [u8; 0],
}

impl Display for EvdiHandle {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "EvdiHandle {:p}", self)
    }
}

#[repr(C)]
#[derive(Debug)]
pub struct EvdiMode {
    pub width: c_int,
    pub height: c_int,
    pub refresh_rate: c_int,
    pub bits_per_pixel: c_int,
    pub pixel_format: c_uint,
}

#[repr(C)]
#[derive(Debug)]
pub struct EvdiCursorSet {
    hot_x: i32,
    hot_y: i32,
    width: u32,
    height: u32,
    enabled: u8,
    buffer_length: u32,
    buffer: *const u32,
    pixel_format: u32,
    stride: u32,
}

#[repr(C)]
#[derive(Debug)]
pub struct EvdiCursorMove {
    x: i32,
    y: i32,
}

#[repr(C)]
#[derive(Debug)]
pub struct EvdiDdciData {
    address: u16,
    flags: u16,
    buffer_length: u32,
    buffer: *const u8,
}

#[repr(C)]
pub struct EvdiEventContext {
    pub dpms_handler: extern "C" fn(dpms_mode: c_int, user_data: &Handle),
    pub mode_changed_handler: extern "C" fn(mode: EvdiMode, user_data: &Handle),
    pub update_ready_handler: extern "C" fn(buffer_to_be_updated: c_int, user_data: &Handle),
    pub crtc_state_handler: extern "C" fn(state: c_int, user_data: &Handle),
    pub cursor_set_handler: extern "C" fn(cursor_set: EvdiCursorSet, user_data: &Handle),
    pub cursor_move_handler: extern "C" fn(cursor_move: EvdiCursorMove, user_data: &Handle),
    pub ddci_data_handler: extern "C" fn(ddci_data: EvdiDdciData, user_data: &Handle),
    pub user_data: *const Handle,
}

impl EvdiEventContext {
    fn new(handle: *const Handle) -> Self {
        Self {
            dpms_handler,
            mode_changed_handler,
            update_ready_handler,
            crtc_state_handler,
            cursor_set_handler,
            cursor_move_handler,
            ddci_data_handler,
            user_data: handle,
        }
    }
}

extern "C" fn dpms_handler(dpms_mode: c_int, handle: &Handle) {
    handle.dpms_handler(dpms_mode);
}

extern "C" fn mode_changed_handler(mode: EvdiMode, handle: &Handle) {
    println!("Global mode changed handler");
    handle.mode_changed_handler(mode);
}

extern "C" fn update_ready_handler(buffer: c_int, handle: &Handle) {
    handle.update_ready_handler(buffer);
}

extern "C" fn crtc_state_handler(state: c_int, handle: &Handle) {
    handle.crtc_state_handler(state);
}

extern "C" fn cursor_set_handler(cursor_set: EvdiCursorSet, handle: &Handle) {
    handle.cursor_set_handler(cursor_set);
}

extern "C" fn cursor_move_handler(cursor_move: EvdiCursorMove, handle: &Handle) {
    handle.cursor_move_handler(cursor_move);
}

extern "C" fn ddci_data_handler(ddci_data: EvdiDdciData, handle: &Handle) {
    handle.ddci_data_handler(ddci_data);
}

#[link(name = "evdi")]
extern "C" {
    fn evdi_check_device(device: i32) -> EvdiDeviceStatus;
    /// 0 for fail, other for success
    fn evdi_add_device() -> i32;
    fn evdi_open<'a>(device: i32) -> &'a EvdiHandle;
    fn evdi_close(handle: &EvdiHandle);
    fn evdi_connect(
        handle: &EvdiHandle,
        edid: *const c_uchar,
        edid_length: c_uint,
        sku_area_limit: u32,
    );
    fn evdi_register_buffer(handle: &EvdiHandle, buffer: EvdiBufferDetails);
    fn evdi_handle_events(handle: &EvdiHandle, context: &EvdiEventContext);
    fn evdi_get_event_ready(handle: &EvdiHandle) -> c_int;
    fn evdi_request_update(handle: &EvdiHandle, buffer_id: c_int) -> bool;
    fn evdi_grab_pixels(handle: &EvdiHandle, rects: *mut EvdiRect, num_rects: *mut c_int);
}

#[derive(Debug)]
pub struct Device(c_int);

impl Display for Device {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "Device /dev/dri/card{}", self.0)
    }
}

impl Device {
    pub fn get() -> Result<Device, GetDeviceError> {
        if !Self::is_evdi_installed() {
            return Err(GetDeviceError::EvdiNotInstalled);
        }

        if Self::count_devices()? == 0 {
            return Err(GetDeviceError::NoDevices);
        }

        for n in 0..c_int::MAX {
            if Self::check(n) == EvdiDeviceStatus::Available {
                let out = Device(n);
                println!("Got device {}", out);
                return Ok(out);
            }
        }

        Err(GetDeviceError::NoDevicesAvailable)
    }

    fn is_evdi_installed() -> bool {
        Path::new("/sys/devices/evdi/version").exists()
    }

    pub fn open(self) -> Result<&'static mut Handle, OpenDeviceError> {
        let handle;
        unsafe {
            handle = evdi_open(self.0);
        }

        if ptr::eq(&handle, ptr::null()) {
            Err(OpenDeviceError::Generic)
        } else {
            let out = Handle::new(self, handle);
            println!("Opened device, got {}", out);
            Ok(out)
        }
    }

    pub fn add() -> Result<(), AddDeviceError> {
        let result = unsafe { evdi_add_device() };
        if result > 0 {
            Ok(())
        } else {
            Err(AddDeviceError::Generic)
        }
    }

    pub fn remove_all() -> Result<(), RemoveDeviceError> {
        let mut f = File::with_options()
            .write(true)
            .open(Path::new("/sys/devices/evdi/remove_all"))?;
        f.write("1".as_ref())?;
        Ok(())
    }

    fn check(device_num: c_int) -> EvdiDeviceStatus {
        unsafe { evdi_check_device(device_num) }
    }

    fn count_devices() -> Result<u32, CountDevicesError> {
        let mut f = File::open(Path::new("/sys/devices/evdi/count"))?;
        let mut count = String::new();
        f.read_to_string(&mut count)?;
        let count = count.trim();
        count
            .parse()
            .map_err(|_| CountDevicesError::Parse(count.into()))
    }
}

#[derive(Error, Debug)]
pub enum GetDeviceError {
    #[error("Kernel module evdi not installed")]
    EvdiNotInstalled,
    #[error("No evdi devices exist")]
    NoDevices,
    #[error("None of the evdi devices have status available")]
    NoDevicesAvailable,
    #[error("Can't count devices")]
    CountDevices(#[from] CountDevicesError),
}

#[derive(Error, Debug)]
pub enum OpenDeviceError {
    #[error("Failed to open device")]
    Generic,
}

#[derive(Error, Debug)]
pub enum AddDeviceError {
    #[error("Failed to add device. Did you run with superuser permissions?")]
    Generic,
}

#[derive(Error, Debug)]
pub enum RemoveDeviceError {
    #[error("Failed to remove device. Did you run with superuser permissions?")]
    Permission,
    #[error("Failed to remove device")]
    Generic(#[source] io::Error),
}

impl From<io::Error> for RemoveDeviceError {
    fn from(err: io::Error) -> Self {
        match err.kind() {
            io::ErrorKind::PermissionDenied => RemoveDeviceError::Permission,
            _ => RemoveDeviceError::Generic(err),
        }
    }
}

#[derive(Error, Debug)]
pub enum CountDevicesError {
    #[error("Failed to read devices count virtual file")]
    ReadFile(#[from] io::Error),
    #[error("Failed to parse devices count number: `{0}`")]
    Parse(String),
}

pub struct Handle {
    device: Device,
    handle: &'static EvdiHandle,
    connected: bool,
    event_handlers: EventHandlers,
}

impl Debug for Handle {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("Handle")
            .field("device", &self.device)
            .field("handle", self.handle)
            .field("connected", &self.connected)
            .finish_non_exhaustive()
    }
}

impl Display for Handle {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "Handle {} for {}", self.handle, self.device)
    }
}

impl Handle {
    /// Safety: handle came from evdi_open(device) and has not been connected
    fn new(device: Device, handle: &'static EvdiHandle) -> &'static mut Self {
        let this = Self {
            device,
            handle,
            connected: false,
            event_handlers: EventHandlers::default(),
        };
        Box::leak(Box::new(this))
    }

    /// Call after connect and before calling anything
    pub fn poll_ready(&self, timeout: Duration) -> Result<(), PollReadyError> {
        let fd = unsafe { evdi_get_event_ready(self.handle) };
        poll(
            &mut [pollfd {
                fd,
                events: POLLIN,
                revents: 0,
            }],
            Some(timeout),
        )
        .map(|_| ())
        .map_err(|e| PollReadyError::Polling(e))
    }

    pub fn connect(&mut self, remote: RemoteConfig) {
        let edid = remote.edid().to_owned();
        unsafe {
            evdi_connect(
                self.handle,
                edid.as_ptr(),
                edid.len() as c_uint,
                remote.area(),
            )
        }
        forget(edid);

        self.connected = true;
        println!("Connected {}", self);
    }

    /// To avoid missing events call this after the handle is connected and ready
    pub fn handle_events(&mut self, handlers: EventHandlers) {
        self.event_handlers = handlers;
        unsafe {
            evdi_handle_events(self.handle, &EvdiEventContext::new(self as *const Self));
        }
    }

    fn dpms_handler(&self, dpms_mode: c_int) {
        println!("Got dpms mode {} {}", dpms_mode, self);
    }

    fn mode_changed_handler(&self, mode: EvdiMode) {
        println!("In mode changed handler for {}", self);
        (self.event_handlers.mode_changed_handler)(self, mode);
    }

    fn update_ready_handler(&self, buffer: c_int) {
        println!("Update ready {:?} {}", buffer, self);
    }

    fn crtc_state_handler(&self, state: c_int) {
        println!("Crtc state {:?} {}", state, self);
    }

    fn cursor_set_handler(&self, cursor: EvdiCursorSet) {
        println!("Cursor set {:?} {}", cursor, self);
    }

    fn cursor_move_handler(&self, cursor: EvdiCursorMove) {
        println!("Cursor move {:?} {}", cursor, self);
    }

    fn ddci_data_handler(&self, data: EvdiDdciData) {
        println!("DDCI data {:?} {}", data, self);
    }

    pub fn register_buffer(&mut self, buf: &Buffer) {
        unsafe { evdi_register_buffer(self.handle, buf.details.clone()) }
        println!("Registered buffer for {}", &self);
    }

    /// Call grab_pixels after this either returns true or if it returns false after the
    /// update_ready_handler is called
    pub fn request_update(&self, buffer_id: i32) -> bool {
        unsafe { evdi_request_update(self.handle, buffer_id) }
    }

    /// Call after request_update
    /// buf must refer to the same buffer that an update was last requested from
    pub fn grab_pixels<'b>(&self, buf: &'b mut Buffer) -> &'b [EvdiRect] {
        let count_ptr = &mut buf.details.rect_count as *mut i32;
        println!(
            "count_ptr {:p}, buf ptr: {:p}",
            count_ptr, buf.details.raw_rects
        );
        unsafe {
            evdi_grab_pixels(self.handle, buf.details.raw_rects, count_ptr);
        }

        &buf.underlying_rect_buffer[0..(buf.details.rect_count as usize)]
    }

    pub fn close(mut self) {
        // NOTE: We deliberately take ownership of self
        self._close()
    }

    fn _close(&mut self) {
        unsafe {
            evdi_close(self.handle);
        }
        println!("Closed {}", self);
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        self._close();
    }
}

#[derive(Debug, Error)]
pub enum RegisterEventHandlersError {
    #[error("Event handlers can only be registered once")]
    AlreadySet,
}

#[derive(Debug, Error)]
pub enum PollReadyError {
    #[error("Error polling file descriptor")]
    Polling(#[source] anyhow::Error),
}

pub struct EventHandlers {
    pub dpms_handler: fn(handle: &Handle, dpms_mode: c_int),
    pub mode_changed_handler: Box<dyn Fn(&Handle, EvdiMode)>,
    pub update_ready_handler: fn(handle: &Handle, buffer: c_int),
    pub crtc_state_handler: fn(handle: &Handle, state: c_int),
    pub cursor_set_handler: fn(handle: &Handle, cursor: EvdiCursorSet),
    pub cursor_move_handler: fn(handle: &Handle, cursor: EvdiCursorMove),
    pub ddci_data_handler: fn(handle: &Handle, data: EvdiDdciData),
}

impl Default for EventHandlers {
    fn default() -> Self {
        Self {
            dpms_handler: |_, _| {},
            mode_changed_handler: Box::new(|_, _| {}),
            update_ready_handler: |_, _| {},
            crtc_state_handler: |_, _| {},
            cursor_set_handler: |_, _| {},
            cursor_move_handler: |_, _| {},
            ddci_data_handler: |_, _| {},
        }
    }
}

#[derive(Debug)]
pub struct Buffer {
    details: EvdiBufferDetails,
    underlying_buffer: Vec<u8>,
    underlying_rect_buffer: Vec<EvdiRect>,
}

impl Buffer {
    /// Can't have more than 16
    /// see <https://displaylink.github.io/evdi/details/#grabbing-pixels>
    const MAX_RECTS_BUFFER_LEN: usize = 16;

    pub fn for_mode(id: i32, mode: &EvdiMode) -> Self {
        let EvdiMode {
            width,
            height,
            bits_per_pixel,
            ..
        } = *mode;

        let stride = bits_per_pixel / 8 * width;

        Self::new(id, width, height, stride)
    }

    pub fn new(id: i32, width: i32, height: i32, stride: i32) -> Self {
        let mut underlying_buffer = vec![0u8; (height * stride) as usize];
        let buffer_ptr = (&mut underlying_buffer) as *mut _ as *mut u8;

        let mut underlying_rect_buffer = vec![EvdiRect::default(); Self::MAX_RECTS_BUFFER_LEN];
        let rects_ptr = (&mut underlying_rect_buffer) as *mut _ as *mut EvdiRect;

        let details = EvdiBufferDetails {
            id,
            raw_buffer: buffer_ptr,
            width,
            height,
            stride,
            raw_rects: rects_ptr,
            rect_count: 0,
        };

        Self {
            details,
            underlying_buffer,
            underlying_rect_buffer,
        }
    }

    pub fn height(&self) -> i32 {
        self.details.height
    }

    pub fn width(&self) -> i32 {
        self.details.width
    }

    pub fn stride(&self) -> i32 {
        self.details.stride
    }
}
