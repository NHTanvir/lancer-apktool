//! `apktool.yml` project file (upstream v3.0+ schema).
//!
//! Renamed fields vs. the historical schema: `packageInfo` → `resourcesInfo`,
//! `forcedPackageId` → `packageId`. `compactEntries` (v2.11.0) records whether
//! the source ARSC used the compact entry format.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ApktoolYml {
    pub version: String,
    pub apk_file_name: String,
    pub is_frameworks_only: bool,
    #[serde(default)]
    pub use_aapt2: bool,
    pub sdk_info: SdkInfo,
    pub package_info: ResourcesInfo,
    #[serde(default)]
    pub version_info: VersionInfo,
    #[serde(default)]
    pub resources_are_compressed: bool,
    #[serde(default)]
    pub compact_entries: bool,
    #[serde(default)]
    pub shared_library: bool,
    #[serde(default)]
    pub sparse_resources: bool,
    #[serde(default)]
    pub uses_framework: UsesFramework,
    #[serde(default)]
    pub do_not_compress: Vec<String>,
    #[serde(default)]
    pub unknown_files: std::collections::BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SdkInfo {
    #[serde(default)]
    pub min_sdk_version: String,
    #[serde(default)]
    pub target_sdk_version: String,
    #[serde(default)]
    pub max_sdk_version: String,
}

/// `resourcesInfo:` (was `packageInfo:`).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ResourcesInfo {
    /// `packageId:` (was `forcedPackageId:`).
    #[serde(default)]
    pub package_id: String,
    #[serde(default)]
    pub rename_manifest_package: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct VersionInfo {
    #[serde(default)]
    pub version_code: String,
    #[serde(default)]
    pub version_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct UsesFramework {
    #[serde(default)]
    pub ids: Vec<i64>,
    #[serde(default)]
    pub tag: Option<String>,
}

impl ApktoolYml {
    pub fn load(dir: &Path) -> Result<ApktoolYml> {
        let path = dir.join("apktool.yml");
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("reading {}", path.display()))?;
        // The historical file starts with a `!!brut.androlib...` YAML tag; drop it.
        let cleaned: String = text
            .lines()
            .filter(|l| !l.trim_start().starts_with("!!"))
            .collect::<Vec<_>>()
            .join("\n");
        let yml: ApktoolYml = serde_yaml::from_str(&cleaned).context("parsing apktool.yml")?;
        Ok(yml)
    }

    pub fn save(&self, dir: &Path) -> Result<()> {
        let path = dir.join("apktool.yml");
        let text = serde_yaml::to_string(self).context("serializing apktool.yml")?;
        std::fs::write(&path, text).with_context(|| format!("writing {}", path.display()))?;
        Ok(())
    }
}
