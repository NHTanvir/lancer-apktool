//! `arsc` — `resources.arsc` (Android resource table) encoder/decoder.
//!
//! Placeholder API surface; implementation lives in the submodules.

pub mod config;
pub mod table;

pub use table::{encode, ResTable, ResTablePackage};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("truncated resource table: {0}")]
    Truncated(&'static str),
    #[error("bad chunk type 0x{0:04x}")]
    BadChunk(u16),
    #[error("unsupported: {0}")]
    Unsupported(String),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

/// Parse `resources.arsc` bytes into a [`ResTable`].
pub fn parse(bytes: &[u8]) -> Result<ResTable, Error> {
    table::parse(bytes)
}
