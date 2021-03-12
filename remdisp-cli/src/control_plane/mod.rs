//! Control plane of display

#[cfg(feature = "display")]
pub mod display;

#[cfg(feature = "control")]
pub mod control;

#[cfg(feature = "display")]
pub mod display_info;

pub mod gen {
    include!(concat!(env!("OUT_DIR"), "/control.rs"));
}

#[cfg(all(test, feature = "display", feature = "control"))]
mod integration_tests {
    use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    use async_trait::async_trait;
    use lazy_static::lazy_static;
    use rand::Rng;
    use tokio::net::TcpStream;
    use tokio::task::JoinHandle;
    use tokio::time::sleep;
    use tonic::{Status, Streaming};

    use crate::control_plane::display::StreamDisplayer;
    use crate::control_plane::display_info::DisplayInfo;
    use crate::prelude::*;

    use super::control::ControlClient;
    use super::display::DisplayServer;
    use super::gen::{attach_event, AttachEvent};

    fn pick_port() -> u16 {
        // Range for ephemeral ports. See <https://en.wikipedia.org/wiki/Ephemeral_port>
        rand::thread_rng().gen_range(49152..65535)
    }

    fn spawn_server(port: u16, displayer: Arc<dyn StreamDisplayer>) {
        tokio::spawn(async move {
            let display = DisplayServer::new(displayer);
            let addr: SocketAddr = SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, port).into();
            display.serve(addr).await.unwrap();
        });
    }

    fn spawn_noop_server(port: u16) {
        #[derive(Debug)]
        struct Noop();

        #[async_trait]
        impl StreamDisplayer for Noop {
            async fn display(&self, _info: DisplayInfo, _stream: TcpStream) -> Result<(), Status> {
                Ok(())
            }
        }

        spawn_server(port, Arc::new(Noop()));
    }

    async fn client_fixture(port: u16) -> ControlClient {
        ControlClient::connect("localhost", port).await.unwrap()
    }

    #[ltest(atest)]
    async fn can_connect() {
        let port = pick_port();
        spawn_noop_server(port);
        client_fixture(port).await;
    }

    #[ltest(atest)]
    async fn can_attach_once() {
        let port = pick_port();
        spawn_noop_server(port);
        let mut client = client_fixture(port).await;

        let mut stream = client.attach().await.unwrap();
        let msg = stream.message().await.unwrap().unwrap();
        assert!(matches!(msg, AttachEvent {
            event: Some(attach_event::Event::Attach(attach_event::Attach {
                ..
            }))
        }))
    }

    #[ltest(atest)]
    async fn attaching_when_already_attached_disconnects_prev() {
        let port = pick_port();
        spawn_noop_server(port);
        let mut client = client_fixture(port).await;

        client.attach().await.unwrap();
        client.attach().await.unwrap();
    }

    #[ltest(atest)]
    async fn handles_many_concurrent_attaches() {
        let port = pick_port();

        lazy_static! {
            static ref COUNT: AtomicUsize = AtomicUsize::new(0);
        }

        #[derive(Debug)]
        struct Displayer();

        #[async_trait]
        impl StreamDisplayer for Displayer {
            async fn display(&self, _info: DisplayInfo, _stream: TcpStream) -> Result<(), Status> {
                sleep(Duration::from_millis(100)).await;
                COUNT.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }
        }

        spawn_server(port, Arc::new(Displayer()));

        // TODO: more than 10
        let handles: Vec<JoinHandle<Streaming<AttachEvent>>> = (0..100).into_iter()
            .map(|_| tokio::spawn(async move {
                let mut client = client_fixture(port).await;
                client.attach().await.unwrap()
            }))
            .collect();

        let last = handles.len() - 1;
        for (n, handle) in handles.into_iter().enumerate() {
            let mut events = handle.await.unwrap();

            // let the last one run as long as it wants
            if n == last {
                loop {
                    if let Ok(Some(AttachEvent { event: Some(attach_event::Event::Attach(attach)) })) = events.message().await {
                        // Connect so that our displayer gets run
                        TcpStream::connect(attach.video_addr).await.unwrap();

                        if let Ok(None) = events.message().await {
                            break;
                        }
                    } else {
                        unreachable!();
                    }
                }
            }
        }

        assert_eq!(COUNT.load(Ordering::SeqCst), 1);
    }

    #[ltest(atest)]
    async fn display_stream_err_is_sent() {
        let port = pick_port();

        #[derive(Debug)]
        struct Displayer();

        #[async_trait]
        impl StreamDisplayer for Displayer {
            async fn display(&self, _info: DisplayInfo, _stream: TcpStream) -> Result<(), Status> {
                Err(Status::permission_denied("Foo"))
            }
        }

        spawn_server(port, Arc::new(Displayer()));

        let mut client = client_fixture(port).await;

        let mut resp = client.attach().await.unwrap();

        loop {
            match resp.message().await {
                Ok(None) => break,
                Ok(Some(AttachEvent { event: Some(attach_event::Event::Attach(attach))})) => {
                    // Connect so that our displayer gets run
                    TcpStream::connect(attach.video_addr).await.unwrap();
                }
                Ok(_) => (),
                Err(err) => {
                    assert_eq!(err.message(), "Foo");
                    break;
                }
            };
        }
    }
}
