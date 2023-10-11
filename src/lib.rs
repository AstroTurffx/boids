/// Just some utility to making bind groups easier
mod bind_group;
mod camera;
mod camera_controller;
mod graphics;
mod texture;

use crate::graphics::State;
use imgui_winit_support::winit::dpi::LogicalSize;
use imgui_winit_support::winit::event::{Event, WindowEvent};
use imgui_winit_support::winit::event_loop::{ControlFlow, EventLoop};
use imgui_winit_support::winit::window::{WindowBuilder, WindowId};
use instant::Instant;
use log::{debug, trace, warn};
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;
use wgpu::SurfaceError;

const SIZE_X: u32 = 600;
const SIZE_Y: u32 = 600;

#[cfg_attr(target_arch = "wasm32", wasm_bindgen(start))]
pub async fn run() {
    // Initiate loggers
    cfg_if::cfg_if! {
        if #[cfg(target_arch = "wasm32")] {
            std::panic::set_hook(Box::new(console_error_panic_hook::hook));
            console_log::init_with_level(log::Level::Info).expect("Couldn't initialize logger");
            trace!("console_log is active");
        } else {
            env_logger::init();
            trace!("env_logger is active");
        }
    }

    debug!("--- System info ---");
    debug!("OS: {}", std::env::consts::OS);
    debug!("Architecture: {}", std::env::consts::ARCH);

    trace!("Starting window creation");
    let now = Instant::now();
    let event_loop = EventLoop::new();
    let window = WindowBuilder::new()
        .with_title("Boids")
        .with_inner_size(LogicalSize::new(SIZE_X, SIZE_Y))
        .build(&event_loop)
        .unwrap();
    debug!("Window creation finished in {:.2?}", now.elapsed());

    #[cfg(target_arch = "wasm32")]
    {
        // winit prevents sizing with CSS, so we have to set
        // the size manually when on web.
        use winit::dpi::PhysicalSize;
        window.set_inner_size(PhysicalSize::new(SIZE_X, SIZE_Y));
        trace!("Set window inner size to {}, {}", SIZE_X, SIZE_Y);

        use winit::platform::web::WindowExtWebSys;
        web_sys::window()
            .and_then(|win| win.document())
            .and_then(|doc| {
                let dst = doc.get_element_by_id("wasm-body")?;
                let canvas = web_sys::Element::from(window.canvas());
                dst.append_child(&canvas).ok()?;
                Some(())
            })
            .expect("Couldn't append canvas to document body.");
    }

    let mut state = State::new(window).await;
    let mut last_frame = Instant::now();

    trace!("Starting window event loop");
    event_loop.run(move |event, _, control_flow| {
        state.ui_handle_event(&event);
        match event {
            Event::WindowEvent { event, window_id } => {
                handle_window_event(event, window_id, &mut state, control_flow)
            }
            Event::RedrawRequested(window_id) if window_id == state.window().id() => {
                let delta_s = last_frame.elapsed();
                let now = Instant::now();
                state.ui().io_mut().update_delta_time(now - last_frame);
                last_frame = now;

                state.update(delta_s);
                match state.render() {
                    Ok(_) => {}
                    // Reconfigure the surface if lost
                    Err(SurfaceError::Lost) => state.resize(*state.size()),
                    // The system is out of memory, we should probably quit
                    Err(SurfaceError::OutOfMemory) => *control_flow = ControlFlow::Exit,
                    // All other errors (Outdated, Timeout) should be resolved by the next frame
                    Err(e) => {
                        log::error!("{:?}", e);
                        eprintln!("{:?}", e);
                    }
                }
            }
            Event::MainEventsCleared => {
                state.window().request_redraw();
            }
            _ => (),
        }
    });
}

fn handle_window_event(
    event: WindowEvent,
    window_id: WindowId,
    state: &mut State,
    control_flow: &mut ControlFlow,
) {
    if window_id != state.window().id() {
        warn!(
            "Window id is different (main window: {:?}, event window: {:?})",
            window_id,
            state.window().id()
        );
        return;
    }
    if state.input(&event) {
        return;
    }

    match event {
        WindowEvent::CloseRequested => {
            debug!("Close window request (window id: {})", u64::from(window_id));
            control_flow.set_exit();
        }
        WindowEvent::Resized(physical_size) => state.resize(physical_size),
        WindowEvent::ScaleFactorChanged { new_inner_size, .. } => state.resize(*new_inner_size),
        _ => {}
    }
}
