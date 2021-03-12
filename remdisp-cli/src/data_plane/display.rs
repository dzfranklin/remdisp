use anyhow::Result;
use async_trait::async_trait;
use tokio::net::TcpStream;
use tonic::Status;

use crate::control_plane::display::StreamDisplayer;
use crate::control_plane::display_info::DisplayInfo;

#[derive(Debug)]
pub struct StreamDisplayerImpl();

impl StreamDisplayerImpl {
    pub fn new() -> Self {
        Self()
    }
}

#[async_trait]
impl StreamDisplayer for StreamDisplayerImpl {
    async fn display(&self, _info: DisplayInfo, _stream: TcpStream) -> Result<(), Status> {
        Ok(())
    }
}
