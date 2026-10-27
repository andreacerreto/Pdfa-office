#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1050.0, 760.0])
            .with_min_inner_size([800.0, 640.0])
            .with_icon(pdfa_office::ui::window_icon()),
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };
    eframe::run_native(
        "PDF/A Office",
        options,
        Box::new(|cc| Ok(Box::new(pdfa_office::ui::PdfaApp::new(cc)))),
    )
}
