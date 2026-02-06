//! Install a proxy CA: drop a user CA under `res/raw/` and reference it from a
//! custom network security config.

use super::{PatchInputs, PatchReport};
use anyhow::Result;
use std::path::Path;

pub fn apply(project_dir: &Path, inputs: &PatchInputs) -> Result<PatchReport> {
    let mut report = PatchReport::default();

    let raw_dir = project_dir.join("res").join("raw");
    std::fs::create_dir_all(&raw_dir)?;
    let dest = raw_dir.join("proxy_ca.pem");
    match &inputs.ca_cert {
        Some(src) => {
            std::fs::copy(src, &dest)?;
            report.touched(&dest, project_dir);
        }
        None => {
            report
                .notes
                .push("no --ca-cert provided; wrote a placeholder res/raw/proxy_ca.pem".into());
            std::fs::write(&dest, b"-----BEGIN CERTIFICATE-----\n# replace me\n-----END CERTIFICATE-----\n")?;
            report.touched(&dest, project_dir);
        }
    }

    let xml_dir = project_dir.join("res").join("xml");
    std::fs::create_dir_all(&xml_dir)?;
    let nsc = xml_dir.join("network_security_config.xml");
    std::fs::write(&nsc, NSC_WITH_CA)?;
    report.touched(&nsc, project_dir);

    let manifest = project_dir.join("AndroidManifest.xml");
    if manifest.exists() {
        let text = std::fs::read_to_string(&manifest)?;
        let patched = super::nsc_bypass::wire_into_manifest(&text);
        if patched != text {
            std::fs::write(&manifest, &patched)?;
            report.touched(&manifest, project_dir);
        }
    }
    report.notes.push("proxy CA wired into network_security_config".into());
    Ok(report)
}

const NSC_WITH_CA: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<network-security-config>
    <base-config cleartextTrafficPermitted="true">
        <trust-anchors>
            <certificates src="system" />
            <certificates src="user" />
            <certificates src="@raw/proxy_ca" />
        </trust-anchors>
    </base-config>
</network-security-config>
"#;
