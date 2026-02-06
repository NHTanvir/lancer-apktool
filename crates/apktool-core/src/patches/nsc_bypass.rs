//! Network Security Config bypass: write a permissive `network_security_config`
//! and wire it into `<application android:networkSecurityConfig=...>`.

use super::PatchReport;
use anyhow::Result;
use std::path::Path;

pub const PERMISSIVE_NSC: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<network-security-config>
    <base-config cleartextTrafficPermitted="true">
        <trust-anchors>
            <certificates src="system" />
            <certificates src="user" />
        </trust-anchors>
    </base-config>
</network-security-config>
"#;

pub fn apply(project_dir: &Path) -> Result<PatchReport> {
    let mut report = PatchReport::default();

    let xml_dir = project_dir.join("res").join("xml");
    std::fs::create_dir_all(&xml_dir)?;
    let nsc_path = xml_dir.join("network_security_config.xml");
    std::fs::write(&nsc_path, PERMISSIVE_NSC)?;
    report.touched(&nsc_path, project_dir);

    let manifest = project_dir.join("AndroidManifest.xml");
    if manifest.exists() {
        let text = std::fs::read_to_string(&manifest)?;
        let patched = wire_into_manifest(&text);
        if patched != text {
            std::fs::write(&manifest, &patched)?;
            report.touched(&manifest, project_dir);
        }
    }
    report
        .notes
        .push("permissive network_security_config installed".into());
    Ok(report)
}

/// Add `android:networkSecurityConfig="@xml/network_security_config"` to the
/// `<application>` tag if absent.
pub fn wire_into_manifest(text: &str) -> String {
    let Some(app_pos) = text.find("<application") else {
        return text.to_string();
    };
    let Some(tag_end_rel) = text[app_pos..].find('>') else {
        return text.to_string();
    };
    let tag_end = app_pos + tag_end_rel;
    if text[app_pos..tag_end].contains("android:networkSecurityConfig=") {
        return text.to_string();
    }
    let mut result = String::with_capacity(text.len() + 64);
    result.push_str(&text[..tag_end]);
    result.push_str(" android:networkSecurityConfig=\"@xml/network_security_config\"");
    result.push_str(&text[tag_end..]);
    result
}
