//! Extract `lib/`, `assets/`, and "unknown" files; record compression state.

use crate::apk::Apk;
use anyhow::Result;
use std::collections::BTreeMap;
use std::path::Path;

/// Result of extracting non-code, non-resource payload.
#[derive(Debug, Default)]
pub struct ExtractReport {
    /// Patterns/paths that must stay STORED on rebuild.
    pub do_not_compress: Vec<String>,
    /// Files apktool does not specifically recognize, preserved verbatim.
    pub unknown_files: BTreeMap<String, String>,
}

/// Copy assets/libs/unknown entries out of the APK into the project dir.
pub fn extract(apk: &Apk, out: &Path) -> Result<ExtractReport> {
    let mut report = ExtractReport::default();

    for e in &apk.entries {
        if e.stored && !report.do_not_compress.contains(&e.name) {
            report.do_not_compress.push(e.name.clone());
        }

        if is_handled_elsewhere(&e.name) {
            continue;
        }

        let copied_as_unknown = !(e.name.starts_with("assets/")
            || e.name.starts_with("lib/")
            || e.name.starts_with("kotlin/")
            || e.name.starts_with("META-INF/"));

        let dest = out.join(&e.name);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&dest, &e.data)?;

        if copied_as_unknown {
            report
                .unknown_files
                .insert(e.name.clone(), if e.stored { "0".into() } else { "8".into() });
        }
    }

    Ok(report)
}

/// Entries handled by the manifest/resources/smali pipelines.
fn is_handled_elsewhere(name: &str) -> bool {
    name == "AndroidManifest.xml"
        || name == "resources.arsc"
        || name.starts_with("res/")
        || (name.starts_with("classes") && name.ends_with(".dex") && !name.contains('/'))
}
