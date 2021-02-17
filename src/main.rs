#![feature(c_variadic)]

use std::ffi::c_void;
use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::process::exit;
use std::thread::sleep;
use std::time::Duration;

use pixels::{Pixels, SurfaceTexture};
use winit::dpi::LogicalSize;
use winit::event::Event;
use winit::event_loop::{ControlFlow, EventLoop};
use winit::window::WindowBuilder;

use remdisp::control::{Buffer, Device, EvdiEventContext, EvdiMode, EventHandlers};
use remdisp::remote::RemoteConfig;
use std::os::raw::c_int;
use std::sync::mpsc::{channel, Receiver, Sender};

fn main() -> anyhow::Result<()> {
    let device = Device::get()?;

    let mut handle = device.open()?;

    let edid: Vec<u8> = include_bytes!("EDIDv1_1280x800").to_vec();
    let my_laptop = RemoteConfig::new(edid, 2073600);
    handle.connect(my_laptop);

    handle.poll_ready(Duration::from_secs(5))?;

    let (tx, rx): (Sender<EvdiMode>, Receiver<EvdiMode>) = channel();
    handle.handle_events(EventHandlers {
        mode_changed_handler: Box::new(move |handle, mode| {
            println!("In closure");
            tx.send(mode);
        }),
        ..EventHandlers::default()
    });

    let mode = rx.recv().unwrap();
    println!("Got mode {:?}", mode);
    let mut buf = Buffer::for_mode(0, &mode);
    handle.register_buffer(&buf);

    for n in 0..100 {
        if handle.request_update(0) {
            println!("Receiving update sync");
            let rects = handle.grab_pixels(&mut buf);
            println!("Got rects {:?}", rects);
        } else {
            println!("Receiving update async");
        }
        sleep(Duration::from_secs(1) / 60);
    }

    // let width = mode.width;
    // let height = mode.height;
    // let event_loop = EventLoop::new();
    // let window = {
    //     let size = LogicalSize::new(width as f64, height as f64);
    //     WindowBuilder::new()
    //         .with_title("remdisp")
    //         .with_inner_size(size)
    //         .with_min_inner_size(size)
    //         .build(&event_loop)
    //         .unwrap()
    // };
    // let mut pixels = {
    //     let window_size = window.inner_size();
    //     let surface_texture = SurfaceTexture::new(window_size.width, window_size.height, &window);
    //     Pixels::new(width as u32, height as u32, surface_texture)?
    // };
    //
    // println!("Pixels size: {:?}", pixels.get_frame().len());
    // println!("Buf size: {:?}", buf.buffer().len());

    // event_loop.run(move |event, _, control_flow| {
    //     // Draw the current frame
    //     if let Event::RedrawRequested(_) = event {
    //         draw(&buf, pixels.get_frame());
    //         if pixels.render().is_err() {
    //             *control_flow = ControlFlow::Exit;
    //             return;
    //         }
    //     }
    // });

    Ok(())
}

fn draw(buf: &Buffer, frame: &mut [u8]) {
    // TODO: Detect pixel format
    // for (n, chunk) in buf.buffer().chunks(4).into_iter().enumerate() {
    //     let rgb = &chunk[1..4];
    //     let start = n * 3;
    //     frame
    //         .get_mut((start)..(start * 3))
    //         .unwrap()
    //         .copy_from_slice(rgb);
    // }
}
