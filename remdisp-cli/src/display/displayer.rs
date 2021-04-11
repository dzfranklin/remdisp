use crate::av::AvError;
use crate::av::{self, decoder::Decoder};
use crate::display::window::{SdlWindow, Window, WindowError};
use crate::prelude::*;
use crate::proto::{display_event, ControlEvent, DisplayEvent};
use std::fmt::Debug;
use std::net::Ipv4Addr;
use std::{io, thread};
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tokio::sync::mpsc::error::SendError;
use tonic::{Status, Streaming};

#[derive(Debug)]
pub struct EventChans {
    pub tx: mpsc::Sender<Result<DisplayEvent, Status>>,
    pub recv: Streaming<ControlEvent>,
}

pub fn spawn_displayer(mut chan: mpsc::Receiver<EventChans>) {
    thread::spawn(|| {
        tokio::runtime::Handle::current().block_on(async move {
            let mut event_chans: Option<EventChans> = None;
            let mut window = SdlWindow::new();

            loop {
                match event_chans {
                    Some(curr_event_chans) => tokio::select! {
                        new_attached = chan.recv() => {
                            match new_attached {
                                Some(new_attached) => {
                                    info!("Window actor detached to attach to new");
                                    event_chans = Some(new_attached)
                                },
                                None => {
                                    info!("Window actor exiting as input chan closed");
                                    return
                                },
                            }
                        },

                        exit_status = show_window(&curr_event_chans, &mut window) => {
                            warn!(?exit_status, "show_window exited early");

                            let status = match exit_status {
                                Ok(_) => Status::ok("Done"),
                                Err(err) => err,
                            };
                            curr_event_chans.tx.send_or_log(Err(status)).await;

                            event_chans = None;
                        }
                    },

                    None => match chan.recv().await {
                        Some(new_attached) => {
                            info!("Window actor attaching");
                            event_chans = Some(new_attached)
                        }
                        None => {
                            info!("Window actor exiting as input chan closed");
                            return;
                        }
                    },
                }
            }
        });
    });
}

#[instrument]
async fn show_window<W: Window>(chans: &EventChans, window: &mut W) -> Result<(), Status> {
    let listener = TcpListener::bind((Ipv4Addr::UNSPECIFIED, 0)).await?; // 0 means OS chooses
    let port = listener.local_addr()?.port();

    let display_info = window.create().map_err(ShowWindowError::from)?;

    chans
        .tx
        .send(Ok(DisplayEvent {
            display_event: Some(display_event::DisplayEvent::Attach(display_event::Attach {
                edid: display_info.edid,
                width_pixels: display_info.width_pixels,
                height_pixels: display_info.height_pixels,
                video_port: port as u32,
            })),
        }))
        .await
        .map_err(ShowWindowError::from)?;
    debug!("Sent attach event to control");

    let (stream, control_addr) = listener.accept().await?;
    info!(?control_addr, "Control accepted stream");

    let mut decoder = Decoder::new().map_err(ShowWindowError::from)?;
    debug!(?decoder, "Created decoder");

    decoder
        .decode(stream, |frame| {
            debug!(?frame, "Received frame from decoder");
            if let Err(err) = window.update(frame) {
                warn!("Error updating window: {:?}", err);
                let tx = chans.tx.clone();
                tokio::spawn(async move {
                    tx.send_or_log(Err(err.into())).await;
                });
            }
        })
        .await?;

    window.close()?;

    Ok(())
}

#[derive(Error, Debug)]
enum ShowWindowError {
    #[error("Error displaying window")]
    Window(#[from] WindowError),
    #[error("Error decoding stream")]
    Decode(#[from] AvError),
    #[error("Error communicating with client")]
    ClientCom,
    #[error("Error performing stream IO")]
    StreamIo(#[from] io::Error),
}

impl<T> From<mpsc::error::SendError<T>> for ShowWindowError {
    fn from(_: SendError<T>) -> Self {
        Self::ClientCom
    }
}

impl From<ShowWindowError> for Status {
    fn from(err: ShowWindowError) -> Self {
        Status::unavailable(format!("{}", err))
    }
}

impl From<av::decoder::DecodeError> for Status {
    fn from(err: av::decoder::DecodeError) -> Self {
        Status::unavailable(format!("{}", err))
    }
}
