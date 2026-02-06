//! Flip `android:debuggable="true"` on `<application>`.

use super::PatchReport;
use anyhow::Result;
use std::path::Path;

pub fn apply(project_dir: &Path) -> Result<PatchReport> {
    let mut report = PatchReport::default();
    let manifest = project_dir.join("AndroidManifest.xml");
    if !manifest.exists() {
        report.notes.push("no AndroidManifest.xml found".into());
        return Ok(report);
    }
    let text = std::fs::read_to_string(&manifest)?;
    let patched = set_debuggable(&text);
    if patched != text {
        std::fs::write(&manifest, &patched)?;
        report.touched(&manifest, project_dir);
    }
    report.notes.push("android:debuggable set to true".into());
    Ok(report)
}

/// Insert or overwrite `android:debuggable="true"` on the `<application>` tag.
pub fn set_debuggable(text: &str) -> String {
    let Some(app_pos) = text.find("<application") else {
        return text.to_string();
    };
    // End of the opening tag.
    let Some(tag_end_rel) = text[app_pos..].find('>') else {
        return text.to_string();
    };
    let tag_end = app_pos + tag_end_rel;
    let tag = &text[app_pos..tag_end];

    if tag.contains("android:debuggable=") {
        // Replace whatever value it has with "true".
        let mut result = text.to_string();
        if let Some(attr_pos) = result.find("android:debuggable=\"") {
            let start = attr_pos + "android:debuggable=\"".len();
            if let Some(end_rel) = result[start..].find('"') {
                result.replace_range(start..start + end_rel, "true");
            }
        }
        result
    } else {
        let mut result = String::with_capacity(text.len() + 32);
        result.push_str(&text[..tag_end]);
        result.push_str(" android:debuggable=\"true\"");
        result.push_str(&text[tag_end..]);
        result
    }
}
