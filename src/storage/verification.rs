// File hash verification

use crate::common::error::{Error, Result};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

/// Verify the hash of a complete file
/// 
/// # Arguments
/// * `file_path` - Path to the file to verify
/// * `expected_hash` - Expected BLAKE3 hash (32 bytes)
/// 
/// # Returns
/// * `Ok(())` - If hash matches
/// * `Err(Error)` - If hash doesn't match or file can't be read
pub fn verify_file_hash(file_path: &Path, expected_hash: &[u8]) -> Result<()> {
    if expected_hash.len() != 32 {
        return Err(Error::Protocol(format!(
            "Invalid hash size: {} (expected 32 bytes for BLAKE3)",
            expected_hash.len()
        )));
    }

    let mut file = File::open(file_path)?;
    let computed_hash = compute_file_hash(&mut file)?;

    if computed_hash.as_bytes() != expected_hash {
        return Err(Error::HashMismatch {
            expected: expected_hash.to_vec(),
            actual: computed_hash.as_bytes().to_vec(),
        });
    }

    Ok(())
}

/// Compute the BLAKE3 hash of a file
/// 
/// # Arguments
/// * `file` - File handle to hash
/// 
/// # Returns
/// * `Ok(Hash)` - The computed hash
/// * `Err(Error)` - If file can't be read
pub fn compute_file_hash(file: &mut File) -> Result<blake3::Hash> {
    file.seek(SeekFrom::Start(0))?;
    
    let mut hasher = blake3::Hasher::new();
    let mut buffer = vec![0u8; 65536]; // 64KB buffer
    
    loop {
        let bytes_read = file.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }
    
    Ok(hasher.finalize())
}

/// Verify file hash matches expected hash from bytes
pub fn verify_file_hash_bytes(file: &mut File, expected_hash: &[u8]) -> Result<()> {
    if expected_hash.len() != 32 {
        return Err(Error::Protocol(format!(
            "Invalid hash size: {} (expected 32)",
            expected_hash.len()
        )));
    }

    let computed = compute_file_hash(file)?;
    
    if computed.as_bytes() != expected_hash {
        return Err(Error::HashMismatch {
            expected: expected_hash.to_vec(),
            actual: computed.as_bytes().to_vec(),
        });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_compute_file_hash() {
        let mut temp_file = NamedTempFile::new().unwrap();
        let test_data = b"Hello, World!";
        temp_file.write_all(test_data).unwrap();
        temp_file.flush().unwrap();

        let mut file = File::open(temp_file.path()).unwrap();
        let hash = compute_file_hash(&mut file).unwrap();
        
        // Verify deterministic
        file.seek(SeekFrom::Start(0)).unwrap();
        let hash2 = compute_file_hash(&mut file).unwrap();
        assert_eq!(hash, hash2);
        
        // Verify against known BLAKE3 hash
        let expected = blake3::hash(test_data);
        assert_eq!(hash, expected);
    }

    #[test]
    fn test_verify_file_hash_success() {
        let mut temp_file = NamedTempFile::new().unwrap();
        let test_data = b"Test data for hash verification";
        temp_file.write_all(test_data).unwrap();
        temp_file.flush().unwrap();

        let expected_hash = blake3::hash(test_data);
        let result = verify_file_hash(temp_file.path(), expected_hash.as_bytes());
        
        assert!(result.is_ok());
    }

    #[test]
    fn test_verify_file_hash_mismatch() {
        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(b"Some data").unwrap();
        temp_file.flush().unwrap();

        let wrong_hash = blake3::hash(b"Different data");
        let result = verify_file_hash(temp_file.path(), wrong_hash.as_bytes());
        
        assert!(result.is_err());
        match result {
            Err(Error::HashMismatch { .. }) => (),
            _ => panic!("Expected HashMismatch error"),
        }
    }

    #[test]
    fn test_verify_file_hash_invalid_size() {
        let temp_file = NamedTempFile::new().unwrap();
        let invalid_hash = vec![0u8; 16]; // Wrong size
        
        let result = verify_file_hash(temp_file.path(), &invalid_hash);
        assert!(result.is_err());
    }

    #[test]
    fn test_verify_file_hash_bytes() {
        let mut temp_file = NamedTempFile::new().unwrap();
        let test_data = b"Byte verification test";
        temp_file.write_all(test_data).unwrap();
        temp_file.flush().unwrap();

        let mut file = File::open(temp_file.path()).unwrap();
        let expected = blake3::hash(test_data);
        
        let result = verify_file_hash_bytes(&mut file, expected.as_bytes());
        assert!(result.is_ok());
    }
}
