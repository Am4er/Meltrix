#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod conv;
mod job;
mod pic;

use eframe::egui;

type Res<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

fn main() -> eframe::Result {
    let opts = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Meltrix")
            .with_inner_size([760.0, 520.0])
            .with_min_inner_size([600.0, 380.0]),
        ..Default::default()
    };
    eframe::run_native("Meltrix", opts, Box::new(|_| Ok(Box::new(app::App::new()))))
}
