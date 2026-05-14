mod app;
mod params;
mod renderer;
mod ui;

use anyhow::Result;
use winit::event_loop::{ControlFlow, EventLoop};

fn main() -> Result<()> {
    env_logger::Builder::from_env(
        env_logger::Env::default()
            .default_filter_or("info,wgpu_core=warn,wgpu_hal=warn,naga=warn"),
    )
    .init();

    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut app = app::App::default();
    event_loop.run_app(&mut app)?;
    Ok(())
}
