mod backup;
mod config;
mod daemon;
mod gui;
mod tracked;
mod watcher;

use anyhow::{Context, Result};

fn main() -> Result<()> {
    gui::init_logger();
    let app = gui::RepoGuiApp::default();

    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("Repo Saver - REPO save protector")
            .with_inner_size([820.0, 620.0])
            .with_min_inner_size([640.0, 480.0]),
        ..Default::default()
    };

    eframe::run_native(
        "repo-saver",
        options,
        Box::new(
            |_cc: &eframe::CreationContext<'_>| -> Result<Box<dyn eframe::App>, Box<dyn std::error::Error + Send + Sync>> {
                Ok(Box::new(app))
            },
        ),
    )
    .context("Failed to launch GUI")?;
    Ok(())
}