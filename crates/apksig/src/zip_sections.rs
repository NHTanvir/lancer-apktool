//! Zip structural parsing needed for APK Signing Block insertion.

use crate::Error;
use byteorder::{ByteOrder, LittleEndian as LE};

const EOCD_SIGNATURE: u32 = 0x0605_4b50;
const APK_SIG_BLOCK_MAGIC: &[u8; 16] = b"APK Sig Block 42";

/// The three offsets that the APK Signing Block lives between.
#[derive(Debug, Clone, Copy)]
pub struct ZipSections {
    /// Offset of the start of the ZIP central directory.
    pub central_dir_offset: u64,
    /// Size of the central directory.
    pub central_dir_size: u64,
    /// Offset of the End Of Central Directory record.
    pub eocd_offset: u64,
}

impl ZipSections {
    /// Locate the EOCD and read the central-directory offsets.
    pub fn parse(bytes: &[u8]) -> Result<ZipSections, Error> {
        let eocd_offset = find_eocd(bytes).ok_or(Error::NotZip)?;
        if eocd_offset + 20 > bytes.len() {
            return Err(Error::NotZip);
        }
        let central_dir_size = LE::read_u32(&bytes[eocd_offset + 12..eocd_offset + 16]) as u64;
        let central_dir_offset = LE::read_u32(&bytes[eocd_offset + 16..eocd_offset + 20]) as u64;
        Ok(ZipSections {
            central_dir_offset,
            central_dir_size,
            eocd_offset: eocd_offset as u64,
        })
    }

    /// Find and parse an existing APK Signing Block, if present.
    pub fn find_signing_block(&self, bytes: &[u8]) -> Option<SigningBlock> {
        let cd = self.central_dir_offset as usize;
        if cd < 24 {
            return None;
        }
        // The block's trailing 16 bytes are the magic; before it is the size.
        let magic_at = cd - 16;
        if &bytes[magic_at..cd] != APK_SIG_BLOCK_MAGIC {
            return None;
        }
        let size_at_end = cd - 24;
        let block_size = LE::read_u64(&bytes[size_at_end..size_at_end + 8]) as usize;
        if block_size + 8 > cd {
            return None;
        }
        let block_start = cd - (block_size + 8);
        Some(SigningBlock {
            start: block_start,
            end: cd,
        })
    }
}

/// A located APK Signing Block (offsets into the original APK bytes).
#[derive(Debug, Clone, Copy)]
pub struct SigningBlock {
    pub start: usize,
    pub end: usize,
}

impl SigningBlock {
    /// Enumerate every id-value pair id in the block.
    ///
    /// The APK Signing Block body is framed as:
    /// `[uint64 size]` then a sequence of `[uint64 pair_len][uint32 id][value]`
    /// pairs, then a trailing `[uint64 size]` and the 16-byte magic
    /// `"APK Sig Block 42"`. `pair_len` covers the 4-byte id plus its value.
    ///
    /// `self.start` points at the leading size field and `self.end` points just
    /// past the magic, so the id-value pairs live between `self.start + 8`
    /// (after the leading size) and `self.end - 24` (before the trailing size +
    /// magic). All reads are bounds-checked; a malformed block yields the ids
    /// collected so far.
    pub fn ids(&self, apk_bytes: &[u8]) -> Vec<u32> {
        let mut ids = Vec::new();
        // The pair region is [start + 8, end - 24).
        let pairs_start = self.start.saturating_add(8);
        let pairs_end = self.end.saturating_sub(24);
        if pairs_end > apk_bytes.len() || pairs_start > pairs_end {
            return ids;
        }

        let mut pos = pairs_start;
        while pos + 8 <= pairs_end {
            let pair_len = LE::read_u64(&apk_bytes[pos..pos + 8]) as usize;
            pos += 8;
            // The pair length covers the 4-byte id and its value; it must fit
            // in the remaining region and be large enough to hold the id.
            if pair_len < 4 || pos + pair_len > pairs_end {
                break;
            }
            let id = LE::read_u32(&apk_bytes[pos..pos + 4]);
            ids.push(id);
            pos += pair_len;
        }
        ids
    }

    /// True if the block contains an id-value pair with the given block id.
    pub fn has_id(&self, id: u32, apk_bytes: &[u8]) -> bool {
        self.ids(apk_bytes).contains(&id)
    }
}

fn find_eocd(bytes: &[u8]) -> Option<usize> {
    if bytes.len() < 22 {
        return None;
    }
    // Scan backwards over the (max 64KiB) comment field for the EOCD signature.
    let min = bytes.len().saturating_sub(22 + 0xffff);
    let mut i = bytes.len() - 22;
    loop {
        if LE::read_u32(&bytes[i..i + 4]) == EOCD_SIGNATURE {
            return Some(i);
        }
        if i == min {
            return None;
        }
        i -= 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// Build a trivial in-memory ZIP with a single stored entry.
    fn trivial_zip() -> Vec<u8> {
        let mut buf = Vec::new();
        {
            let mut zw = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
            let opts: zip::write::FileOptions<()> = zip::write::FileOptions::default()
                .compression_method(zip::CompressionMethod::Stored);
            zw.start_file("entry.bin", opts).unwrap();
            zw.write_all(b"payload bytes").unwrap();
            zw.finish().unwrap();
        }
        buf
    }

    #[test]
    fn parse_finds_eocd_and_central_dir() {
        let zip = trivial_zip();
        let sections = ZipSections::parse(&zip).expect("EOCD must be located");

        // The EOCD signature must really live at the reported offset.
        let eocd = sections.eocd_offset as usize;
        assert_eq!(
            LE::read_u32(&zip[eocd..eocd + 4]),
            EOCD_SIGNATURE,
            "eocd_offset must point at the EOCD signature"
        );

        // The central directory offset must be before the EOCD and its first
        // record must carry the central-directory-header signature (PK\x01\x02).
        let cd = sections.central_dir_offset as usize;
        assert!(cd < eocd, "central dir must precede the EOCD");
        assert_eq!(
            LE::read_u32(&zip[cd..cd + 4]),
            0x0201_4b50,
            "central_dir_offset must point at a central directory header"
        );
        assert!(sections.central_dir_size > 0, "central dir must be non-empty");
    }

    #[test]
    fn parse_rejects_non_zip() {
        let garbage = vec![0u8; 64];
        assert!(matches!(
            ZipSections::parse(&garbage),
            Err(crate::Error::NotZip)
        ));
    }
}
