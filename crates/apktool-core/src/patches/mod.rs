//! `patch --preset` (Lancer extension): canned pentest transforms over a
//! decoded project tree.

pub mod debuggable;
pub mod frida_gadget;
pub mod nsc_bypass;
pub mod pin_bypass;
pub mod proxy_ca;
pub mod webview_debug;

use anyhow::Result;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

/// The available presets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Preset {
    PinBypass,
    FridaGadget,
    Debuggable,
    WebviewDebug,
    NscBypass,
    ProxyCa,
}

impl Preset {
    pub fn parse(s: &str) -> Option<Preset> {
        Some(match s.to_lowercase().replace('_', "-").as_str() {
            "pin-bypass" => Preset::PinBypass,
            "frida-gadget" => Preset::FridaGadget,
            "debuggable" => Preset::Debuggable,
            "webview-debug" => Preset::WebviewDebug,
            "nsc-bypass" => Preset::NscBypass,
            "proxy-ca" => Preset::ProxyCa,
            _ => return None,
        })
    }
}

/// Optional inputs some presets need (a .so to inject, a CA to install).
#[derive(Debug, Clone, Default)]
pub struct PatchInputs {
    pub frida_so: Option<PathBuf>,
    pub ca_cert: Option<PathBuf>,
}

/// Summary of what a preset changed.
#[derive(Debug, Default, Clone)]
pub struct PatchReport {
    pub changed_files: Vec<String>,
    pub notes: Vec<String>,
}

impl PatchReport {
    pub fn touched(&mut self, path: &Path, base: &Path) {
        if let Ok(rel) = path.strip_prefix(base) {
            self.changed_files.push(rel.to_string_lossy().replace('\\', "/"));
        }
    }
}

/// Apply a preset to a decoded project directory.
pub fn apply(project_dir: &Path, preset: Preset, inputs: &PatchInputs) -> Result<PatchReport> {
    match preset {
        Preset::PinBypass => pin_bypass::apply(project_dir),
        Preset::FridaGadget => frida_gadget::apply(project_dir, inputs),
        Preset::Debuggable => debuggable::apply(project_dir),
        Preset::WebviewDebug => webview_debug::apply(project_dir),
        Preset::NscBypass => nsc_bypass::apply(project_dir),
        Preset::ProxyCa => proxy_ca::apply(project_dir, inputs),
    }
}

/// Iterate every `.smali` file in the project.
pub(crate) fn smali_files(project_dir: &Path) -> Vec<PathBuf> {
    WalkDir::new(project_dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .map(|e| e.into_path())
        .filter(|p| p.extension().and_then(|x| x.to_str()) == Some("smali"))
        .collect()
}
