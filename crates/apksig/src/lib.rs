//! `apksig` — APK signing (v1 JAR, v2/v3 APK Signing Block, v4 `.apk.idsig`).
//!
//! Upstream apktool defers signing to Google's `apksigner`; shipping native
//! signing is a Lancer differentiator. v1 (digest manifests + JAR signature
//! files) is implemented here; the v2/v3 signing-block framing and v4 Merkle
//! tree are scaffolded with their zip-section plumbing.

pub mod v1;
pub mod v2;
pub mod v3;
pub mod v4;
pub mod zip_sections;

use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("not a zip/APK (no End Of Central Directory record)")]
    NotZip,
    #[error("keystore error: {0}")]
    Keystore(String),
    #[error("signing not yet implemented: {0}")]
    Unimplemented(&'static str),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

/// APK signature schemes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scheme {
    /// JAR signing (`META-INF/MANIFEST.MF` + `*.SF` + `*.RSA`).
    V1,
    /// APK Signature Scheme v2 (signing block before the central directory).
    V2,
    /// v3 (adds key rotation).
    V3,
    /// v4 (sidecar `.apk.idsig` with an fs-verity Merkle tree).
    V4,
}

/// Signing configuration.
#[derive(Debug, Clone)]
pub struct SignOptions {
    pub schemes: Vec<Scheme>,
    pub keystore: Option<PathBuf>,
    pub alias: Option<String>,
    pub storepass: Option<String>,
    pub keypass: Option<String>,
    /// Use the bundled debug keystore (default).
    pub use_debug: bool,
}

impl Default for SignOptions {
    fn default() -> Self {
        SignOptions {
            schemes: vec![Scheme::V1, Scheme::V2, Scheme::V3],
            keystore: None,
            alias: None,
            storepass: None,
            keypass: None,
            use_debug: true,
        }
    }
}

/// Sign an in-memory APK, returning the signed bytes.
pub fn sign(apk_bytes: &[u8], opts: &SignOptions) -> Result<Vec<u8>, Error> {
    let mut out = apk_bytes.to_vec();
    if opts.schemes.contains(&Scheme::V1) {
        out = v1::sign(&out, opts)?;
    }
    if opts.schemes.contains(&Scheme::V2) {
        out = v2::sign(&out, opts)?;
    }
    if opts.schemes.contains(&Scheme::V3) {
        out = v3::sign(&out, opts)?;
    }
    Ok(out)
}

/// Report a (best-effort) summary of which schemes are present in an APK.
#[derive(Debug, Default, Clone)]
pub struct VerifyReport {
    pub v1: bool,
    pub v2: bool,
    pub v3: bool,
}

pub fn detect_schemes(apk_bytes: &[u8]) -> Result<VerifyReport, Error> {
    let mut report = VerifyReport::default();
    // v1: presence of a *.RSA/*.DSA/*.EC under META-INF.
    if let Ok(mut zip) = zip::ZipArchive::new(std::io::Cursor::new(apk_bytes)) {
        for i in 0..zip.len() {
            if let Ok(f) = zip.by_index(i) {
                let n = f.name().to_uppercase();
                if n.starts_with("META-INF/")
                    && (n.ends_with(".RSA") || n.ends_with(".DSA") || n.ends_with(".EC"))
                {
                    report.v1 = true;
                }
            }
        }
    }
    // v2/v3: presence of the APK Signing Block.
    if let Ok(sections) = zip_sections::ZipSections::parse(apk_bytes) {
        if let Some(block) = sections.find_signing_block(apk_bytes) {
            report.v2 = block.has_id(v2::APK_SIGNATURE_SCHEME_V2_BLOCK_ID, apk_bytes);
            report.v3 = block.has_id(v3::APK_SIGNATURE_SCHEME_V3_BLOCK_ID, apk_bytes);
        }
    }
    Ok(report)
}
