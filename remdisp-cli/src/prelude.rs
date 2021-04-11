pub(crate) use crate::send_or_log::SendOrLog;
pub(crate) use crate::VERSION;
pub(crate) use derivative::Derivative;
pub(crate) use thiserror::Error;
#[allow(unused_imports)]
pub(crate) use tracing::{debug, error, info, instrument, span, trace, warn};

#[cfg(test)]
mod test_prelude {
    use std::time::Duration;
    pub(crate) use test_env_log::test as ltest;
    pub(crate) use tokio::test as atest;

    pub const TIMEOUT: Duration = Duration::from_secs(5);
}

#[cfg(test)]
pub(crate) use test_prelude::*;
