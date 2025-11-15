// Chunk hash validation

use crate::common::error::{Error, Result};

/// Validate a BLAKE3 hash
pub fn validate_hash_size(hash: &[u8]) -> Result<()> {
    if hash.len() != 32 {
        return Err(Error::Protocol(format!(
            "Invalid hash size: {} bytes (expected 32 for BLAKE3)",
            hash.len()
        )));
    }
    Ok(())
}

/// Validate multiple hashes
pub fn validate_hash_list(hashes: &[Vec<u8>]) -> Result<()> {
    for (idx, hash) in hashes.iter().enumerate() {
        if hash.len() != 32 {
            return Err(Error::Protocol(format!(
                "Hash {} has invalid size: {} bytes (expected 32)",
                idx,
                hash.len()
            )));
        }
    }
    Ok(())
}

/// Verify data matches hash
pub fn verify_data_hash(data: &[u8], expected_hash: &[u8]) -> Result<()> {
    validate_hash_size(expected_hash)?;
    
    let computed = blake3::hash(data);
    if computed.as_bytes() != expected_hash {
        return Err(Error::HashMismatch {
            expected: expected_hash.to_vec(),
            actual: computed.as_bytes().to_vec(),
        });
    }
    
    Ok(())
}

/// Compute BLAKE3 hash of data
pub fn compute_hash(data: &[u8]) -> Vec<u8> {
    blake3::hash(data).as_bytes().to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_hash_size() {
        let valid_hash = vec![0u8; 32];
        assert!(validate_hash_size(&valid_hash).is_ok());

        let invalid_hash = vec![0u8; 16];
        assert!(validate_hash_size(&invalid_hash).is_err());
    }

    #[test]
    fn test_validate_hash_list() {
        let valid_hashes = vec![vec![0u8; 32], vec![1u8; 32], vec![2u8; 32]];
        assert!(validate_hash_list(&valid_hashes).is_ok());

        let invalid_hashes = vec![vec![0u8; 32], vec![1u8; 16]];
        assert!(validate_hash_list(&invalid_hashes).is_err());
    }

    #[test]
    fn test_verify_data_hash() {
        let data = b"test data";
        let hash = blake3::hash(data);
        
        assert!(verify_data_hash(data, hash.as_bytes()).is_ok());
        
        let wrong_hash = blake3::hash(b"wrong data");
        assert!(verify_data_hash(data, wrong_hash.as_bytes()).is_err());
    }

    #[test]
    fn test_compute_hash() {
        let data = b"test";
        let hash1 = compute_hash(data);
        let hash2 = compute_hash(data);
        
        assert_eq!(hash1, hash2);
        assert_eq!(hash1.len(), 32);
        
        let expected = blake3::hash(data);
        assert_eq!(hash1, expected.as_bytes());
    }
}
