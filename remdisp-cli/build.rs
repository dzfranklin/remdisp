use anyhow::{Context, Result};
use built::write_built_file;
use std::env;

fn main() -> Result<()> {
    let out_dir = env::var("OUT_DIR").unwrap();

    write_built_file().context("Failed to acquire built-time info")?;

    let mut proto_conf = prost_build::Config::new();
    proto_conf.protoc_arg("--experimental_allow_proto3_optional");

    tonic_build::configure()
        .out_dir(out_dir)
        .compile_with_config(proto_conf, &["control.proto"], &["proto"])
        .context("Failed to compile protos")?;

    Ok(())
}
