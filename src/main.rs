mod app;
mod apply;
mod loader;
mod ui;
mod wallpapers;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Cherry")
            .with_app_id("uk.co.ryannavsaria.cherry")
            .with_inner_size([900.0, 520.0])
            .with_min_inner_size([900.0, 520.0])
            .with_max_inner_size([900.0, 520.0])
            .with_decorations(false)
            .with_transparent(true)
            .with_resizable(false),
        ..Default::default()
    };
    eframe::run_native(
        "cherry",
        options,
        Box::new(|cc| Ok(Box::new(app::CherryApp::new(cc)))),
    )
}
