//! Web entry point for the mara UI kit.
//!
//! Compiles to `wasm32-unknown-unknown` and mounts the mara demo
//! onto an HTML `<canvas>` via eframe's [`WebRunner`]. Build and
//! serve with `trunk` — see the repo Makefile's `serve-web` /
//! `build-web` targets, or `api_crates/web/README.md`.
//!
//! The UI ([`demo::DemoApp`]) is host-agnostic egui ported from
//! `bevy_mara --example demo` — the same ribbons, panes, widget
//! gallery, theme picker, canvas whiteboard, and node-graph / code
//! editor, minus the Bevy 3D scene.
//!
//! On non-wasm targets this file is an inert stub so
//! `cargo check --workspace` stays fast and green without dragging
//! eframe / wgpu / winit into a native build.

#[cfg(target_arch = "wasm32")]
mod demo;

/// Native builds: this crate has no native runner. Point the
/// developer at the wasm toolchain instead of failing cryptically.
#[cfg(not(target_arch = "wasm32"))]
fn main() {
    eprintln!(
        "egui_mara_web is a wasm-only crate — build it for the browser with \
         `make serve-web` (trunk). See api_crates/web/README.md."
    );
}

/// wasm builds: hand the mara demo to eframe's `WebRunner`.
#[cfg(target_arch = "wasm32")]
fn main() {
    use eframe::wasm_bindgen::JsCast as _;

    // `WebRunner` installs a panic hook + on-canvas error overlay;
    // `WebLogger` forwards `log` records to the browser devtools.
    eframe::WebLogger::init(log::LevelFilter::Debug).ok();

    let mut web_options = eframe::WebOptions::default();
    // Pin the WebGL2 backend. eframe defaults the wgpu instance to
    // `BROWSER_WEBGPU | GL` and auto-detects WebGPU — on a browser
    // that exposes `navigator.gpu` it keeps WebGPU, but our `wgpu`
    // crate is compiled with only the `webgl` backend, so the
    // detected-but-uncompiled WebGPU path leaves the canvas black.
    // Forcing `Backends::GL` makes the renderer deterministic
    // WebGL2 in every browser.
    if let eframe::egui_wgpu::WgpuSetup::CreateNew(setup) = &mut web_options.wgpu_options.wgpu_setup
    {
        setup.instance_descriptor.backends = eframe::wgpu::Backends::GL;
    }

    wasm_bindgen_futures::spawn_local(async move {
        let canvas = eframe::web_sys::window()
            .and_then(|w| w.document())
            .and_then(|d| d.get_element_by_id("mara_canvas"))
            .expect("index.html must contain <canvas id=\"mara_canvas\">")
            .dyn_into::<eframe::web_sys::HtmlCanvasElement>()
            .expect("#mara_canvas must be a <canvas> element");

        let result = eframe::WebRunner::new()
            .start(
                canvas,
                web_options,
                Box::new(|cc| Ok(Box::new(demo::DemoApp::new(cc)))),
            )
            .await;

        // Surface a startup failure on the page itself — otherwise a
        // failed `start` just leaves a silent black canvas.
        if let Err(err) = result {
            log::error!("egui_mara_web failed to start: {err:?}");
            if let Some(body) = eframe::web_sys::window()
                .and_then(|w| w.document())
                .and_then(|d| d.body())
            {
                let _ = body.set_inner_html(&format!(
                    "<p style=\"color:#e2606a;font-family:monospace;padding:1.5rem;\
                     line-height:1.5\">egui_mara_web failed to start:<br>{err:?}</p>"
                ));
            }
        }
    });
}
