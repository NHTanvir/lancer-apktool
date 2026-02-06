//! APK Signature Scheme v4: sidecar `.apk.idsig` with an fs-verity Merkle tree.

use crate::Error;
use sha2::{Digest, Sha256};

/// fs-verity uses 4 KiB pages.
pub const MERKLE_BLOCK_SIZE: usize = 4096;

/// Compute the root hash of a SHA-256 Merkle tree over `data` using
/// fs-verity's 4 KiB block size. (The `.idsig` framing is layered on top.)
pub fn merkle_root(data: &[u8]) -> [u8; 32] {
    if data.is_empty() {
        return Sha256::digest(b"").into();
    }
    let mut level: Vec<[u8; 32]> = data
        .chunks(MERKLE_BLOCK_SIZE)
        .map(|c| {
            let mut block = c.to_vec();
            block.resize(MERKLE_BLOCK_SIZE, 0);
            Sha256::digest(&block).into()
        })
        .collect();

    while level.len() > 1 {
        // Pack hashes into 4 KiB blocks, then hash each block.
        let mut packed = Vec::new();
        for h in &level {
            packed.extend_from_slice(h);
        }
        level = packed
            .chunks(MERKLE_BLOCK_SIZE)
            .map(|c| {
                let mut block = c.to_vec();
                block.resize(MERKLE_BLOCK_SIZE, 0);
                Sha256::digest(&block).into()
            })
            .collect();
    }
    level[0]
}

/// Produce the `.apk.idsig` bytes for an APK. (Signature framing pending.)
pub fn build_idsig(_apk_bytes: &[u8]) -> Result<Vec<u8>, Error> {
    Err(Error::Unimplemented(
        "v4 .apk.idsig framing (Merkle root is available via merkle_root)",
    ))
}
