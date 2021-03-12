use std::fmt::Debug;
use std::time::Duration;

use anyhow::Context;
use async_trait::async_trait;
use tokio::net::TcpListener;
use tokio::time::timeout;
use tonic::{Status, Streaming};
use tonic::transport::{Channel, Endpoint, Uri};

use gen::{*, display_control_client::DisplayControlClient as GeneratedDisplayControlClient};

use crate::prelude::*;

use super::gen;

const CONTROL_MSG_TIMEOUT: Duration = Duration::from_secs(15);
const CONTROL_CONNECT_TIMEOUT: Duration = CONTROL_MSG_TIMEOUT;

#[async_trait]
pub trait DisplayStreamer: Send + Sync + Debug {
    async fn stream(&self, listener: TcpListener) -> Result<(), Status>;
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

    pub async fn attach(&mut self) -> Result<Streaming<AttachEvent>, Status> {
        let reply = self.client.attach(AttachRequest {}).await?.into_inner();
        Ok(reply)
    }
}
