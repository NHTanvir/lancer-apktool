//! `lancer-apktool` frontend: a clap CLI plus an egui GUI, both driving
//! `apktool-core`.

pub mod cli;

#[cfg(feature = "gui")]
pub mod gui;
