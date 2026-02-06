//! Enable WebView debugging: flip the `const` feeding
//! `setWebContentsDebuggingEnabled(Z)` from `0x0` to `0x1`.

use super::PatchReport;
use anyhow::Result;
use std::path::Path;

const CALL: &str = "setWebContentsDebuggingEnabled(Z)V";

pub fn apply(project_dir: &Path) -> Result<PatchReport> {
    let mut report = PatchReport::default();
    for path in super::smali_files(project_dir) {
        let text = std::fs::read_to_string(&path)?;
        if !text.contains(CALL) {
            continue;
        }
        let (patched, changed) = rewrite(&text);
        if changed {
            std::fs::write(&path, &patched)?;
            report.touched(&path, project_dir);
        }
    }
    report
        .notes
        .push(format!("patched {} WebView debug call site(s)", report.changed_files.len()));
    Ok(report)
}

/// For each invoke of the target, flip the nearest preceding `const* vX, 0x0`
/// that loads the argument register to `0x1`.
fn rewrite(text: &str) -> (String, bool) {
    let mut lines: Vec<String> = text.lines().map(|s| s.to_string()).collect();
    let mut changed = false;

    for i in 0..lines.len() {
        if !lines[i].contains(CALL) {
            continue;
        }
        // Argument register is the last v-reg in the invoke's register list.
        let arg_reg = invoke_last_reg(&lines[i]);
        // Walk backwards for the const that loads it.
        let start = i.saturating_sub(12);
        for j in (start..i).rev() {
            let l = lines[j].trim_start().to_string();
            if l.starts_with("const") && l.ends_with("0x0") {
                if arg_reg.is_none() || l.contains(&format!("v{},", arg_reg.unwrap())) {
                    let replaced = lines[j].trim_end().trim_end_matches("0x0").to_string() + "0x1";
                    lines[j] = replaced;
                    changed = true;
                    break;
                }
            }
        }
    }
    (lines.join("\n") + "\n", changed)
}

fn invoke_last_reg(line: &str) -> Option<u32> {
    let open = line.find('{')?;
    let close = line.find('}')?;
    let regs = &line[open + 1..close];
    regs.rsplit(',')
        .next()
        .and_then(|r| r.trim().trim_start_matches('v').parse().ok())
}
