//! Host glue: the parts of the demo that legitimately hold backend
//! types.
//!
//! PLAN.md WS-F6. `eframe::App` is an egui-facing trait — implementing
//! it means naming `egui::Ui`, `egui::Context` and `egui::Visuals`, and
//! no amount of sealing changes that. The honest answer is a declared
//! boundary rather than a pretend-sealed one, exactly as `hosts/*` is
//! for renderer-owning crates.
//!
//! **Everything outside this module is checked at zero** by `make
//! check`. If a raw egui type appears in `example/src/*.rs`, that is a
//! demo reaching past the seal; if it appears here, it is the demo
//! being a host.

use crate::DemoApp;
use mara::host::MaraHostCtx;

/// Translate an eframe pass into a Mara host context.
///
/// The whole of the demo's backend contact: build a `MaraHostCtx` from
/// egui's context plus eframe's wgpu render state, then hand off to
/// [`DemoApp::update_frame`], which names no backend type.
#[cfg(not(target_os = "android"))]
impl eframe::App for DemoApp {
    #[cfg(target_arch = "wasm32")]
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        // The web Bevy view is a real browser canvas behind/inside
        // Mara's transparent egui canvas. Do not clear the whole
        // eframe canvas opaquely or it hides Bevy.
        [0.0, 0.0, 0.0, 0.0]
    }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let render_state = frame
            .wgpu_render_state()
            .expect("eframe must run with the wgpu backend (see example/Cargo.toml)");
        let mut host = MaraHostCtx::ui_only(ui.ctx(), Some(render_state));
        self.update_frame(&mut host);
    }
}
