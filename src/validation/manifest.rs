// Manifest validation

use crate::common::error::{Error, Result};
use crate::protocol::messages::Manifest;

/// BLAKE3 hash size in bytes
const BLAKE3_HASH_SIZE: usize = 32;

/// SHA256 hash size in bytes (for future compatibility)
const SHA256_HASH_SIZE: usize = 32;

/// Maximum reasonable chunk size (100 MB)
const MAX_CHUNK_SIZE: u32 = 100 * 1024 * 1024;

/// Minimum chunk size (1 KB)
const MIN_CHUNK_SIZE: u32 = 1024;

/// Maximum file size we'll validate (1 TB)
const MAX_FILE_SIZE: u64 = 1024 * 1024 * 1024 * 1024;

/// Validation errors
#[derive(Debug, Clone)]
pub enum ValidationError {
    InvalidChunkCount,
    InvalidChunkSize,
    InvalidFileSize,
    InvalidHashSize,
    MismatchedChunkHashes,
    MismatchedFileHash,
    InvalidSessionId,
    InvalidFileName,
    InvalidCompression,
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ValidationError::InvalidChunkCount => write!(f, "Invalid chunk count"),
            ValidationError::InvalidChunkSize => write!(f, "Invalid chunk size"),
            ValidationError::InvalidFileSize => write!(f, "Invalid file size"),
            ValidationError::InvalidHashSize => write!(f, "Invalid hash size"),
            ValidationError::MismatchedChunkHashes => write!(f, "Chunk hash count mismatch"),
            ValidationError::MismatchedFileHash => write!(f, "File hash mismatch"),
            ValidationError::InvalidSessionId => write!(f, "Invalid session ID"),
            ValidationError::InvalidFileName => write!(f, "Invalid file name"),
            ValidationError::InvalidCompression => write!(f, "Invalid compression algorithm"),
        }
    }
}

impl std::error::Error for ValidationError {}

/// Manifest validator for ensuring manifest integrity
pub struct ManifestValidator {
    strict_mode: bool,
}

impl ManifestValidator {
    /// Create a new validator with default settings (non-strict)
    pub fn new() -> Self {
        Self {
            strict_mode: false,
        }
    }

    /// Create a validator in strict mode
    /// Strict mode enforces additional checks like compression validation
    pub fn strict() -> Self {
        Self {
            strict_mode: true,
        }
    }

    /// Validate a manifest comprehensively
    /// 
    /// # Arguments
    /// * `manifest` - The manifest to validate
    /// 
    /// # Returns
    /// * `Ok(())` - If manifest is valid
    /// * `Err(Error)` - If validation fails
    pub fn validate(&self, manifest: &Manifest) -> Result<()> {
        self.validate_session_id(&manifest.session_id)?;
        self.validate_file_name(&manifest.file_name)?;
        self.validate_file_size(manifest.file_size)?;
        self.validate_chunk_size(manifest.chunk_size)?;
        self.validate_chunk_count(manifest.file_size, manifest.chunk_size, manifest.total_chunks)?;
        self.validate_file_hash(&manifest.file_hash)?;
        self.validate_chunk_hashes(&manifest.chunk_hashes, manifest.total_chunks)?;
        
        if self.strict_mode {
            self.validate_compression(&manifest.compression)?;
            self.validate_original_size(manifest.file_size, manifest.original_size)?;
        }

        Ok(())
    }

    /// Validate session ID format
    pub fn validate_session_id(&self, session_id: &str) -> Result<()> {
        if session_id.is_empty() {
            return Err(Error::Protocol(
                "Session ID cannot be empty".to_string()
            ));
        }

        if session_id.len() < 8 || session_id.len() > 128 {
            return Err(Error::Protocol(format!(
                "Session ID length invalid: {} (expected 8-128 chars)",
                session_id.len()
            )));
        }

        // Check for valid characters (alphanumeric, hyphens, underscores)
        if !session_id.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_') {
            return Err(Error::Protocol(
                "Session ID contains invalid characters".to_string()
            ));
        }

