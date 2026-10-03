pub mod resources;
pub mod exchange;
pub mod input_metadata;
pub mod profile_registry;
pub mod profiles;
pub mod selections;
pub mod state;
pub mod charts;
pub mod controller;
pub mod desktop_inputs;
pub mod desktop_models;
pub mod jobs;
pub mod reports;

pub use resources::{AppPaths, ResourceError, Resources, load_resources, load_resources_at};

use slint::ComponentHandle;

slint::include_modules!();

/// Creates the startup-only error window on Slint's event-loop thread.
pub fn create_resource_error_window(
    error: &ResourceError,
) -> Result<ResourceErrorWindow, slint::PlatformError> {
    let window = ResourceErrorWindow::new()?;
    window.set_error_detail(error.to_string().into());
    let weak = window.as_weak();
    window.on_close_requested(move || {
        if let Some(window) = weak.upgrade()
            && let Err(error) = window.hide()
        {
            window.set_error_detail(format!("{}\nUnable to close the window: {error}", window.get_error_detail()).into());
        }
    });
    Ok(window)
}

/// Presents a startup failure before any main window is opened. Close hides this
/// last window, causing Slint's event loop to return. Backend errors propagate.
pub fn show_resource_error(error: &ResourceError) -> Result<(), slint::PlatformError> {
    create_resource_error_window(error)?.run()
}
