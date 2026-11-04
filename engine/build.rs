fn main() {
    // Mounting loads libfuse3 explicitly; an absent driver must not prevent daemon startup.
    #[cfg(target_os = "macos")]
    println!("cargo:rustc-link-arg=-Wl,-dead_strip_dylibs,-weak-lfuse");
    #[cfg(windows)]
    winfsp::build::winfsp_link_delayload();
}
