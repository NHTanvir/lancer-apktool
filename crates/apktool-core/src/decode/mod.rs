//! Top-level decode pipeline: APK → editable project tree.

pub mod library;
pub mod manifest;
pub mod resources;
pub mod smali_decode;

use crate::apk::Apk;
use crate::apktool_yml::{ApktoolYml, ResourcesInfo, SdkInfo, VersionInfo};
use crate::framework;
use crate::resolver::TableResolver;
use crate::DecodeOptions;
use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};

/// Decode `apk_path` into an editable project directory.
pub fn decode(apk_path: &Path, opts: &DecodeOptions) -> Result<PathBuf> {
    let out = opts.output.clone().unwrap_or_else(|| {
        let stem = apk_path.file_stem().and_then(|s| s.to_str()).unwrap_or("out");
        PathBuf::from(stem)
    });

    if out.exists() {
        if opts.force {
            std::fs::remove_dir_all(&out)
                .with_context(|| format!("clearing existing {}", out.display()))?;
        } else {
            bail!(
                "output directory {} already exists (use -f to overwrite)",
                out.display()
            );
        }
    }
    std::fs::create_dir_all(&out)?;

    let apk = Apk::open(apk_path)?;
    tracing::info!("decoding {} ({} entries)", apk_path.display(), apk.entries.len());

    // Resource table + framework table → reference resolver.
    let app_table = apk
        .entry("resources.arsc")
        .and_then(|e| arsc::parse(&e.data).ok());
    let fw_table = framework::load_table(opts.frame_path.as_deref(), opts.frame_tag.as_deref(), 1)
        .ok()
        .flatten();
    let resolver = TableResolver::new(app_table.clone(), fw_table);

    // Manifest.
    let mut manifest_info = manifest::ManifestInfo::default();
    if let Some(m) = apk.entry("AndroidManifest.xml") {
        let text = manifest::decode(&m.data, &resolver)?;
        std::fs::write(out.join("AndroidManifest.xml"), text)?;
        manifest_info = manifest::info(&m.data).unwrap_or_default();
    }

    // Resources.
    if opts.no_res {
        if let Some(e) = apk.entry("resources.arsc") {
            std::fs::write(out.join("resources.arsc"), &e.data)?;
        }
    } else if let Some(table) = &app_table {
        resources::emit_values(table, &resolver, &out).context("emitting values resources")?;
        resources::decode_res_files(&apk, &resolver, &out).context("decoding res files")?;
    }

    // Smali.
    if opts.no_src {
        smali_decode::copy_dex(&apk, &out)?;
    } else {
        smali_decode::decode_smali(&apk, &out, opts.all_src)?;
    }

    // Assets / libs / unknown.
    let report = library::extract(&apk, &out)?;

    // apktool.yml.
    let yml = build_yml(apk_path, &apk, &manifest_info, app_table.as_ref(), &report);
    yml.save(&out)?;

    tracing::info!("decoded into {}", out.display());
    Ok(out)
}

fn build_yml(
    apk_path: &Path,
    _apk: &Apk,
    info: &manifest::ManifestInfo,
    table: Option<&arsc::ResTable>,
    report: &library::ExtractReport,
) -> ApktoolYml {
    let package_id = table
        .and_then(|t| t.packages.first())
        .map(|p| format!("{}", p.id))
        .unwrap_or_default();

    ApktoolYml {
        version: "2.0.0".to_string(),
        apk_file_name: apk_path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string(),
        is_frameworks_only: false,
        use_aapt2: true,
        sdk_info: SdkInfo {
            min_sdk_version: info.min_sdk.clone(),
            target_sdk_version: info.target_sdk.clone(),
            max_sdk_version: String::new(),
        },
        package_info: ResourcesInfo {
            package_id,
            rename_manifest_package: None,
        },
        version_info: VersionInfo {
            version_code: info.version_code.clone(),
            version_name: info.version_name.clone(),
        },
        resources_are_compressed: false,
        compact_entries: false,
        shared_library: false,
        sparse_resources: false,
        uses_framework: Default::default(),
        do_not_compress: report.do_not_compress.clone(),
        unknown_files: report.unknown_files.clone(),
    }
}
