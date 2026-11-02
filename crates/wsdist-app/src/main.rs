#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

use std::process::ExitCode;

fn main() -> ExitCode {
    let resources = match wsdist_app::load_resources() {
        Ok(resources) => resources,
        Err(error) => {
            eprintln!("{error}");
            if let Err(platform) = wsdist_app::show_resource_error(&error) { eprintln!("Cannot present startup error: {platform}"); }
            return ExitCode::FAILURE;
        }
    };
    match wsdist_app::controller::Desktop::new(resources) {
        Ok(desktop) => match desktop.run() {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => { eprintln!("Desktop event loop: {error}"); ExitCode::FAILURE }
        },
        Err(error) => { eprintln!("Cannot open the desktop: {error}"); ExitCode::FAILURE }
    }
}
