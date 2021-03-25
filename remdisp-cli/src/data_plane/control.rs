use anyhow::Result;
use async_trait::async_trait;
use tokio::net::TcpListener;
use tonic::Status;

use crate::control_plane::control::DisplayStreamer;

mod encoder;

#[derive(Debug)]
pub struct DisplayStreamerImpl();

impl DisplayStreamerImpl {
    pub fn new() -> Self {
        Self()
    }
}

#[async_trait]
impl DisplayStreamer for DisplayStreamerImpl {
    async fn stream(&self, _listener: TcpListener) -> Result<(), Status> {
        unimplemented!()
    }
}
