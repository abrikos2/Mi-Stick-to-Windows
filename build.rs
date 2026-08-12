use std::io;

fn main() -> io::Result<()> {
    if std::env::var("CARGO_CFG_TARGET_OS").unwrap() == "windows" {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon.ico");
        res.set("ProductName", "Mi Stick Bridge");
        res.set("FileDescription", "Управление Mi TV Stick с Windows");
        res.set("LegalCopyright", "Copyright © 2026");
        res.set("ProductVersion", env!("CARGO_PKG_VERSION"));
        res.compile()?;
    }
    Ok(())
}