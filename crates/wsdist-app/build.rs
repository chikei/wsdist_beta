fn main() {
    // Introspection drives the real-backend smoke, including extracted release archives.
    let config = slint_build::CompilerConfiguration::new().with_debug_info(true);
    slint_build::compile_with_config("ui/main.slint", config)
        .expect("failed to compile the Slint application UI");

    if std::env::var_os("CARGO_CFG_TARGET_OS").as_deref() == Some(std::ffi::OsStr::new("windows")) {
        println!("cargo:rerun-if-changed=../../icons32/23937.ico");
        winresource::WindowsResource::new()
            .set_icon("../../icons32/23937.ico")
            .compile()
            .expect("failed to compile the Windows application icon");
    }
}
