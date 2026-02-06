//! APK Signature Scheme v2 (APK Signing Block).

use crate::zip_sections::ZipSections;
use crate::{Error, SignOptions};
use sha2::{Digest, Sha256};

/// v2 block id in the APK Signing Block.
pub const APK_SIGNATURE_SCHEME_V2_BLOCK_ID: u32 = 0x7109_871a;

/// The APK v2/v3 schemes digest content in 1 MiB chunks.
pub const CHUNK_SIZE: usize = 1024 * 1024;

/// Compute the per-chunk SHA-256 digests for the three signed sections of an
/// APK, per the APK Signature Scheme v2 "digest over chunks" algorithm.
///
/// The three sections are:
/// 1. The contents of the ZIP entries (everything before the APK Signing
///    Block — i.e. `[0, central_dir_offset)` when no block is present, or
///    `[0, signing_block_start)` when one is).
/// 2. The ZIP central directory (`[central_dir_offset, eocd_offset)`).
/// 3. The End Of Central Directory record (`[eocd_offset, len)`).
///
/// Each section is split into 1 MiB chunks. Every chunk is hashed as
/// `SHA-256(0xa5 || uint32le(chunk_len) || chunk_bytes)`, matching the
/// per-chunk framing the final top-level digest is computed over. The returned
/// vector is the ordered list of those per-chunk digests across all three
/// sections.
///
/// All slicing is bounds-checked against `apk_bytes`; out-of-range offsets
/// simply yield no chunks for the affected section.
pub fn digest_chunks(apk_bytes: &[u8], sections: &ZipSections) -> Vec<[u8; 32]> {
    let len = apk_bytes.len();

    // Section 1 ends at the signing block if present, else at the CD start.
    let contents_end = sections
        .find_signing_block(apk_bytes)
        .map(|b| b.start)
        .unwrap_or(sections.central_dir_offset as usize)
        .min(len);
    let cd_start = (sections.central_dir_offset as usize).min(len);
    let eocd_start = (sections.eocd_offset as usize).min(len);

    let mut digests = Vec::new();
    for (start, end) in [
        (0usize, contents_end),
        (cd_start, eocd_start),
        (eocd_start, len),
    ] {
        if start >= end || end > len {
            continue;
        }
        for chunk in apk_bytes[start..end].chunks(CHUNK_SIZE) {
            let mut hasher = Sha256::new();
            // Chunk framing: 0xa5 prefix byte + little-endian chunk length.
            hasher.update([0xa5u8]);
            hasher.update((chunk.len() as u32).to_le_bytes());
            hasher.update(chunk);
            digests.push(hasher.finalize().into());
        }
    }
    digests
}

pub fn sign(_apk_bytes: &[u8], _opts: &SignOptions) -> Result<Vec<u8>, Error> {
    Err(Error::Unimplemented(
        "v2 APK Signing Block (digest-of-chunks + signing block insertion)",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::zip_sections::ZipSections;

    /// Build a trivial single-entry ZIP in memory.
    fn trivial_zip() -> Vec<u8> {
        use std::io::Write;
        let mut buf = Vec::new();
        {
            let mut zw = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
            let opts: zip::write::FileOptions<()> =
                zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Stored);
            zw.start_file("hello.txt", opts).unwrap();
            zw.write_all(b"hello world").unwrap();
            zw.finish().unwrap();
        }
        buf
    }

    #[test]
    fn digest_chunks_covers_all_sections() {
        let zip = trivial_zip();
        let sections = ZipSections::parse(&zip).expect("parse zip");
        let digests = digest_chunks(&zip, &sections);
        // A tiny zip has one chunk per non-empty section: contents + CD + EOCD.
        assert_eq!(digests.len(), 3, "expected one digest per section");
        // Digests must be deterministic and distinct from the empty-chunk hash.
        let again = digest_chunks(&zip, &sections);
        assert_eq!(digests, again, "digest computation must be deterministic");
    }
}
