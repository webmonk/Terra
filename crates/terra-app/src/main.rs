//! Terra editor application shell.

use terra_app::app::TerraApp;
use winit::event_loop::{ControlFlow, EventLoop};

fn main() {
    harden_gpu_env();
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let event_loop = EventLoop::new().expect("event loop");
    // Wait on OS events; about_to_wait arms WaitUntil only while refining.
    event_loop.set_control_flow(ControlFlow::Wait);

    let mut app = TerraApp::default();
    event_loop.run_app(&mut app).expect("run app");
}

/// Soften Vulkan capture-overlay damage when `WGPU_BACKEND=vulkan` is forced.
///
/// OBS / Overwolf / Medal register implicit layers that have stack-overflowed
/// `vkCreateDevice` on this machine. DX12 is the Windows default; these env
/// vars only matter on the Vulkan path.
fn harden_gpu_env() {
    const CAPTURE_DISABLE: &[(&str, &str)] = &[
        ("DISABLE_VULKAN_OBS_CAPTURE", "1"),
        ("DISABLE_VULKAN_OW_OBS_CAPTURE", "1"),
        ("DISABLE_VULKAN_OW_OVERLAY_LAYER", "1"),
        ("DISABLE_VULKAN_MEDAL_OBS_CAPTURE", "1"),
    ];
    for &(key, value) in CAPTURE_DISABLE {
        if std::env::var_os(key).is_none() {
            // SAFETY: called once at process start before other threads.
            unsafe { std::env::set_var(key, value) };
        }
    }
    if std::env::var_os("VK_LOADER_LAYERS_DISABLE").is_none() {
        unsafe {
            std::env::set_var(
                "VK_LOADER_LAYERS_DISABLE",
                "~VK_LAYER_OBS_HOOK:~VK_LAYER_OW_OBS_HOOK:~VK_LAYER_OW_Overlay:~VK_LAYER_MEDAL_HOOK",
            );
        }
    }
}
