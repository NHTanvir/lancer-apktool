//! APK Signature Scheme v3 (adds key rotation).

use crate::zip_sections::ZipSections;
use crate::{Error, SignOptions};

/// v3 block id in the APK Signing Block.
pub const APK_SIGNATURE_SCHEME_V3_BLOCK_ID: u32 = 0xf053_68c0;

/// Compute the per-chunk SHA-256 digests for the three signed APK sections.
///
/// v3 reuses the v2 "digest over chunks" algorithm verbatim (1 MiB chunks of
/// the entry contents, central directory, and EOCD); see
/// [`crate::v2::digest_chunks`] for the framing details. The v3 differences
/// (key rotation, the proof-of-rotation struct) live in the signing-block
/// payload, not the content digest.
pub fn digest_chunks(apk_bytes: &[u8], sections: &ZipSections) -> Vec<[u8; 32]> {
    crate::v2::digest_chunks(apk_bytes, sections)
}

pub fn sign(_apk_bytes: &[u8], _opts: &SignOptions) -> Result<Vec<u8>, Error> {
    Err(Error::Unimplemented(
        "v3 APK Signing Block (rotation proof-of-rotation struct)",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn v3_digest_matches_v2() {
        use std::io::Write;
        let mut buf = Vec::new();
        {
            let mut zw = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
            let opts: zip::write::FileOptions<()> =
                zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Stored);
            zw.start_file("a.txt", opts).unwrap();
            zw.write_all(b"abc").unwrap();
            zw.finish().unwrap();
        }
        let sections = ZipSections::parse(&buf).expect("parse zip");
        assert_eq!(
            digest_chunks(&buf, &sections),
            crate::v2::digest_chunks(&buf, &sections)
        );
    }
}
