mod animation;
mod app;
mod error;
mod material;
mod model;
mod parser;
mod renderer;
mod settings;
mod texture;
mod ui;

use crate::app::handler::AppHandler;
use crate::app::handler_registry;
use crate::error::MdlError;
use crate::renderer::camera::{CameraController, CameraState};
use crate::settings::Settings;
use crate::texture::manager::TextureManager;
use crate::texture::panel::TexturePanel;
use crate::ui::Ui;
use std::ffi::c_void;
use tokio::runtime::Runtime;
use tokio::sync::mpsc;
use winit::event_loop::{ControlFlow, EventLoop};

const CONFY_APP_NAME: &str = "mdlvis-rs";

fn main() -> Result<(), MdlError> {
    std::panic::set_hook(Box::new(|panic_info| {
        let message = if let Some(s) = panic_info.payload().downcast_ref::<&str>() {
            s.to_string()
        } else if let Some(s) = panic_info.payload().downcast_ref::<String>() {
            s.clone()
        } else {
            "未知的程序异常".to_string()
        };

        let location = if let Some(location) = panic_info.location() {
            format!(
                "\n\n位置：{}:{}:{}",
                location.file(),
                location.line(),
                location.column()
            )
        } else {
            String::new()
        };

        let full_message = format!("MDLVis-RS 已崩溃！\n\n{}{}", message, location);

        eprintln!("{}", full_message);

        // Show native error dialog
        #[cfg(not(target_os = "linux"))]
        {
            use rfd::MessageDialog;
            MessageDialog::new()
                .set_title("MDLVis-RS 错误")
                .set_description(&full_message)
                .set_level(rfd::MessageLevel::Error)
                .show();
        }

        #[cfg(target_os = "linux")]
        {
            eprintln!("\n{}\n", "=".repeat(80));
            eprintln!("请将此错误报告给开发者。");
            eprintln!("{}\n", "=".repeat(80));
        }
    }));

    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::Poll);

    let (texture_sender, texture_receiver) = mpsc::unbounded_channel();

    let handler = &mut AppHandler {
        app: None,
        model: None,
        pending_model_path: std::env::args().skip(1).next().map(String::from),
        model_path: None,
        runtime: Runtime::new()?,
        window: None,
        texture_receiver,
        texture_sender,
        current_cursor_pos: None,
        ui: Ui::new(),
        texture_panel: TexturePanel::new(),
        texture_manager: TextureManager::new(),
        camera_controller: CameraController::new(CameraState::default()),
        animation_system: animation::AnimationSystem::new(),
        egui_wants_pointer: false,
        settings: Settings::load(),
        egui_state: None,
        renderer: None,
    };

    handler_registry::register(handler as *mut _ as *mut c_void);

    // Defer window creation to the ApplicationHandler::resumed callback
    // (creating a window before the event loop is active is deprecated).
    event_loop.run_app(handler)?;

    Ok(())
}
