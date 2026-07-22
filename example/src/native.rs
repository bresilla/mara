// The native bin is desktop-only; on Android the entry point is the
// `cdylib`'s `android_main` (see `lib.rs`), so this binary is a no-op
// there to keep `cargo build` over the whole package green.
#[cfg(not(target_os = "android"))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    mara::window::run::<mara_example::DemoApp>()
}

#[cfg(target_os = "android")]
fn main() {}