        Ok(())
    }

    /// Validate file name
    pub fn validate_file_name(&self, file_name: &str) -> Result<()> {
        if file_name.is_empty() {
            return Err(Error::Protocol(
                "File name cannot be empty".to_string()
            ));
        }

        if file_name.len() > 255 {
            return Err(Error::Protocol(format!(
                "File name too long: {} chars (max: 255)",
                file_name.len()
            )));
        }

        // Check for path traversal attempts
        if file_name.contains("..") || file_name.contains("//") {
            return Err(Error::Protocol(
                "File name contains path traversal sequences".to_string()
            ));
        }

        Ok(())
    }

    /// Validate file size
    pub fn validate_file_size(&self, file_size: u64) -> Result<()> {
        if file_size == 0 {
            return Err(Error::Protocol(
                "File size cannot be zero".to_string()
            ));
        }

        if file_size > MAX_FILE_SIZE {
            return Err(Error::Protocol(format!(
                "File size too large: {} bytes (max: {})",
                file_size, MAX_FILE_SIZE
            )));
        }

        Ok(())
    }

    /// Validate chunk size
    pub fn validate_chunk_size(&self, chunk_size: u32) -> Result<()> {
        if chunk_size < MIN_CHUNK_SIZE {
            return Err(Error::Protocol(format!(
                "Chunk size too small: {} bytes (min: {})",
                chunk_size, MIN_CHUNK_SIZE
            )));
        }

        if chunk_size > MAX_CHUNK_SIZE {
            return Err(Error::Protocol(format!(
                "Chunk size too large: {} bytes (max: {})",
                chunk_size, MAX_CHUNK_SIZE
            )));
        }

        // Check if chunk size is a power of 2 (recommended but not required)
        // Just log a warning in strict mode
        if self.strict_mode && !chunk_size.is_power_of_two() {
            // Not an error, but could log a warning
        }

        Ok(())
    }

    /// Validate chunk count matches file size and chunk size
    pub fn validate_chunk_count(
        &self,
        file_size: u64,
        chunk_size: u32,
        total_chunks: u64,
    ) -> Result<()> {
        if total_chunks == 0 {
            return Err(Error::Protocol(
                "Total chunks cannot be zero".to_string()
            ));
        }

        // Calculate expected chunk count
        let expected_chunks = (file_size + chunk_size as u64 - 1) / chunk_size as u64;

        if total_chunks != expected_chunks {
            return Err(Error::Protocol(format!(
                "Chunk count mismatch: got {}, expected {} (file_size={}, chunk_size={})",
                total_chunks, expected_chunks, file_size, chunk_size
            )));
        }

        Ok(())
    }

    /// Validate file hash
    pub fn validate_file_hash(&self, file_hash: &[u8]) -> Result<()> {
        if file_hash.is_empty() {
            return Err(Error::Protocol(
                "File hash cannot be empty".to_string()
            ));
        }

        // Support BLAKE3 (32 bytes) and SHA256 (32 bytes)
        if file_hash.len() != BLAKE3_HASH_SIZE && file_hash.len() != SHA256_HASH_SIZE {
            return Err(Error::Protocol(format!(
                "File hash size invalid: {} bytes (expected {} for BLAKE3/SHA256)",
                file_hash.len(),
                BLAKE3_HASH_SIZE
            )));
        }

        Ok(())
    }

    /// Validate chunk hashes
    pub fn validate_chunk_hashes(
        &self,
        chunk_hashes: &[Vec<u8>],
        total_chunks: u64,
    ) -> Result<()> {
        // Verify count matches
        if chunk_hashes.len() as u64 != total_chunks {
            return Err(Error::Protocol(format!(
                "Chunk hash count mismatch: got {}, expected {}",
                chunk_hashes.len(),
                total_chunks
            )));
        }

        // Verify each hash is the correct size
        for (idx, hash) in chunk_hashes.iter().enumerate() {
            if hash.len() != BLAKE3_HASH_SIZE {
                return Err(Error::Protocol(format!(
                    "Chunk hash {} has invalid size: {} bytes (expected {})",
                    idx,
                    hash.len(),
                    BLAKE3_HASH_SIZE
                )));
            }
        }

        Ok(())
    }

    /// Validate compression algorithm
    pub fn validate_compression(&self, compression: &str) -> Result<()> {
        const VALID_COMPRESSION: &[&str] = &["none", "lz4", "lz4hc", "zstd", "lzma2"];

        if !VALID_COMPRESSION.contains(&compression) {
            return Err(Error::Protocol(format!(
                "Invalid compression algorithm: '{}' (valid: {:?})",
                compression, VALID_COMPRESSION
            )));
        }

        Ok(())
    }

    /// Validate original size (if compression is used)
    pub fn validate_original_size(
        &self,
        file_size: u64,
        original_size: Option<u64>,
    ) -> Result<()> {
        if let Some(orig_size) = original_size {
            if orig_size == 0 {
                return Err(Error::Protocol(
                    "Original size cannot be zero".to_string()
                ));
            }

            if orig_size > MAX_FILE_SIZE {
                return Err(Error::Protocol(format!(
                    "Original size too large: {} bytes",
                    orig_size
                )));
            }

            // In most cases, compressed size should be <= original size
            // (though this isn't always true for small files or incompressible data)
            if self.strict_mode && file_size > orig_size * 2 {
                // Allow some tolerance for edge cases
                return Err(Error::Protocol(format!(
                    "Compressed size ({}) suspiciously larger than original ({})",
                    file_size, orig_size
                )));
            }
        }

        Ok(())
    }

    /// Quick validation - only checks critical fields
    pub fn validate_quick(&self, manifest: &Manifest) -> Result<()> {
        self.validate_session_id(&manifest.session_id)?;
        self.validate_file_size(manifest.file_size)?;
        self.validate_chunk_size(manifest.chunk_size)?;
        self.validate_chunk_count(manifest.file_size, manifest.chunk_size, manifest.total_chunks)?;
        
        Ok(())
    }
}

