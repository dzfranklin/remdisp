use std::io::Read;
use std::{fs::File, time::Duration};

use anyhow::Result;
use remdisp_cli::{
    av::yuv_frame::YuvFrame,
    display::window::{SdlWindow, Window},
};
use serde::Deserialize;
use tracing::info;
use tracing_subscriber::{fmt::format::FmtSpan, FmtSubscriber};

fn main() -> Result<()> {
    let subscriber = FmtSubscriber::builder()
        .with_env_filter("debug")
        .with_span_events(FmtSpan::NEW | FmtSpan::CLOSE)
        .pretty()
        .finish();
    let _ = tracing::subscriber::set_global_default(subscriber);

    show_frames()?;

    Ok(())
}

/// Should be kept in sync with av::decoder::tests::SampleYuvFrameMeta
#[derive(Debug, Deserialize)]
pub struct SampleYuvFrameMeta {
    pub y_linesize: usize,
    pub uv_linesize: usize,
    pub height: usize,
    pub count: usize,
}

fn show_frames() -> Result<()> {
    let mut window = SdlWindow::new();

    let info = window.create()?;
    info!("Got display info: {:?}", info);

    let meta: SampleYuvFrameMeta =
        serde_json::from_reader(File::open("sample_data/yuv_frames/meta.json")?)?;
    info!("{:?}", meta);

    let mut y_data = vec![0u8; meta.y_linesize * meta.height];
    let mut u_data = vec![0u8; meta.uv_linesize * meta.height / 2];
    let mut v_data = vec![0u8; meta.uv_linesize * meta.height / 2];

    for n in 0..meta.count {
        let mut f = File::open(format!("sample_data/yuv_frames/{}.yuv", n))?;
        f.read_exact(&mut y_data)?;
        f.read_exact(&mut u_data)?;
        f.read_exact(&mut v_data)?;

        let frame = YuvFrame {
            y_linesize: meta.y_linesize,
            uv_linesize: meta.uv_linesize,
            height: meta.height,
            y: &y_data,
            u: &u_data,
            v: &v_data,
        };

        window.update(frame)?;

        std::thread::sleep(Duration::from_millis(400));
    }

    window.close()?;

    Ok(())
}
