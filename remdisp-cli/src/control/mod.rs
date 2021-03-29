use std::fmt::Debug;
use std::time::Duration;

use anyhow::Context;
use anyhow::Result;
use async_trait::async_trait;
use evdi::prelude::*;
use tokio::net::TcpStream;
use tokio::time::timeout;
use tonic::{Status, Streaming};
use tonic::transport::{Channel, Endpoint, Uri};

use proto::{*, display_control_client::DisplayControlClient as GeneratedDisplayControlClient};

use crate::control_plane::control::DisplayStreamer;
use crate::data_plane::control::encoder::Encoder;
use crate::prelude::*;
use crate::prelude::*;

use super::proto;

const CONTROL_MSG_TIMEOUT: Duration = Duration::from_secs(15);
const CONTROL_CONNECT_TIMEOUT: Duration = CONTROL_MSG_TIMEOUT;

mod encoder;

#[async_trait(? Send)]
pub trait DisplayStreamer: Debug {
    /// Cancelled by caller
    async fn stream(&self, stream: TcpStream) -> Result<!, Status>;
}

pub struct ControlClient {
    client: GeneratedDisplayControlClient<Channel>
}

impl ControlClient {
    /// Connect and perform hello to verify compatibility
    pub async fn connect(host: &str, port: u16) -> anyhow::Result<Self> {
        let display_uri = Uri::builder()
            .scheme("http")
            .authority(format!("{}:{}", host, port.to_string()).as_str())
            .path_and_query("/")
            .build()?;

        let endpoint = Endpoint::new(display_uri)?
            .timeout(CONTROL_MSG_TIMEOUT);

        // NOTE: We need to use a generic tokio timeout fn because tonic doesn't support setting the
        //  connect timeout. See <https://github.com/hyperium/tonic/issues/498>
        let mut client = timeout(
            CONTROL_CONNECT_TIMEOUT,
            GeneratedDisplayControlClient::connect(endpoint),
        )
            .await.context("Timed out trying to connect to host")?
            .context("Error connecting to host")?;

        client.hello(HelloRequest {
            version: VERSION.to_string(),
        }).await?;

        Ok(Self { client })
    }

    pub async fn attach(&mut self) -> Result<AttachReply, Status> {
        let reply = self.client.attach(AttachRequest {}).await?.into_inner();
        Ok(reply)
    }
}

#[derive(Debug)]
pub struct DisplayStreamerImpl {
    device: DeviceNode,
}

const EVDI_TIMEOUT: Duration = Duration::from_secs(10);

#[async_trait(? Send)]
impl DisplayStreamer for DisplayStreamerImpl {
    async fn stream(&self, mut stream: TcpStream) -> Result<!, Status> {
        let config = DeviceConfig::sample(); // TODO: Read real values

        // TODO: Open handles ahead of time so we reserve a device?
        //  then we need a good way to ensure we close them.
        let mut handle = self.device.open()
            .map_err(|err| unavailable!("Failed to open device: {:?}", err))?
            .connect(&config);

        let mode = handle.events.await_mode(EVDI_TIMEOUT).await
            .map_err(|err| unavailable!("Error awaiting mode: {:?}", err))?;

        let buf_id = handle.new_buffer(&mode);

        let mut encoder = Encoder::new(mode)
            .map_err(|err| unavailable!("Failed to create encoder: {:?}", err))?;

        loop {
            handle.request_update(buf_id, EVDI_TIMEOUT).await
                .map_err(|err| unavailable!("Failed requesting update from kernel: {:?}", err))?;

            let buf = handle.get_buffer(buf_id).expect("Buffer exists");
            encoder.send_frame(buf.bytes())
                .map_err(|err| unavailable!("Error sending frame to video encoder: {:?}", err))?;

            encoder.receive_available(&mut stream).await
                .map_err(|err| unavailable!("Failed to write video: {:?}", err))?;
        }
    }
}

impl DisplayStreamerImpl {
    pub fn new() -> Result<Self, Status> {
        let device = DeviceNode::get()
            .ok_or_else(|| Status::failed_precondition("No evdi device node available"))?;

        Ok(Self { device })
    }
}

