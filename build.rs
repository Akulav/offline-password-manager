fn main() {
    // Embed the shield icon and version info into the Windows executable.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/shield.ico");
        res.set("ProductName", "SeePass");
        res.set("FileDescription", "SeePass offline password manager");
        if let Err(e) = res.compile() {
            println!("cargo:warning=could not embed Windows resources: {e}");
        }
    }
    println!("cargo:rerun-if-changed=assets/shield.ico");
}
