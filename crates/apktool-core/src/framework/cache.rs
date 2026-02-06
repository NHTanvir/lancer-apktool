//! Framework cache directory management.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

/// Resolve the framework cache directory, honoring an explicit `--frame-path`.
pub fn cache_dir(frame_path: Option<&Path>) -> PathBuf {
    if let Some(p) = frame_path {
        return p.to_path_buf();
    }
    directories::ProjectDirs::from("io", "lancer", "lancer-apktool")
        .map(|d| d.data_dir().join("framework"))
        .unwrap_or_else(|| PathBuf::from("framework"))
}

/// Compute the on-disk filename for a framework id + optional tag.
pub fn framework_file(dir: &Path, id: u32, tag: Option<&str>) -> PathBuf {
    match tag {
        Some(t) if !t.is_empty() => dir.join(format!("{}-{}.apk", id, t)),
        _ => dir.join(format!("{}.apk", id)),
    }
}

/// Load a framework's parsed resource table, if cached.
pub fn load_table(
    frame_path: Option<&Path>,
    tag: Option<&str>,
    id: u32,
) -> Result<Option<arsc::ResTable>> {
    let dir = cache_dir(frame_path);
    let file = framework_file(&dir, id, tag);
    if !file.exists() {
        return Ok(None);
    }
    let apk = crate::apk::Apk::open(&file)?;
    let Some(entry) = apk.entry("resources.arsc") else {
        return Ok(None);
    };
    let table = arsc::parse(&entry.data).context("parsing framework resources.arsc")?;
    Ok(Some(table))
}

/// Wipe the framework cache (optionally a single tag, or everything).
pub fn clean(frame_path: Option<&Path>, tag: Option<&str>, all: bool) -> Result<()> {
    let dir = cache_dir(frame_path);
    if !dir.exists() {
        return Ok(());
    }
    if all || tag.is_none() {
        for entry in std::fs::read_dir(&dir)? {
            let entry = entry?;
            if entry.path().extension().and_then(|e| e.to_str()) == Some("apk") {
                std::fs::remove_file(entry.path())?;
            }
        }
    } else if let Some(t) = tag {
        for entry in std::fs::read_dir(&dir)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.contains(&format!("-{}.apk", t)) {
                std::fs::remove_file(entry.path())?;
            }
        }
    }
    Ok(())
}
