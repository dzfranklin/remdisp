#![feature(async_closure)]
#![feature(type_alias_impl_trait)]
#![feature(array_methods)]
#![feature(never_type)]

use std::sync::Once;
use std::ffi::{c_void, CStr};
use ffmpeg_sys_next as av;
use std::os::raw::{c_char, c_int};

pub const VERSION: &str = built_info::PKG_VERSION;

#[macro_use]
mod status_helpers;
mod send_or_log;
pub mod prelude;

#[cfg(feature = "display")]
pub mod display;

#[cfg(feature = "control")]
pub mod control;

use prelude::*;
use ffmpeg_sys_next::__va_list_tag;

pub mod built_info {
    include!(concat!(env!("OUT_DIR"), "/built.rs"));
}

pub mod proto {
    include!(concat!(env!("OUT_DIR"), "/mod.rs"));
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

#[cfg(all(test, feature = "display", feature = "control"))]
mod integration_tests {
    #[test]
    fn mark_unimplemented() {
        unimplemented!()
    }
    // use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
    // use std::sync::Arc;
    // use std::sync::atomic::{AtomicUsize, Ordering};
    // use std::time::Duration;
    //
    // use async_trait::async_trait;
    // use lazy_static::lazy_static;
    // use rand::Rng;
    // use tokio::net::TcpStream;
    // use tokio::task::JoinHandle;
    // use tokio::time::sleep;
    // use tonic::{Status, Streaming};
    //
    // use crate::control_plane::display::StreamDisplayer;
    // use crate::control_plane::display_info::DisplayInfo;
    // use crate::prelude::*;
    //
    // use super::control::ControlClient;
    // use super::display::DisplayServer;
    //
    // fn pick_port() -> u16 {
    //     // Range for ephemeral ports. See <https://en.wikipedia.org/wiki/Ephemeral_port>
    //     rand::thread_rng().gen_range(49152..65535)
    // }
    //
    // fn spawn_server(port: u16, displayer: Arc<dyn StreamDisplayer>) {
    //     tokio::spawn(async move {
    //         let display = DisplayServer::new(displayer);
    //         let addr: SocketAddr = SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, port).into();
    //         display.serve(addr).await.unwrap();
    //     });
    // }
    //
    // fn spawn_noop_server(port: u16) {
    //     #[derive(Debug)]
    //     struct Noop();
    //
    //     #[async_trait]
    //     impl StreamDisplayer for Noop {
    //         async fn display(&self, _info: DisplayInfo, _stream: TcpStream) -> Result<(), Status> {
    //             Ok(())
    //         }
    //     }
    //
    //     spawn_server(port, Arc::new(Noop()));
    // }
    //
    // async fn client_fixture(port: u16) -> ControlClient {
    //     ControlClient::connect("localhost", port).await.unwrap()
    // }
    //
    // #[ltest(atest)]
    // async fn can_connect() {
    //     let port = pick_port();
    //     spawn_noop_server(port);
    //     client_fixture(port).await;
    // }
    //
    // #[ltest(atest)]
    // async fn can_attach_once() {
    //     let port = pick_port();
    //     spawn_noop_server(port);
    //     let mut client = client_fixture(port).await;
    //
    //     let mut stream = client.attach().await.unwrap();
    //     let msg = stream.message().await.unwrap().unwrap();
    //     assert!(matches!(msg, AttachEvent {
    //         event: Some(attach_event::Event::Attach(attach_event::Attach {
    //             ..
    //         }))
    //     }))
    // }
    //
    // #[ltest(atest)]
    // async fn attaching_when_already_attached_disconnects_prev() {
    //     let port = pick_port();
    //     spawn_noop_server(port);
    //     let mut client = client_fixture(port).await;
    //
    //     client.attach().await.unwrap();
    //     client.attach().await.unwrap();
    // }
    //
    // #[ltest(atest)]
    // async fn handles_many_concurrent_attaches() {
    //     let port = pick_port();
    //
    //     lazy_static! {
    //         static ref COUNT: AtomicUsize = AtomicUsize::new(0);
    //     }
    //
    //     #[derive(Debug)]
    //     struct Displayer();
    //
    //     #[async_trait]
    //     impl StreamDisplayer for Displayer {
    //         async fn display(&self, _info: DisplayInfo, _stream: TcpStream) -> Result<(), Status> {
    //             sleep(Duration::from_millis(100)).await;
    //             COUNT.fetch_add(1, Ordering::SeqCst);
    //             Ok(())
    //         }
    //     }
    //
    //     spawn_server(port, Arc::new(Displayer()));
    //
    //     // TODO: more than 10
    //     let handles: Vec<JoinHandle<Streaming<AttachEvent>>> = (0..100).into_iter()
    //         .map(|_| tokio::spawn(async move {
    //             let mut client = client_fixture(port).await;
    //             client.attach().await.unwrap()
    //         }))
    //         .collect();
    //
    //     let last = handles.len() - 1;
    //     for (n, handle) in handles.into_iter().enumerate() {
    //         let mut events = handle.await.unwrap();
    //
    //         // let the last one run as long as it wants
    //         if n == last {
    //             loop {
    //                 if let Ok(Some(AttachEvent { event: Some(attach_event::Event::Attach(attach)) })) = events.message().await {
    //                     // Connect so that our displayer gets run
    //                     TcpStream::connect(attach.video_addr).await.unwrap();
    //
    //                     if let Ok(None) = events.message().await {
    //                         break;
    //                     }
    //                 } else {
    //                     unreachable!();
    //                 }
    //             }
    //         }
    //     }
    //
    //     assert_eq!(COUNT.load(Ordering::SeqCst), 1);
    // }
    //
    // #[ltest(atest)]
    // async fn display_stream_err_is_sent() {
    //     let port = pick_port();
    //
    //     #[derive(Debug)]
    //     struct Displayer();
    //
    //     #[async_trait]
    //     impl StreamDisplayer for Displayer {
    //         async fn display(&self, _info: DisplayInfo, _stream: TcpStream) -> Result<(), Status> {
    //             Err(Status::permission_denied("Foo"))
    //         }
    //     }
    //
    //     spawn_server(port, Arc::new(Displayer()));
    //
    //     let mut client = client_fixture(port).await;
    //
    //     let mut resp = client.attach().await.unwrap();
    //
    //     loop {
    //         match resp.message().await {
    //             Ok(None) => break,
    //             Ok(Some(AttachEvent { event: Some(attach_event::Event::Attach(attach))})) => {
    //                 // Connect so that our displayer gets run
    //                 TcpStream::connect(attach.video_addr).await.unwrap();
    //             }
    //             Ok(_) => (),
    //             Err(err) => {
    //                 assert_eq!(err.message(), "Foo");
    //                 break;
    //             }
    //         };
    //     }
    // }
}
