use std::fmt::Debug;
use std::io;
use std::time::Duration;

use anyhow::Context;
use anyhow::Result;

use evdi::prelude::*;

use tokio::sync::mpsc;
use tokio::sync::mpsc::error::SendError;
use tokio::time::timeout;
use tokio_stream::wrappers::ReceiverStream;
use tonic::transport::{Channel, Endpoint, Uri};
use tonic::{Request, Status, Streaming};

use proto::{display_control_client::DisplayControlClient as GeneratedDisplayControlClient, *};

use crate::av::encoder::Encoder;
use crate::prelude::*;

use super::proto;

const CONTROL_MSG_TIMEOUT: Duration = Duration::from_secs(15);
const CONTROL_CONNECT_TIMEOUT: Duration = CONTROL_MSG_TIMEOUT;

pub struct ControlClient {
    client: GeneratedDisplayControlClient<Channel>,
}

impl ControlClient {
    /// Connect and perform hello to verify compatibility
    pub async fn connect(host: &str, port: u16) -> anyhow::Result<Self> {
        let display_uri = Uri::builder()
            .scheme("http")
            .authority(format!("{}:{}", host, port.to_string()).as_str())
            .path_and_query("/")
            .build()?;

        let endpoint = Endpoint::new(display_uri)?.timeout(CONTROL_MSG_TIMEOUT);

        // NOTE: We need to use a generic tokio timeout fn because tonic doesn't support setting the
        //  connect timeout. See <https://github.com/hyperium/tonic/issues/498>
        let mut client = timeout(
            CONTROL_CONNECT_TIMEOUT,
            GeneratedDisplayControlClient::connect(endpoint),
        )
        .await
        .context("Timed out trying to connect to host")?
        .context("Error connecting to host")?;

        client
            .hello(HelloRequest {
                version: VERSION.to_string(),
            })
            .await?;

        Ok(Self { client })
    }

    pub async fn attach(&mut self, handle: UnconnectedHandle) -> Result<(), AttachedError> {
        let (_tx, display_recv) = mpsc::channel::<ControlEvent>(16);
        let mut recv = self
            .client
            .attach(ReceiverStream::new(display_recv))
            .await?
            .into_inner();

        let display_attach = if let Some(event) = recv.message().await? {
            match event.display_event.expect("Must provide event") {
                display_event::DisplayEvent::Attach(attach) => attach,
            }
        } else {
            return Err(AttachedError::Protocol);
        };

        let config = DeviceConfig::new(
            display_attach.edid,
            display_attach.width_pixels,
            display_attach.height_pixels,
        );

        let mut handle = handle.connect(&config);

        let mode = handle
            .events
            .await_mode(EVDI_TIMEOUT)
            .await
            .map_err(|err| unavailable!("Error awaiting mode: {:?}", err))?;

        let _buf_id = handle.new_buffer(&mode);

        let _encoder = Encoder::new(mode)
            .map_err(|err| unavailable!("Failed to create encoder: {:?}", err))?;

        // let mut video_stream = TcpStream::connect(display_attach.video_addr).await?;

        // loop {
        //     handle.request_update(buf_id, EVDI_TIMEOUT).await
        //         .map_err(|err| unavailable!("Failed requesting update from kernel: {:?}", err))?;
        //
        //     let buf = handle.get_buffer(buf_id).expect("Buffer exists");
        //     encoder.send_frame(buf.bytes())
        //         .map_err(|err| unavailable!("Error sending frame to video encoder: {:?}", err))?;
        //
        //     // We currently exit when this call can't write to the stream.
        //     encoder.receive_available(&mut video_stream).await
        //         .map_err(|err| unavailable!("Failed to write video: {:?}", err))?;
        // }

        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum AttachedError {
    #[error("Remote display sent error")]
    Remote(#[from] Status),
    #[error("Other side violated protocol")]
    Protocol,
    #[error("Local error streaming")]
    Local,
    #[error("IO Error sending stream")]
    IO(#[from] io::Error),
    #[error("Error sending to other side")]
    Send,
}

impl<T> From<mpsc::error::SendError<T>> for AttachedError {
    fn from(_: SendError<T>) -> Self {
        Self::Send
    }
}

const EVDI_TIMEOUT: Duration = Duration::from_secs(10);
