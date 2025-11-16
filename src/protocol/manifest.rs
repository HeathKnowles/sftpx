// Manifest message structures

use crate::common::error::{Error, Result};
use crate::protocol::messages::Manifest;
use std::fs::File;
use std::path::Path;

/// Default chunk size (1 MB)
const DEFAULT_CHUNK_SIZE: u32 = 1024 * 1024;

/// Builder for creating manifest instances from files
#[derive(Clone)]
pub struct ManifestBuilder {
    session_id: String,
    file_path: Option<std::path::PathBuf>,
    chunk_size: u32,
    compression: String,
}

impl ManifestBuilder {
    /// Create a new manifest builder
    /// 
    /// # Arguments
    /// * `session_id` - Unique session identifier
    pub fn new(session_id: impl Into<String>) -> Self {
        Self {
            session_id: session_id.into(),
            file_path: None,
            chunk_size: DEFAULT_CHUNK_SIZE,
            compression: "none".to_string(),
        }
    }

    /// Set the file path to build manifest from
    pub fn file_path(mut self, path: impl AsRef<Path>) -> Self {
        self.file_path = Some(path.as_ref().to_path_buf());
        self
    }

    /// Set the chunk size
    pub fn chunk_size(mut self, size: u32) -> Self {
        self.chunk_size = size;
        self
    }

    /// Set the compression algorithm
    pub fn compression(mut self, algorithm: impl Into<String>) -> Self {
        self.compression = algorithm.into();
        self
    }

    /// Build the manifest by reading and hashing the file
    /// 
    /// # Returns
    /// * `Ok(Manifest)` - Successfully built manifest
    /// * `Err(Error)` - If file cannot be read or processed
    pub fn build(self) -> Result<Manifest> {
        self.build_internal(false)
    }
    
    /// Build the manifest using parallel hash computation for better performance
    pub fn build_parallel(self) -> Result<Manifest> {
        self.build_internal(true)
    }
    
    fn build_internal(self, use_parallel: bool) -> Result<Manifest> {
        let file_path = self.file_path.ok_or_else(|| {
            Error::Protocol("File path not set".to_string())
        })?;

        // Open and get file metadata
        let file = File::open(&file_path)?;
        let metadata = file.metadata()?;
        let file_size = metadata.len();

        if file_size == 0 {
            return Err(Error::Protocol("File is empty".to_string()));
        }

        // Get file name
        let file_name = file_path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| Error::Protocol("Invalid file name".to_string()))?
            .to_string();

        // Calculate total chunks
        let total_chunks = (file_size + self.chunk_size as u64 - 1) / self.chunk_size as u64;

        // Compute chunk hashes - use parallel version if requested and file is large enough
        let chunk_hashes = if use_parallel && total_chunks > 4 {
            use crate::chunking::compute_chunk_hashes_parallel;
            compute_chunk_hashes_parallel(&file_path, self.chunk_size as usize)?
        } else {
            // Sequential version for small files
            Self::compute_hashes_sequential(&file_path, file_size, total_chunks, self.chunk_size)?
        };
        
        // Compute file hash (always sequential since we need to read entire file)
        let file_hash_bytes = Self::compute_file_hash(&file_path, file_size, self.chunk_size)?;

        // Create manifest
        let manifest = Manifest {
            session_id: self.session_id,
            file_name,
            file_size,
            chunk_size: self.chunk_size,
            total_chunks,
            file_hash: file_hash_bytes,
            chunk_hashes,
            compression: self.compression.clone(),
            original_size: if self.compression == "none" {
                None
            } else {
                Some(file_size)
            },
        };

