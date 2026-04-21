// Embeds assets/icon.ico into the Windows .exe as the file icon Explorer
// shows. Runs at build time; no-op on Linux and macOS. Uses CARGO_CFG_TARGET_OS
// rather than #[cfg(target_os = ...)] so cross-compilation from Linux to
// Windows (via mingw) gets the right branch — the host cfg would be "linux".
fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon.ico");
        // Fail the build if the icon can't be embedded — silent failure here
        // would ship an icon-less exe.
        res.compile()
            .expect("failed to embed Windows icon resource");
    }
}
