//! `classes*.dex` → `smali/`, `smali_classes2/`, ... trees.

use crate::apk::Apk;
use anyhow::{Context, Result};
use std::path::Path;

/// Disassemble every (or just the main) DEX into smali trees.
pub fn decode_smali(apk: &Apk, out: &Path, all_src: bool) -> Result<()> {
    let mut dexes: Vec<(&str, &[u8])> = apk
        .entries
        .iter()
        .filter(|e| is_dex(&e.name))
        .map(|e| (e.name.as_str(), e.data.as_slice()))
        .collect();
    dexes.sort_by_key(|(n, _)| dex_order(n));

    for (name, data) in dexes {
        // v3 default: main classes only unless --all-src.
        if !all_src && name != "classes.dex" {
            continue;
        }
        let dir = smali_dir_for(name);
        let files = smali::disassemble(data)
            .with_context(|| format!("disassembling {}", name))?;
        for f in files {
            let dest = out.join(&dir).join(&f.path);
            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&dest, f.text)?;
        }
    }
    Ok(())
}

/// Copy `classes*.dex` verbatim when smali decoding is disabled (`-s`).
pub fn copy_dex(apk: &Apk, out: &Path) -> Result<()> {
    for e in &apk.entries {
        if is_dex(&e.name) {
            std::fs::write(out.join(&e.name), &e.data)?;
        }
    }
    Ok(())
}

fn is_dex(name: &str) -> bool {
    name.starts_with("classes") && name.ends_with(".dex") && !name.contains('/')
}

fn dex_order(name: &str) -> u32 {
    if name == "classes.dex" {
        return 1;
    }
    name.trim_start_matches("classes")
        .trim_end_matches(".dex")
        .parse::<u32>()
        .unwrap_or(999)
}

fn smali_dir_for(name: &str) -> String {
    if name == "classes.dex" {
        "smali".to_string()
    } else {
        let n = name.trim_start_matches("classes").trim_end_matches(".dex");
        format!("smali_classes{}", n)
    }
}
