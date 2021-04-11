#![feature(async_closure)]
#![feature(array_methods)]
#![feature(never_type)]
#![feature(debug_non_exhaustive)]
#![feature(result_into_ok_or_err)]
#![feature(associated_type_bounds)]

pub const VERSION: &str = built_info::PKG_VERSION;

#[macro_use]
mod status_helpers;
pub mod av;
pub mod prelude;
mod send_or_log;

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
