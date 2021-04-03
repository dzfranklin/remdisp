#![feature(async_closure)]
#![feature(array_methods)]
#![feature(never_type)]

pub const VERSION: &str = built_info::PKG_VERSION;

#[macro_use]
mod status_helpers;
mod send_or_log;
pub mod prelude;
mod av;

#[cfg(feature = "display")]
pub mod display;

#[cfg(feature = "control")]
pub mod control;

pub mod built_info {
    include!(concat!(env!("OUT_DIR"), "/built.rs"));
}

pub mod proto {
    include!(concat!(env!("OUT_DIR"), "/control.rs"));
}
