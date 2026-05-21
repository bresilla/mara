//! Web entry point for the root Mara example.

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    eprintln!(
        "mara_example web is a wasm-only binary — build it with \
         `make serve-web` or `make build-web`."
    );
}

#[cfg(target_arch = "wasm32")]
fn main() {
    use eframe::wasm_bindgen::JsCast as _;
    use mara_example::DemoApp;

    eframe::WebLogger::init(log::LevelFilter::Debug).ok();

    let mut web_options = eframe::WebOptions::default();
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
                Box::new(|cc| Ok(Box::new(DemoApp::new(cc)))),
            )
            .await;

        if let Err(err) = result {
            log::error!("mara_example web failed to start: {err:?}");
            if let Some(body) = eframe::web_sys::window()
                .and_then(|w| w.document())
                .and_then(|d| d.body())
            {
                body.set_inner_html(&format!(
                    "<p style=\"color:#e2606a;font-family:monospace;padding:1.5rem;\
                     line-height:1.5\">mara_example failed to start:<br>{err:?}</p>"
                ));
            }
        }
    });
}
