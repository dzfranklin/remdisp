use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};

use anyhow::{Context, Result};
use clap::{App, Arg, ArgMatches, SubCommand};

use remdisp_cli::*;
use tokio_stream::StreamExt;
use tracing::{debug, error, info, instrument, span, warn};

const DEFAULT_PORT: &str = "48611";

#[tokio::main]
async fn main() -> Result<()> {
    let args = App::new(built_info::PKG_NAME)
        .version(VERSION)
        .author(built_info::PKG_AUTHORS)
        .about(built_info::PKG_DESCRIPTION)
        .arg(Arg::with_name("features")
            .long("features")
            .help("Display the features this binary was built with and exit."))
        .arg(Arg::with_name("port")
            .long("port")
            .help("Communicate on a custom port. You must provide the same port to the display and control.")
            .takes_value(true)
            .default_value(DEFAULT_PORT))
        .subcommand(SubCommand::with_name("display")
            .about("Create a remote display that can be output to"))
        .subcommand(SubCommand::with_name("control")
            .about("Output to remote displays")
            .arg(Arg::with_name("host")
                .long("host")
                .short("h")
                .required(true)
                .takes_value(true)))
        .get_matches();

    let port: u16 = args
        .value_of("port")
        .unwrap()
        .parse()
        .context("Failed to parse port")?;

    if args.is_present("features") {
        info!("Built with features: {}", built_info::FEATURES.join(", "));
        Ok(())
    } else if let Some(sub_args) = args.subcommand_matches("display") {
        subcommand_display(port, sub_args).await
    } else if let Some(sub_args) = args.subcommand_matches("control") {
        subcommand_control(port, sub_args).await
    } else {
        Ok(())
    }
}

#[cfg(not(feature = "display"))]
async fn subcommand_display(_port: u16, _sub_args: &ArgMatches<'_>) -> Result<()> {
    Err(anyhow!("Not built with feature `display`"))
}

#[cfg(feature = "display")]
async fn subcommand_display(port: u16, _sub_args: &ArgMatches<'_>) -> Result<()> {
    use display::DisplayServer;

    let addr: SocketAddr = SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, port).into();
    DisplayServer::default().serve(addr).await?;

    Ok(())
}

#[cfg(not(feature = "control"))]
async fn subcommand_control(_port: u16, _sub_args: &ArgMatches<'_>) -> Result<()> {
    Err(anyhow!("Not built with feature `control`"))
}

#[cfg(feature = "control")]
async fn subcommand_control(port: u16, sub_args: &ArgMatches<'_>) -> Result<()> {
    use control::ControlClient;

    let host = sub_args.value_of("host").unwrap();

    let _control = ControlClient::connect(host, port).await?;

    Ok(())
}
