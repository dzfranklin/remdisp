use anyhow::Result;
use async_trait::async_trait;
use tokio::net::{TcpStream};
use tonic::Status;
use evdi::prelude::*;
use crate::prelude::*;

use crate::control_plane::control::DisplayStreamer;
use std::time::Duration;
use crate::data_plane::control::encoder::Encoder;

mod encoder;

macro_rules! unavailable {
    ($($arg:tt)*) => {{
        Status::unavailable(format!($($arg)*))
    }}
}

#[derive(Debug)]
pub struct DisplayStreamerImpl {
    device: DeviceNode,
}

const EVDI_TIMEOUT: Duration = Duration::from_secs(10);

#[async_trait(?Send)]
impl DisplayStreamer for DisplayStreamerImpl {
    async fn stream(&self, mut stream: TcpStream) -> Result<!, Status> {
        let config = DeviceConfig::sample(); // TODO: Read real values

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
