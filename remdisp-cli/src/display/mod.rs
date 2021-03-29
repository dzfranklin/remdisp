use std::fmt::Debug;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::sync::Arc;

use async_trait::async_trait;
use parking_lot::Mutex;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status};
use tonic::transport;

use proto::*;
use proto::display_control_server::DisplayControlServer as GenDisplayControlServer;

use crate::control_plane::display_info::*;
use crate::data_plane::display::StreamDisplayerImpl;
use crate::prelude::*;

use super::proto;

mod info;
mod window;

#[async_trait]
pub trait StreamDisplayer: Send + Sync + Debug {
    async fn display(&self, info: DisplayInfo, stream: TcpStream) -> Result<(), Status>;
}

#[derive(Debug)]
pub struct DisplayServer {
    close_existing: Mutex<Option<mpsc::Sender<()>>>,
    stream_displayer: Arc<dyn StreamDisplayer>,
}

#[tonic::async_trait]
impl display_control_server::DisplayControl for DisplayServer {
    async fn hello(&self, req: Request<HelloRequest>) -> Result<Response<HelloReply>, Status> {
        let requester_version = req.into_inner().version;
        return if requester_version == VERSION {
            Ok(Response::new(HelloReply {
                version: VERSION.to_string()
            }))
        } else {
            Err(Status::failed_precondition("Incompatible version"))
        };
    }

    async fn attach(&self, _req: Request<AttachRequest>) -> Result<Response<AttachReply>, Status> {
        // let (tx, rx) = mpsc::channel::<Result<AttachEvent, Status>>(10);
        // let response = Ok(Response::new(ReceiverStream::new(rx)));
        //
        // let (close_sender, mut close_receiver) = mpsc::channel::<()>(1);
        // // Replace the existing closer
        // let close_previous = {
        //     let mut close_existing = self.close_existing.lock();
        //     let close_previous = close_existing.clone();
        //     *close_existing = Some(close_sender.clone());
        //     close_previous
        // };
        // // If already attached, detach
        // if close_previous.is_some() {
        //     let tx = close_previous.as_ref().unwrap();
        //     // The prev may already be done, so ignore send errors
        //     let _ = tx.send(()).await;
        // }
        //
        // // Port 0 means OS chooses
        // let addr = SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 0);
        // let listener = TcpListener::bind(addr).await
        //     .map_err(|err| Status::unavailable(format!("Error opening listener: {:?}", err)))?;
        //
        // // TODO: This isn't the local network addr, just 0.0.0.0
        // let addr = listener.local_addr()
        //     .map_err(|err| Status::unavailable(format!("Failed to get address of listener: {:?}", err)))?;
        //
        // let display_info = DisplayInfo::get()
        //     .map_err(|err| Status::unavailable(format!("Failed to get display info: {:?}", err)))?;
        //
        // let stream_displayer = Arc::clone(&self.stream_displayer);
        // tokio::spawn(async move {
        //     tx.send_or_log(Ok(AttachEvent {
        //         event: Some(attach_event::Event::Attach(attach_event::Attach {
        //             edid: display_info.edid.clone(),
        //             width_pixels: display_info.width_pixels,
        //             height_pixels: display_info.height_pixels,
        //             video_addr: addr.to_string(),
        //         }))
        //     })).await;
        //
        //     // TODO: Timeout on accept, and set ttl timeout on stream
        //     let stream = match listener.accept().await {
        //         Ok((stream, remote_addr)) => {
        //             info!("Connected to {:?}", remote_addr);
        //             stream
        //         }
        //         Err(err) => {
        //             let status = Status::unavailable(format!("Failed to accept on tcp listener: {:?}", err));
        //             tx.send_or_log(Err(status)).await;
        //             return;
        //         }
        //     };
        //
        //     tokio::select! {
        //         _ = tx.closed() => {
        //             info!("Control disconnected");
        //         }
        //         _ = close_receiver.recv() => {
        //             info!("Disconnecting from control");
        //         }
        //         result = stream_displayer.display(display_info, stream) => {
        //             match result {
        //                 Ok(()) => (),
        //                 Err(err) => tx.send_or_log(Err(err)).await,
        //             }
        //         }
        //     }
        // });
        //
        // response
        Err(Status::unimplemented(""))
    }
}

impl DisplayServer {
    pub fn new(stream_displayer: Arc<dyn StreamDisplayer>) -> Self {
        Self {
            close_existing: Mutex::new(None),
            stream_displayer,
        }
    }

    pub async fn serve(self, addr: SocketAddr) -> Result<(), transport::Error> {
        transport::Server::builder()
            .add_service(GenDisplayControlServer::new(self))
            .serve(addr)
            .await
    }
}

impl Default for DisplayServer {
    fn default() -> Self {
        let displayer = Arc::new(StreamDisplayerImpl::new());
        Self::new(displayer)
    }
}
