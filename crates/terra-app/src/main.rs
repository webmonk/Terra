//! Terra editor application shell.

mod app;

use app::TerraApp;
use winit::event_loop::{ControlFlow, EventLoop};

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let event_loop = EventLoop::new().expect("event loop");
    // Wait on OS events; about_to_wait arms WaitUntil only while refining.
    event_loop.set_control_flow(ControlFlow::Wait);

    let mut app = TerraApp::default();
    event_loop.run_app(&mut app).expect("run app");
}
