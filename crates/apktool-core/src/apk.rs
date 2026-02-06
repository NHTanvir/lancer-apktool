//! APK (zip) reading/writing with per-entry compression bookkeeping.

use anyhow::{Context, Result};
use std::io::{Cursor, Read, Write};
use std::path::Path;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

/// A single APK entry plus whether it was STORED (uncompressed) in the original.
#[derive(Debug, Clone)]
pub struct ApkEntry {
    pub name: String,
    pub data: Vec<u8>,
    pub stored: bool,
}

/// An in-memory APK.
#[derive(Debug, Clone, Default)]
pub struct Apk {
    pub entries: Vec<ApkEntry>,
}

impl Apk {
    pub fn open(path: &Path) -> Result<Apk> {
        let bytes = std::fs::read(path)
            .with_context(|| format!("reading APK {}", path.display()))?;
        Apk::from_bytes(&bytes)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Apk> {
        let mut zip = ZipArchive::new(Cursor::new(bytes)).context("opening APK as zip")?;
        let mut entries = Vec::with_capacity(zip.len());
        for i in 0..zip.len() {
            let mut f = zip.by_index(i)?;
            if f.is_dir() {
                continue;
            }
            let stored = f.compression() == CompressionMethod::Stored;
            let mut data = Vec::with_capacity(f.size() as usize);
            f.read_to_end(&mut data)?;
            entries.push(ApkEntry {
                name: f.name().to_string(),
                data,
                stored,
            });
        }
        Ok(Apk { entries })
    }

    pub fn entry(&self, name: &str) -> Option<&ApkEntry> {
        self.entries.iter().find(|e| e.name == name)
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.entries.iter().map(|e| e.name.as_str())
    }
}

/// Write an APK from entries, honoring each entry's STORED/DEFLATED flag.
pub fn write_apk(path: &Path, entries: &[ApkEntry]) -> Result<()> {
    let bytes = write_apk_bytes(entries)?;
    std::fs::write(path, bytes).with_context(|| format!("writing APK {}", path.display()))?;
    Ok(())
}

/// Serialize entries into APK bytes.
pub fn write_apk_bytes(entries: &[ApkEntry]) -> Result<Vec<u8>> {
    let mut cursor = Cursor::new(Vec::new());
    {
        let mut zip = ZipWriter::new(&mut cursor);
        for e in entries {
            let method = if e.stored || must_store(&e.name) {
                CompressionMethod::Stored
            } else {
                CompressionMethod::Deflated
            };
            let opts = SimpleFileOptions::default().compression_method(method);
            zip.start_file(&e.name, opts)?;
            zip.write_all(&e.data)?;
        }
        zip.finish()?;
    }
    Ok(cursor.into_inner())
}

/// Entries that MUST be stored uncompressed for the runtime to mmap them.
pub fn must_store(name: &str) -> bool {
    name == "resources.arsc" || name.ends_with(".arsc")
}
