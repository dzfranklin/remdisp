use std::fmt::Debug;
use tokio::sync::mpsc;
use tracing::{debug, error, info, instrument, span, trace, warn};

use async_trait::async_trait;

#[async_trait]
pub(crate) trait SendOrLog<T: Debug> {
    async fn send_or_log(&self, msg: T);
}

#[async_trait]
impl<T: Debug + Send + Sync> SendOrLog<T> for mpsc::Sender<T> {
    async fn send_or_log(&self, msg: T) {
        let msg_dbg = format!("{:?}", msg);

        match self.send(msg).await {
            Ok(()) => {
                info!(msg = ?msg_dbg, "Sent");
            }
            Err(_) => {
                warn!(chan = ?self, msg = ?msg_dbg, "Failed to send to")
            }
        }
    }
}
