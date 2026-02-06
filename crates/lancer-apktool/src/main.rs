//! `lancer-apktool` entry point.
//!
//! Plain invocation opens the native GUI; any subcommand (or `--cli`) runs the
//! command-line surface. For CI/headless use, the `lancer-apktool-cli`
//! companion binary always runs the CLI.

use anyhow::Result;

fn main() -> Result<()> {
    init_tracing();

    let cli = lancer_apktool::cli::parse();
    let run_cli = cli.command.is_some() || cli.cli;

    if run_cli {
        lancer_apktool::cli::dispatch(cli)
    } else {
        launch_gui()
    }
}

fn init_tracing() {
    let filter = std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string());
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .without_time()
        .try_init();
}

#[cfg(feature = "gui")]
fn launch_gui() -> Result<()> {
    lancer_apktool::gui::launch()
}

#[cfg(not(feature = "gui"))]
fn launch_gui() -> Result<()> {
    use clap::CommandFactory;
    lancer_apktool::cli::Cli::command().print_help()?;
    println!();
    Ok(())
}
