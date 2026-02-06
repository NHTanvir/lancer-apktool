//! `install-framework` / `if`: ingest a framework APK into the cache.

use crate::framework::cache;
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

/// Install a framework APK into the cache, keyed by its package id (+ tag).
pub fn install(apk: &Path, frame_path: Option<&Path>, tag: Option<&str>) -> Result<PathBuf> {
    let dir = cache::cache_dir(frame_path);
    std::fs::create_dir_all(&dir)
        .with_context(|| format!("creating framework cache {}", dir.display()))?;

    let loaded = crate::apk::Apk::open(apk)?;
    let pkg_id = loaded
        .entry("resources.arsc")
        .and_then(|e| arsc::parse(&e.data).ok())
        .and_then(|t| t.packages.first().map(|p| p.id))
        .unwrap_or(1);

    let dest = cache::framework_file(&dir, pkg_id, tag);
    std::fs::copy(apk, &dest)
        .with_context(|| format!("copying framework to {}", dest.display()))?;
    tracing::info!("installed framework {} as {}", apk.display(), dest.display());
    Ok(dest)
}
