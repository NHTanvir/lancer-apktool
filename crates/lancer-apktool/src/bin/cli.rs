//! `lancer-apktool-cli` — the headless CLI companion binary (never opens a GUI).

use anyhow::Result;

fn main() -> Result<()> {
    let filter = std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string());
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .without_time()
        .try_init();

    let cli = lancer_apktool::cli::parse();
    lancer_apktool::cli::dispatch(cli)
}
