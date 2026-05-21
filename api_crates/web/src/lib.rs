//! Web helpers for `egui_mara`.
//!
//! This crate intentionally no longer hosts the Mara demo. The
//! runnable browser example lives at the repo root in `example/`,
//! consuming `egui_mara` like an external application.

#[cfg(target_arch = "wasm32")]
pub use egui_mara;

#[cfg(target_arch = "wasm32")]
pub use eframe;

/// Force eframe's wgpu setup to WebGL2.
///
/// This keeps browser builds deterministic when the crate is
/// compiled with `wgpu/webgl` only: if the browser exposes WebGPU,
/// eframe may otherwise auto-select a backend that was not compiled
/// into the binary.
#[cfg(target_arch = "wasm32")]
pub fn force_webgl2(options: &mut eframe::WebOptions) {
    if let eframe::egui_wgpu::WgpuSetup::CreateNew(setup) = &mut options.wgpu_options.wgpu_setup {
        setup.instance_descriptor.backends = eframe::wgpu::Backends::GL;
    }
}

/// Native builds intentionally do nothing; use `example/` for the
/// runnable desktop/web demo.
#[cfg(not(target_arch = "wasm32"))]
pub fn crate_has_no_native_runner() {}
