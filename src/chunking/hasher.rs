// Hash computation for chunks

/// Hasher for computing chunk checksums
pub struct ChunkHasher;

impl ChunkHasher {
    /// Compute BLAKE3 hash of data
    pub fn hash(data: &[u8]) -> Vec<u8> {
        blake3::hash(data).as_bytes().to_vec()
    }

    /// Verify data against a checksum
    pub fn verify(data: &[u8], checksum: &[u8]) -> bool {
        let computed = blake3::hash(data);
        computed.as_bytes() == checksum
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_deterministic() {
        let data = b"test data";
        let hash1 = ChunkHasher::hash(data);
        let hash2 = ChunkHasher::hash(data);
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_verify_success() {
        let data = b"test data";
        let hash = ChunkHasher::hash(data);
        assert!(ChunkHasher::verify(data, &hash));
    }

    #[test]
    fn test_verify_failure() {
        let data = b"test data";
        let wrong_hash = vec![0u8; 32];
        assert!(!ChunkHasher::verify(data, &wrong_hash));
    }
}
