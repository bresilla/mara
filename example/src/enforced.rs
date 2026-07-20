//! Minimal enforcement test — a deliberately "naughty" Mara consumer.
//!
//! This app is hosted by **eframe** (a foreign egui host, not the Mara
//! runner) and does everything a misbehaving library consumer does:
//! it never applies a theme, never publishes a shelf layout, and never
//! renders the `ShellBar`. It draws exactly one Mara surface.
//!
//! What you should see: the Mara theme, a sane layout, and the
//! **enforced top bar** — all provided by `mara_core::enforce` the
//! moment the first surface draws. Ticking the toggle exercises the
//! single escape hatch (`opt_out_shell_bar`, an explicit per-frame
//! call): the bar disappears while it is held on and comes right back
//! when it is turned off.
//!
//! Run with: `cargo run -p mara_example --bin enforced`

#[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
fn main() -> eframe::Result<()> {
    // Restrict wgpu to the primary backends (Vulkan/Metal/DX12). The
    // default set also probes GL/EGL, which panics on the Nix GL stack
    // this repo develops under; the Mara runner is Vulkan-only for the
    // same reason.
    let mut options = eframe::NativeOptions::default();
    let mut setup = eframe::egui_wgpu::WgpuSetupCreateNew::without_display_handle();
    setup.instance_descriptor.backends = wgpu::Backends::PRIMARY;
    options.wgpu_options.wgpu_setup = eframe::egui_wgpu::WgpuSetup::CreateNew(setup);

    eframe::run_native(
        "Mara enforcement test",
        options,
        Box::new(|_cc| Ok(Box::new(NaughtyApp::default()))),
    )
}

#[cfg(any(target_arch = "wasm32", target_os = "android"))]
fn main() {}

#[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
#[derive(Default)]
struct NaughtyApp {
    opt_out: bool,
}

#[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
impl eframe::App for NaughtyApp {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let render_state = frame
            .wgpu_render_state()
            .expect("eframe must run with the wgpu backend");
        let host = mara::host::MaraHostCtx::ui_only(ui.ctx(), Some(render_state));

        // The ONLY thing this app does: draw one root surface. No
        // theme, no shelf layout, no ShellBar — enforcement supplies
        // all three.
        host.show_root_body(mara::ui::style::active_accent(), |mui, _rect| {
            mui.label("this app never draws the top bar — Mara enforces it anyway");
            mui.label("(no theme applied, no shelf layout published either)");
            let _ = mui.toggle("opt out of the enforced bar (explicit, per-frame)", &mut self.opt_out);
        });

        // The single deliberate escape hatch: a per-frame decision.
        // Stop calling it (toggle off) and the bar returns by itself.
        if self.opt_out {
            host.opt_out_shell_bar();
        }
    }
}
