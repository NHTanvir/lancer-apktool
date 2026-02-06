//! SSL-pinning bypass: blank out `checkServerTrusted` / `checkClientTrusted` /
//! `getAcceptedIssuers` / `verify` method bodies to `return-void`.

use super::PatchReport;
use anyhow::Result;
use std::path::Path;

const TARGET_METHODS: &[&str] = &[
    "checkServerTrusted",
    "checkClientTrusted",
    "getAcceptedIssuers",
];

pub fn apply(project_dir: &Path) -> Result<PatchReport> {
    let mut report = PatchReport::default();
    for path in super::smali_files(project_dir) {
        let text = std::fs::read_to_string(&path)?;
        let (new_text, changed) = rewrite(&text);
        if changed {
            std::fs::write(&path, &new_text)?;
            report.touched(&path, project_dir);
        }
    }
    report
        .notes
        .push(format!("neutralized {} trust-manager method(s)", report.changed_files.len()));
    Ok(report)
}

/// Replace the body of each targeted method with `return-void`.
fn rewrite(text: &str) -> (String, bool) {
    let mut out = String::with_capacity(text.len());
    let mut changed = false;
    let mut lines = text.lines().peekable();

    while let Some(line) = lines.next() {
        out.push_str(line);
        out.push('\n');
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix(".method ") {
            let is_target = TARGET_METHODS.iter().any(|m| rest.contains(&format!("{}(", m)));
            // X509TrustManager methods return void; only blank void methods.
            let returns_void = rest.trim_end().ends_with(")V");
            if is_target && returns_void {
                // Skip original body until `.end method`, emitting a stub.
                out.push_str("    .registers 8\n");
                out.push_str("    return-void\n");
                for body in lines.by_ref() {
                    if body.trim() == ".end method" {
                        out.push_str(".end method\n");
                        break;
                    }
                }
                changed = true;
            }
        }
    }
    (out, changed)
}