        Ok(manifest)
    }
    
    /// Compute chunk hashes sequentially (for small files or fallback)
    fn compute_hashes_sequential(
        file_path: &Path,
        file_size: u64,
        total_chunks: u64,
        chunk_size: u32,
    ) -> Result<Vec<Vec<u8>>> {
        use std::io::{Read, Seek, SeekFrom};
        use crate::chunking::hasher::ChunkHasher;
        
        let mut file = File::open(file_path)?;
        let mut chunk_hashes = Vec::with_capacity(total_chunks as usize);
        let mut buffer = vec![0u8; chunk_size as usize];
        let mut bytes_read_total = 0u64;

        file.seek(SeekFrom::Start(0))?;

        // Process each chunk
        for _ in 0..total_chunks {
            let remaining = file_size - bytes_read_total;
            let to_read = std::cmp::min(remaining, chunk_size as u64) as usize;

            // Read chunk
            let bytes_read = file.read(&mut buffer[..to_read])?;
            if bytes_read == 0 {
                break;
            }

            let chunk_data = &buffer[..bytes_read];

            // Compute chunk hash
            let chunk_hash = ChunkHasher::hash(chunk_data);
            chunk_hashes.push(chunk_hash);

            bytes_read_total += bytes_read as u64;
        }
        
        Ok(chunk_hashes)
    }
    
    /// Compute file hash
    fn compute_file_hash(file_path: &Path, file_size: u64, chunk_size: u32) -> Result<Vec<u8>> {
        use std::io::Read;
        
        let mut file = File::open(file_path)?;
        let mut file_hasher = blake3::Hasher::new();
        let mut buffer = vec![0u8; chunk_size as usize];
        let mut bytes_read_total = 0u64;

        while bytes_read_total < file_size {
            let remaining = file_size - bytes_read_total;
            let to_read = std::cmp::min(remaining, chunk_size as u64) as usize;
            
            let bytes_read = file.read(&mut buffer[..to_read])?;
            if bytes_read == 0 {
                break;
            }
            
            file_hasher.update(&buffer[..bytes_read]);
            bytes_read_total += bytes_read as u64;
        }
        
        let file_hash = file_hasher.finalize();
        Ok(file_hash.as_bytes().to_vec())
    }

    /// Build manifest from an already-chunked file with provided hashes
    /// Useful when chunks have already been processed
    /// 
    /// # Arguments
    /// * `file_name` - Name of the file
    /// * `file_size` - Size of the file in bytes
    /// * `file_hash` - BLAKE3 hash of the entire file
    /// * `chunk_hashes` - Vector of BLAKE3 hashes for each chunk
    pub fn build_from_hashes(
        self,
        file_name: String,
        file_size: u64,
        file_hash: Vec<u8>,
        chunk_hashes: Vec<Vec<u8>>,
    ) -> Result<Manifest> {
        // Validate inputs
        if file_name.is_empty() {
            return Err(Error::Protocol("File name cannot be empty".to_string()));
        }

        if file_size == 0 {
            return Err(Error::Protocol("File size cannot be zero".to_string()));
        }

        if file_hash.len() != 32 {
            return Err(Error::Protocol("File hash must be 32 bytes (BLAKE3)".to_string()));
        }

        // Calculate expected chunk count
        let total_chunks = (file_size + self.chunk_size as u64 - 1) / self.chunk_size as u64;

        if chunk_hashes.len() as u64 != total_chunks {
            return Err(Error::Protocol(format!(
                "Chunk hash count mismatch: got {}, expected {}",
                chunk_hashes.len(),
                total_chunks
            )));
        }

        // Validate all chunk hashes are correct size
        for (idx, hash) in chunk_hashes.iter().enumerate() {
            if hash.len() != 32 {
                return Err(Error::Protocol(format!(
                    "Chunk hash {} has invalid size: {} bytes (expected 32)",
                    idx,
                    hash.len()
                )));
            }
        }

        let manifest = Manifest {
            session_id: self.session_id,
            file_name,
            file_size,
            chunk_size: self.chunk_size,
            total_chunks,
            file_hash,
            chunk_hashes,
            compression: self.compression.clone(),
            original_size: if self.compression == "none" {
                None
            } else {
                Some(file_size)
            },
        };

        Ok(manifest)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_manifest_builder_basic() {
        // Create a temporary file
        let mut temp_file = NamedTempFile::new().unwrap();
        let test_data = b"Hello, World! This is test data for manifest building.";
        temp_file.write_all(test_data).unwrap();
        temp_file.flush().unwrap();

        // Build manifest
        let manifest = ManifestBuilder::new("test-session-123")
            .file_path(temp_file.path())
            .chunk_size(16)
            .build()
            .unwrap();

        // Verify
        assert_eq!(manifest.session_id, "test-session-123");
        assert_eq!(manifest.file_size, test_data.len() as u64);
        assert_eq!(manifest.chunk_size, 16);
        assert_eq!(manifest.total_chunks, 4); // 56 bytes / 16 = 3.5 -> 4 chunks
        assert_eq!(manifest.chunk_hashes.len(), 4);
        assert_eq!(manifest.file_hash.len(), 32); // BLAKE3 hash
    }

    #[test]
    fn test_manifest_builder_with_compression() {
        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(b"test data").unwrap();
        temp_file.flush().unwrap();

        let manifest = ManifestBuilder::new("session-456")
            .file_path(temp_file.path())
            .compression("lz4")
            .build()
            .unwrap();

        assert_eq!(manifest.compression, "lz4");
        assert_eq!(manifest.original_size, Some(9)); // "test data" is 9 bytes
    }

    #[test]
    fn test_manifest_builder_chunk_hashes() {
        let mut temp_file = NamedTempFile::new().unwrap();
        let test_data = vec![0u8; 100]; // 100 bytes of zeros
        temp_file.write_all(&test_data).unwrap();
        temp_file.flush().unwrap();

        let manifest = ManifestBuilder::new("session-789")
            .file_path(temp_file.path())
            .chunk_size(30)
            .build()
            .unwrap();

        // 100 / 30 = 3.33... -> 4 chunks
        assert_eq!(manifest.total_chunks, 4);
        assert_eq!(manifest.chunk_hashes.len(), 4);

        // Each hash should be 32 bytes (BLAKE3)
        for hash in &manifest.chunk_hashes {
            assert_eq!(hash.len(), 32);
        }
    }

    #[test]
    fn test_manifest_builder_file_hash() {
        let mut temp_file = NamedTempFile::new().unwrap();
        let test_data = b"consistent data";
        temp_file.write_all(test_data).unwrap();
        temp_file.flush().unwrap();

        // Build manifest twice
        let manifest1 = ManifestBuilder::new("session-1")
            .file_path(temp_file.path())
            .build()
            .unwrap();

        let manifest2 = ManifestBuilder::new("session-2")
            .file_path(temp_file.path())
            .build()
            .unwrap();

        // File hashes should be identical (deterministic)
        assert_eq!(manifest1.file_hash, manifest2.file_hash);
    }

    #[test]
    fn test_manifest_builder_from_hashes() {
        let file_hash = vec![0u8; 32];
        let chunk_hashes = vec![vec![0u8; 32]; 4];

        let manifest = ManifestBuilder::new("session-hash")
            .chunk_size(256)
            .build_from_hashes(
                "test.dat".to_string(),
                1024,
                file_hash.clone(),
                chunk_hashes.clone(),
            )
            .unwrap();

        assert_eq!(manifest.file_name, "test.dat");
        assert_eq!(manifest.file_size, 1024);
        assert_eq!(manifest.total_chunks, 4);
        assert_eq!(manifest.file_hash, file_hash);
        assert_eq!(manifest.chunk_hashes, chunk_hashes);
    }

    #[test]
    fn test_manifest_builder_from_hashes_validation() {
        let builder = ManifestBuilder::new("session-val").chunk_size(256);

        // Invalid file hash size
        let result = builder.clone().build_from_hashes(
            "test.dat".to_string(),
            1024,
            vec![0u8; 16], // Wrong size
            vec![vec![0u8; 32]; 4],
        );
        assert!(result.is_err());

        // Chunk count mismatch
        let result = builder.clone().build_from_hashes(
            "test.dat".to_string(),
            1024,
            vec![0u8; 32],
            vec![vec![0u8; 32]; 3], // Should be 4
        );
        assert!(result.is_err());

        // Invalid chunk hash size
        let mut chunk_hashes = vec![vec![0u8; 32]; 4];
        chunk_hashes[2] = vec![0u8; 16]; // Wrong size
        let result = builder.clone().build_from_hashes(
            "test.dat".to_string(),
            1024,
            vec![0u8; 32],
            chunk_hashes,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_manifest_builder_no_file_path() {
        let builder = ManifestBuilder::new("session-nofile");
        let result = builder.build();
        assert!(result.is_err());
    }

    #[test]
    fn test_manifest_builder_empty_file() {
        let temp_file = NamedTempFile::new().unwrap();
        // Don't write anything - empty file

        let result = ManifestBuilder::new("session-empty")
            .file_path(temp_file.path())
            .build();
        
        assert!(result.is_err());
    }
}

