use std::fmt::Debug;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::pin::Pin;

use async_trait::async_trait;
use futures::{Stream, TryStreamExt};

use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::transport;
use tonic::{Request, Response, Status, Streaming};

use proto::display_control_server::DisplayControlServer as GenDisplayControlServer;
use proto::*;

use crate::display::displayer::spawn_displayer;
use crate::display::info::DisplayInfo;
use crate::prelude::*;

use super::proto;

pub mod displayer;
pub mod info;
pub mod window;

#[async_trait]
pub trait StreamDisplayer: Send + Sync + Debug {
    async fn display(&self, info: DisplayInfo, stream: TcpStream) -> Result<(), Status>;
}

#[derive(Debug)]
pub struct DisplayServer {
    window: mpsc::Sender<displayer::EventChans>,
}

#[tonic::async_trait]
impl display_control_server::DisplayControl for DisplayServer {
    async fn hello(&self, req: Request<HelloRequest>) -> Result<Response<HelloReply>, Status> {
        let requester_version = req.into_inner().version;
        return if requester_version == VERSION {
            Ok(Response::new(HelloReply {
                version: VERSION.to_string(),
            }))
        } else {
            Err(Status::failed_precondition("Incompatible version"))
        };
    }

    type AttachStream =
        Pin<Box<dyn Stream<Item = Result<DisplayEvent, Status>> + Send + Sync + 'static>>;

    async fn attach(
        &self,
        request: Request<Streaming<ControlEvent>>,
    ) -> Result<Response<Self::AttachStream>, Status> {
        let recv = request.into_inner();
        let (tx, control_recv) = mpsc::channel::<Result<DisplayEvent, Status>>(16);

        self.window
            .send(displayer::EventChans { tx, recv })
            .await
            .map_err(|_| Status::unavailable("Could not connect to window actor"))?;

        Ok(Response::new(Box::pin(ReceiverStream::new(control_recv))))
    }
}

impl DisplayServer {
    pub async fn serve(self, addr: SocketAddr) -> Result<(), transport::Error> {
        transport::Server::builder()
            .add_service(GenDisplayControlServer::new(self))
            .serve(addr)
            .await
    }
}

impl Default for DisplayServer {
    fn default() -> Self {
        let (window_tx, window_recv) = mpsc::channel(16);
        spawn_displayer(window_recv);

        Self { window: window_tx }
    }
}
