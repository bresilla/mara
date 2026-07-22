//! Root Mara example application.
//!
//! This crate consumes `mara`, `mara_core`, and standalone Mara modules
//! the same way downstream applications should.

pub mod app;
pub mod bevy_content;

pub use app::DemoApp;

/// Android entry point.
///
/// Android has no `main`; the OS calls `android_main(AndroidApp)` in the
/// `cdylib` (NativeActivity loads `libmara_example.so` and invokes this
/// symbol). We hand the `AndroidApp` to the Mara Android runner, which
/// drives the same [`DemoApp`] as desktop.
#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
fn android_main(app: mara::android::AndroidApp) {
    mara::android::run_android::<DemoApp>(app);
}
