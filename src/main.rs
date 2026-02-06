//! lancer-apktool
//! Rust clone of apktool. See plan.md (local, gitignored) for the spec.

use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "lancer-apktool", version, about = "Rust clone of apktool")]
struct Cli {}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    let _cli = Cli::parse();
    println!("lancer-apktool - scaffold ready. Implement per plan.md.");
    Ok(())
}
