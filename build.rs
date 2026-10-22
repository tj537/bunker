//! On Windows: embeds the app icon and version info into solarum.exe.
//! On other systems this script does nothing.

fn main() {
    println!("cargo:rerun-if-changed=assets/icon.ico");
    // Only when building on Windows for Windows – that's where the resource compiler exists
    if cfg!(windows) && std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon.ico")
            .set("ProductName", "Solarum")
            .set("FileDescription", "Solarum – techno beats in orbit")
            .set("LegalCopyright", "MIT License");
        if let Err(e) = res.compile() {
            println!("cargo:warning=could not embed the Windows icon: {e}");
        }
    }
}
