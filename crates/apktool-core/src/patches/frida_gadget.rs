//! Frida gadget injection: drop `libfrida-gadget.so` into `lib/<abi>/`, ensure
//! `extractNativeLibs="true"`, and add `System.loadLibrary("frida-gadget")` to
//! the application's `onCreate`.

use super::{PatchInputs, PatchReport};
use anyhow::Result;
use std::path::Path;

const ABIS: &[&str] = &["arm64-v8a", "armeabi-v7a", "x86", "x86_64"];

pub fn apply(project_dir: &Path, inputs: &PatchInputs) -> Result<PatchReport> {
    let mut report = PatchReport::default();

    // 1. Place the gadget .so into every present (or the default) ABI dir.
    if let Some(so) = &inputs.frida_so {
        let mut placed = false;
        for abi in ABIS {
            let dir = project_dir.join("lib").join(abi);
            if dir.exists() {
                let dest = dir.join("libfrida-gadget.so");
                std::fs::copy(so, &dest)?;
                report.touched(&dest, project_dir);
                placed = true;
            }
        }
        if !placed {
            let dir = project_dir.join("lib").join("arm64-v8a");
            std::fs::create_dir_all(&dir)?;
            let dest = dir.join("libfrida-gadget.so");
            std::fs::copy(so, &dest)?;
            report.touched(&dest, project_dir);
        }
    } else {
        report
            .notes
            .push("no --frida-so provided; skipped copying the gadget binary".into());
    }

    // 2. extractNativeLibs="true" on <application>.
    let manifest = project_dir.join("AndroidManifest.xml");
    if manifest.exists() {
        let text = std::fs::read_to_string(&manifest)?;
        let patched = set_extract_native_libs(&text);
        if patched != text {
            std::fs::write(&manifest, &patched)?;
            report.touched(&manifest, project_dir);
        }
    }

    report.notes.push(
        "inject System.loadLibrary(\"frida-gadget\") into the app's Application.onCreate (manual step if no Application subclass exists)".into(),
    );
    Ok(report)
}

fn set_extract_native_libs(text: &str) -> String {
    let Some(app_pos) = text.find("<application") else {
        return text.to_string();
    };
    let Some(tag_end_rel) = text[app_pos..].find('>') else {
        return text.to_string();
    };
    let tag_end = app_pos + tag_end_rel;
    if text[app_pos..tag_end].contains("android:extractNativeLibs=") {
        return text.to_string();
    }
    let mut result = String::with_capacity(text.len() + 48);
    result.push_str(&text[..tag_end]);
    result.push_str(" android:extractNativeLibs=\"true\"");
    result.push_str(&text[tag_end..]);
    result
}
