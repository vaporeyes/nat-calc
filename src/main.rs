// ABOUTME: Entry point for the egui/eframe GUI, native and wasm.
// ABOUTME: Native uses run_native; wasm boots eframe onto a canvas.

mod app;

use app::CalcApp;

#[cfg(not(target_arch = "wasm32"))]
fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1000.0, 720.0])
            .with_min_inner_size([640.0, 480.0])
            .with_title("nat-calc"),
        ..Default::default()
    };
    eframe::run_native(
        "nat-calc",
        options,
        Box::new(|cc| Ok(Box::new(CalcApp::new(cc)))),
    )
}

#[cfg(target_arch = "wasm32")]
fn main() {
    use eframe::wasm_bindgen::JsCast as _;

    let web_options = eframe::WebOptions::default();

    wasm_bindgen_futures::spawn_local(async {
        let canvas = web_sys::window()
            .expect("no window")
            .document()
            .expect("no document")
            .get_element_by_id("the_canvas_id")
            .expect("missing canvas element")
            .dyn_into::<web_sys::HtmlCanvasElement>()
            .expect("element was not a canvas");

        eframe::WebRunner::new()
            .start(
                canvas,
                web_options,
                Box::new(|cc| Ok(Box::new(CalcApp::new(cc)))),
            )
            .await
            .expect("failed to start eframe");
    });
}
