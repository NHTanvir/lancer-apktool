//! AndroidManifest.xml (AXML) → text XML.

use crate::resolver::TableResolver;
use anyhow::{Context, Result};

/// Decode binary `AndroidManifest.xml` bytes to readable text XML.
pub fn decode(bytes: &[u8], resolver: &TableResolver) -> Result<String> {
    axml::decode_with(bytes, resolver).context("decoding AndroidManifest.xml")
}

/// Read `package`, `versionCode`, `versionName`, and sdk versions from a decoded
/// manifest model for `apktool.yml`.
#[derive(Debug, Default, Clone)]
pub struct ManifestInfo {
    pub package: String,
    pub version_code: String,
    pub version_name: String,
    pub min_sdk: String,
    pub target_sdk: String,
    pub compile_sdk: String,
}

pub fn info(bytes: &[u8]) -> Result<ManifestInfo> {
    let doc = axml::parse(bytes).context("parsing manifest")?;
    let mut info = ManifestInfo::default();
    for ev in &doc.events {
        if let axml::XmlEvent::StartElement { name, attributes, .. } = ev {
            match name.as_str() {
                "manifest" => {
                    for a in attributes {
                        match a.name.as_str() {
                            "package" => info.package = attr_text(a),
                            "versionCode" => info.version_code = attr_text(a),
                            "versionName" => info.version_name = attr_text(a),
                            "compileSdkVersion" => info.compile_sdk = attr_text(a),
                            _ => {}
                        }
                    }
                }
                "uses-sdk" => {
                    for a in attributes {
                        match a.name.as_str() {
                            "minSdkVersion" => info.min_sdk = attr_text(a),
                            "targetSdkVersion" => info.target_sdk = attr_text(a),
                            _ => {}
                        }
                    }
                }
                _ => {}
            }
        }
    }
    Ok(info)
}

fn attr_text(a: &axml::Attribute) -> String {
    if let Some(raw) = &a.raw_value {
        return raw.clone();
    }
    format!("{}", a.typed.data)
}
