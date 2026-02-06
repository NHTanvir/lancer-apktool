//! Native GUI frontend (eframe/egui).

mod app;
mod server;

/// Launch the desktop GUI. Blocks until the window is closed.
pub fn launch() -> anyhow::Result<()> {
    if let Some(port) = server::start() {
        tracing::info!("lancer-apktool discovery server on 127.0.0.1:{port}");
    }

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1100.0, 700.0])
            .with_title("lancer-apktool"),
        ..Default::default()
    };

    eframe::run_native(
        "lancer-apktool",
        native_options,
        Box::new(|_cc| Ok(Box::<app::LancerApp>::default())),
    )
    .map_err(|e| anyhow::anyhow!("GUI error: {e}"))
}
