//! v1 (JAR) signing: digest manifests + signature file.
//!
//! Builds `META-INF/MANIFEST.MF` (a SHA-256 digest per zip entry) and
//! `META-INF/CERT.SF` (a digest over the manifest). The PKCS#7 `CERT.RSA`
//! block requires an RSA private key + X.509 chain and is layered in next.

use crate::{Error, SignOptions};
use sha2::{Digest, Sha256};
use std::io::Read;

/// Build the `MANIFEST.MF` text for the entries in `apk_bytes`.
pub fn build_manifest(apk_bytes: &[u8]) -> Result<String, Error> {
    let mut manifest = String::from("Manifest-Version: 1.0\r\nCreated-By: lancer-apktool\r\n\r\n");
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(apk_bytes)).map_err(|_| Error::NotZip)?;
    let mut names: Vec<String> = (0..zip.len())
        .filter_map(|i| zip.by_index(i).ok().map(|f| f.name().to_string()))
        .filter(|n| !n.starts_with("META-INF/") && !n.ends_with('/'))
        .collect();
    names.sort();

    for name in names {
        let mut file = zip.by_name(&name).map_err(|_| Error::NotZip)?;
        let mut data = Vec::new();
        file.read_to_end(&mut data)?;
        let digest = Sha256::digest(&data);
        manifest.push_str(&format!(
            "Name: {}\r\nSHA-256-Digest: {}\r\n\r\n",
            name,
            base64_encode(&digest)
        ));
    }
    Ok(manifest)
}

/// Build the `CERT.SF` text from a manifest.
pub fn build_signature_file(manifest: &str) -> String {
    let digest = Sha256::digest(manifest.as_bytes());
    format!(
        "Signature-Version: 1.0\r\nCreated-By: lancer-apktool\r\nSHA-256-Digest-Manifest: {}\r\n\r\n",
        base64_encode(&digest)
    )
}

/// Apply v1 signing to `apk_bytes`.
pub fn sign(apk_bytes: &[u8], _opts: &SignOptions) -> Result<Vec<u8>, Error> {
    // Real digest + manifest computation (exercised); the RSA block is pending.
    let manifest = build_manifest(apk_bytes)?;
    let _sf = build_signature_file(&manifest);
    Err(Error::Unimplemented(
        "v1 PKCS#7 CERT.RSA block (needs RSA key + X.509 chain)",
    ))
}

/// Minimal standard base64 encoder (no padding-free, no line wraps).
pub fn base64_encode(data: &[u8]) -> String {
    const ALPHABET: &[u8; 64] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity((data.len() + 2) / 3 * 4);
    for chunk in data.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
        out.push(ALPHABET[((n >> 18) & 0x3f) as usize] as char);
        out.push(ALPHABET[((n >> 12) & 0x3f) as usize] as char);
        if chunk.len() > 1 {
            out.push(ALPHABET[((n >> 6) & 0x3f) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(ALPHABET[(n & 0x3f) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}
