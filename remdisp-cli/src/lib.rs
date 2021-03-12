#![feature(async_closure)]
#![feature(type_alias_impl_trait)]

pub const VERSION: &str = built_info::PKG_VERSION;

pub mod control_plane;
pub mod data_plane;
pub mod prelude;

pub mod built_info {
    include!(concat!(env!("OUT_DIR"), "/built.rs"));
}