impl Default for ManifestValidator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_valid_manifest() -> Manifest {
        Manifest {
            session_id: "test-session-12345678".to_string(), // Must be at least 8 chars
            file_name: "test.txt".to_string(),
            file_size: 4096,
            chunk_size: 1024, // Must be at least MIN_CHUNK_SIZE (1024)
            total_chunks: 4,
            file_hash: vec![0u8; 32],
            chunk_hashes: vec![vec![0u8; 32]; 4],
            compression: "none".to_string(),
            original_size: Some(4096),
        }
    }

    #[test]
    fn test_valid_manifest() {
        let validator = ManifestValidator::new();
        let manifest = create_valid_manifest();
        
        let result = validator.validate(&manifest);
        if let Err(e) = &result {
            eprintln!("Validation failed: {:?}", e);
        }
        assert!(result.is_ok());
    }

    #[test]
    fn test_invalid_session_id() {
        let validator = ManifestValidator::new();
        
        // Empty session ID
        assert!(validator.validate_session_id("").is_err());
        
        // Too short
        assert!(validator.validate_session_id("short").is_err());
        
        // Invalid characters
        assert!(validator.validate_session_id("test session 123!").is_err());
        
        // Valid session ID
        assert!(validator.validate_session_id("test-session-123").is_ok());
    }

    #[test]
    fn test_invalid_file_name() {
        let validator = ManifestValidator::new();
        
        // Empty file name
        assert!(validator.validate_file_name("").is_err());
        
        // Path traversal
        assert!(validator.validate_file_name("../etc/passwd").is_err());
        assert!(validator.validate_file_name("test//file.txt").is_err());
        
        // Valid file name
        assert!(validator.validate_file_name("test.txt").is_ok());
        assert!(validator.validate_file_name("my-file_123.dat").is_ok());
    }

    #[test]
    fn test_invalid_chunk_count() {
        let validator = ManifestValidator::new();
        
        // Chunk count mismatch
        assert!(validator.validate_chunk_count(1024, 256, 5).is_err()); // Should be 4
        assert!(validator.validate_chunk_count(1024, 256, 3).is_err()); // Should be 4
        
        // Valid
        assert!(validator.validate_chunk_count(1024, 256, 4).is_ok());
        assert!(validator.validate_chunk_count(1000, 256, 4).is_ok()); // 1000/256 = 3.9... -> 4
    }

    #[test]
    fn test_invalid_hash_sizes() {
        let validator = ManifestValidator::new();
        
        // File hash too small
        assert!(validator.validate_file_hash(&vec![0u8; 16]).is_err());
        
        // File hash too large
        assert!(validator.validate_file_hash(&vec![0u8; 64]).is_err());
        
        // Valid BLAKE3 hash
        assert!(validator.validate_file_hash(&vec![0u8; 32]).is_ok());
    }

    #[test]
    fn test_chunk_hash_count_mismatch() {
        let validator = ManifestValidator::new();
        
        // Wrong number of chunk hashes
        let hashes = vec![vec![0u8; 32]; 3];
        assert!(validator.validate_chunk_hashes(&hashes, 4).is_err());
        
        // Correct number
        let hashes = vec![vec![0u8; 32]; 4];
        assert!(validator.validate_chunk_hashes(&hashes, 4).is_ok());
    }

    #[test]
    fn test_invalid_chunk_hash_size() {
        let validator = ManifestValidator::new();
        
        // One hash is wrong size
        let mut hashes = vec![vec![0u8; 32]; 4];
        hashes[2] = vec![0u8; 16]; // Wrong size
        
        assert!(validator.validate_chunk_hashes(&hashes, 4).is_err());
    }

    #[test]
    fn test_compression_validation_strict() {
        let validator = ManifestValidator::strict();
        
        assert!(validator.validate_compression("none").is_ok());
        assert!(validator.validate_compression("lz4").is_ok());
        assert!(validator.validate_compression("zstd").is_ok());
        assert!(validator.validate_compression("invalid").is_err());
        assert!(validator.validate_compression("gzip").is_err()); // Not supported
    }

    #[test]
    fn test_full_validation() {
        let validator = ManifestValidator::strict();
        let mut manifest = create_valid_manifest();
        
        // Valid manifest
        assert!(validator.validate(&manifest).is_ok());
        
        // Break chunk count
        manifest.total_chunks = 5;
        assert!(validator.validate(&manifest).is_err());
        manifest.total_chunks = 4;
        
        // Break chunk hashes
        manifest.chunk_hashes.push(vec![0u8; 32]);
        assert!(validator.validate(&manifest).is_err());
        manifest.chunk_hashes.pop();
        
        // Break compression
        manifest.compression = "invalid".to_string();
        assert!(validator.validate(&manifest).is_err());
    }

    #[test]
    fn test_quick_validation() {
        let validator = ManifestValidator::new();
        let manifest = create_valid_manifest();
        
        // Quick validation should pass
        assert!(validator.validate_quick(&manifest).is_ok());
        
        // Quick validation doesn't check hashes deeply
        let mut bad_manifest = manifest.clone();
        bad_manifest.chunk_hashes = vec![]; // Empty hashes
        
        // Full validation would fail, but quick might pass some checks
        assert!(validator.validate_quick(&bad_manifest).is_ok());
    }
}

