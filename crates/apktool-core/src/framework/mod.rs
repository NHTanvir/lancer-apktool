//! Framework cache: install / clean / load framework resource tables.

pub mod cache;
pub mod install;

pub use cache::{clean, load_table};
pub use install::install;
