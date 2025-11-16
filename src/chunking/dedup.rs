// Chunk deduplication based on content hashing
use crate::common::error::{Error, Result};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::fs;
use std::io::{BufReader, BufRead, Write};

/// Maps chunk hashes (BLAKE3) to file locations
/// Format: hash -> (file_path, byte_offset, chunk_size)
#[derive(Debug, Clone)]
pub struct ChunkHashIndex {
    index: HashMap<Vec<u8>, Vec<ChunkLocation>>,
    index_file: PathBuf,
}

#[derive(Debug, Clone)]
pub struct ChunkLocation {
    pub file_path: PathBuf,
    pub byte_offset: u64,
    pub chunk_size: u32,
}

impl ChunkHashIndex {
    /// Create a new chunk hash index
    pub fn new(index_dir: &Path) -> Result<Self> {
        fs::create_dir_all(index_dir)?;
        let index_file = index_dir.join("chunk_index.db");
        
        let mut index = Self {
            index: HashMap::new(),
            index_file,
        };
        
        // Load existing index if it exists
        index.load()?;
        
        Ok(index)
    }
    
    /// Add a chunk to the index
    pub fn add_chunk(&mut self, hash: Vec<u8>, location: ChunkLocation) {
        self.index.entry(hash)
            .or_insert_with(Vec::new)
            .push(location);
    }
    
    /// Check if a chunk with this hash exists
    pub fn has_chunk(&self, hash: &[u8]) -> bool {
        self.index.contains_key(hash)
    }
    
    /// Get all locations for a chunk hash
    pub fn get_locations(&self, hash: &[u8]) -> Option<&Vec<ChunkLocation>> {
        self.index.get(hash)
    }
    
    /// Check which hashes from a list already exist
    /// Returns a HashSet of hashes that exist
    pub fn check_hashes(&self, hashes: &[Vec<u8>]) -> Vec<Vec<u8>> {
        hashes.iter()
            .filter(|hash| self.has_chunk(hash))
            .cloned()
            .collect()
    }
    
    /// Get total number of unique chunks
    pub fn total_chunks(&self) -> usize {
        self.index.len()
    }
    
    /// Save index to disk
    pub fn save(&self) -> Result<()> {
        let mut file = fs::File::create(&self.index_file)?;
        
        for (hash, locations) in &self.index {
            for location in locations {
                // Format: hash_hex|file_path|byte_offset|chunk_size
                let hash_hex = hex::encode(hash);
                let line = format!("{}|{}|{}|{}\n",
                    hash_hex,
                    location.file_path.display(),
                    location.byte_offset,
                    location.chunk_size
                );
                file.write_all(line.as_bytes())?;
            }
        }
        
        Ok(())
    }
    
    /// Load index from disk
    pub fn load(&mut self) -> Result<()> {
        if !self.index_file.exists() {
            return Ok(());
        }
        
        let file = fs::File::open(&self.index_file)?;
        let reader = BufReader::new(file);
        
        for line in reader.lines() {
            let line = line?;
            let parts: Vec<&str> = line.split('|').collect();
            
            if parts.len() != 4 {
                continue;
            }
            
            let hash = hex::decode(parts[0])
                .map_err(|e| Error::Protocol(format!("Invalid hash in index: {}", e)))?;
            let file_path = PathBuf::from(parts[1]);
            let byte_offset = parts[2].parse::<u64>()
                .map_err(|e| Error::Protocol(format!("Invalid offset in index: {}", e)))?;
            let chunk_size = parts[3].parse::<u32>()
                .map_err(|e| Error::Protocol(format!("Invalid size in index: {}", e)))?;
            
            let location = ChunkLocation {
                file_path,
                byte_offset,
                chunk_size,
            };
            
            self.add_chunk(hash, location);
        }
        
        Ok(())
    }
    
    /// Clear the index
    pub fn clear(&mut self) {
        self.index.clear();
    }
    
    /// Remove entries for a specific file (e.g., when file is deleted)
    pub fn remove_file(&mut self, file_path: &Path) {
        self.index.retain(|_, locations| {
            locations.retain(|loc| loc.file_path != file_path);
            !locations.is_empty()
        });
    }
}

/// Deduplication statistics
#[derive(Debug, Default, Clone)]
pub struct DedupStats {
    pub total_chunks: u64,
    pub duplicate_chunks: u64,
    pub bytes_saved: u64,
    pub unique_chunks: u64,
}

impl DedupStats {
    pub fn dedup_ratio(&self) -> f64 {
        if self.total_chunks == 0 {
            return 0.0;
        }
        self.duplicate_chunks as f64 / self.total_chunks as f64
    }
    
    pub fn bytes_saved_mb(&self) -> f64 {
        self.bytes_saved as f64 / 1_048_576.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    
    #[test]
    fn test_chunk_hash_index() {
        let temp_dir = TempDir::new().unwrap();
        let mut index = ChunkHashIndex::new(temp_dir.path()).unwrap();
        
        let hash1 = vec![1, 2, 3, 4];
        let hash2 = vec![5, 6, 7, 8];
        
        let location1 = ChunkLocation {
            file_path: PathBuf::from("/tmp/file1.txt"),
            byte_offset: 0,
            chunk_size: 1024,
        };
        
        index.add_chunk(hash1.clone(), location1);
        
        assert!(index.has_chunk(&hash1));
        assert!(!index.has_chunk(&hash2));
        assert_eq!(index.total_chunks(), 1);
    }
    
    #[test]
    fn test_check_hashes() {
        let temp_dir = TempDir::new().unwrap();
        let mut index = ChunkHashIndex::new(temp_dir.path()).unwrap();
        
        let hash1 = vec![1, 2, 3, 4];
        let hash2 = vec![5, 6, 7, 8];
        let hash3 = vec![9, 10, 11, 12];
        
        let location = ChunkLocation {
            file_path: PathBuf::from("/tmp/file.txt"),
            byte_offset: 0,
            chunk_size: 1024,
        };
        
        index.add_chunk(hash1.clone(), location.clone());
        index.add_chunk(hash3.clone(), location);
        
        let to_check = vec![hash1.clone(), hash2.clone(), hash3.clone()];
        let existing = index.check_hashes(&to_check);
        
        assert_eq!(existing.len(), 2);
        assert!(existing.contains(&hash1));
        assert!(existing.contains(&hash3));
        assert!(!existing.contains(&hash2));
    }
    
    #[test]
    fn test_save_and_load() {
        let temp_dir = TempDir::new().unwrap();
        
        let hash = vec![1, 2, 3, 4];
        let location = ChunkLocation {
            file_path: PathBuf::from("/tmp/test.txt"),
            byte_offset: 1024,
            chunk_size: 256,
        };
        
        // Create and save
        {
            let mut index = ChunkHashIndex::new(temp_dir.path()).unwrap();
            index.add_chunk(hash.clone(), location.clone());
            index.save().unwrap();
        }
        
        // Load in new instance
        {
            let index = ChunkHashIndex::new(temp_dir.path()).unwrap();
            assert!(index.has_chunk(&hash));
            assert_eq!(index.total_chunks(), 1);
            
            let locations = index.get_locations(&hash).unwrap();
            assert_eq!(locations.len(), 1);
            assert_eq!(locations[0].byte_offset, 1024);
            assert_eq!(locations[0].chunk_size, 256);
        }
    }
}
