//! Embeds the app icon and version info into the Windows executable.
fn main() {
    println!("cargo:rerun-if-changed=assets/icon/icon.ico");
    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon/icon.ico");
        res.set("ProductName", "SpeakUp 6M");
        res.set("FileDescription", "SpeakUp 6M");
        if let Err(e) = res.compile() {
            println!("cargo:warning=could not embed the icon: {e}");
        }
    }
}
