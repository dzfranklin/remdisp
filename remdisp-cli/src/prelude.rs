#[allow(unused_imports)]
pub(crate) use tracing::{trace, debug, info, warn, error, span, instrument};
pub(crate) use crate::VERSION;
pub(crate) use derivative::Derivative;
pub(crate) use crate::ensure_av_logs_setup;
pub(crate) use thiserror::Error;

#[cfg(test)]
mod test_prelude {
    pub(crate) use test_env_log::test as ltest;
    pub(crate) use tokio::test as atest;
    use std::time::Duration;

    pub const TIMEOUT: Duration = Duration::from_secs(5);
}

#[cfg(test)]
pub(crate) use test_prelude::*;
