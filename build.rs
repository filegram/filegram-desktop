//! Embeds the application icon as a Windows resource into the .exe.
//! A no-op on every other platform; `winresource` is a Windows-only
//! build-dependency, so the non-Windows variant must not reference it.

#[cfg(windows)]
fn main() {
    println!("cargo:rerun-if-changed=assets/icon/filegram.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        winresource::WindowsResource::new()
            .set_icon("assets/icon/filegram.ico")
            .compile()
            .expect("failed to embed assets/icon/filegram.ico into the executable");
    }
}

#[cfg(not(windows))]
fn main() {}
