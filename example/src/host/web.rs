//! Web entrypoint for the Mara example.
//!
//! The browser path follows the same ownership model as native:
//! egui/Mara owns the shell, and Bevy is just an embedded view inside
//! that shell.

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    eprintln!(
        "mara_example web is a wasm-only binary — build it with \
         `make serve` or `make build TARGET=web`."
    );
}

#[cfg(target_arch = "wasm32")]
fn main() {
    console_error_panic_hook::set_once();
    eframe::WebLogger::init(log::LevelFilter::Debug).ok();

    wasm_bindgen_futures::spawn_local(async {
        let Some(window) = eframe::web_sys::window() else {
            return;
        };
        let Some(document) = window.document() else {
            return;
        };
        let Some(canvas) = document
            .get_element_by_id("mara_canvas")
            .and_then(|element| {
                element
                    .dyn_into::<eframe::web_sys::HtmlCanvasElement>()
                    .ok()
            })
        else {
            eframe::web_sys::console::error_1(&"missing #mara_canvas".into());
            return;
        };

        let runner = eframe::WebRunner::new();
        if let Err(error) = runner
            .start(
                canvas,
                eframe::WebOptions::default(),
                Box::new(|cc| Ok(Box::new(mara_example::DemoApp::new(cc)))),
            )
            .await
        {
            eframe::web_sys::console::error_1(&error);
        }
    });
}

#[cfg(target_arch = "wasm32")]
use eframe::wasm_bindgen::JsCast;
