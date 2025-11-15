// Chunk table and metadata
use std::collections::HashMap;
use serde::{Deserialize, Serialize};

/// Metadata for a single chunk
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChunkMetadata {
    /// Chunk sequence number
    pub chunk_number: u64,
    /// Byte offset in the file where this chunk starts
    pub byte_offset: u64,
    /// Length of this chunk in bytes
    pub chunk_length: u32,
    /// Checksum/hash of the chunk data
    pub checksum: Vec<u8>,
    /// Flag indicating if this is the last chunk (end of file)
    pub end_of_file_flag: bool,
}

impl ChunkMetadata {
    /// Create new chunk metadata
    pub fn new(
        chunk_number: u64,
        byte_offset: u64,
        chunk_length: u32,
        checksum: Vec<u8>,
        end_of_file_flag: bool,
    ) -> Self {
        Self {
            chunk_number,
            byte_offset,
            chunk_length,
            checksum,
            end_of_file_flag,
        }
    }

    /// Get the end byte offset (exclusive) of this chunk
    pub fn end_offset(&self) -> u64 {
        self.byte_offset + self.chunk_length as u64
    }

    /// Check if this chunk is the final chunk
    pub fn is_last_chunk(&self) -> bool {
        self.end_of_file_flag
    }
}

/// Table for storing chunk metadata, indexed by chunk number
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkTable {
    /// Map of chunk_number -> ChunkMetadata
    chunks: HashMap<u64, ChunkMetadata>,
    /// Total file size in bytes
    total_size: u64,
    /// Total number of chunks expected
    total_chunks: u64,
}

impl ChunkTable {
    /// Create a new empty chunk table
    pub fn new() -> Self {
        Self {
            chunks: HashMap::new(),
            total_size: 0,
            total_chunks: 0,
        }
    }

    /// Create a chunk table with expected capacity
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            chunks: HashMap::with_capacity(capacity),
            total_size: 0,
            total_chunks: 0,
        }
    }

    /// Set the total file size and chunk count
    pub fn set_file_info(&mut self, total_size: u64, total_chunks: u64) {
        self.total_size = total_size;
        self.total_chunks = total_chunks;
    }

    /// Insert or update chunk metadata
    pub fn insert(&mut self, metadata: ChunkMetadata) -> Option<ChunkMetadata> {
        self.chunks.insert(metadata.chunk_number, metadata)
    }

    /// Get metadata for a specific chunk
    pub fn get(&self, chunk_number: u64) -> Option<&ChunkMetadata> {
        self.chunks.get(&chunk_number)
    }

    /// Check if metadata exists for a chunk
    pub fn contains(&self, chunk_number: u64) -> bool {
        self.chunks.contains_key(&chunk_number)
    }

    /// Remove metadata for a chunk
    pub fn remove(&mut self, chunk_number: u64) -> Option<ChunkMetadata> {
        self.chunks.remove(&chunk_number)
    }

    /// Get the number of chunks stored in the table
    pub fn len(&self) -> usize {
        self.chunks.len()
    }

    /// Check if the table is empty
    pub fn is_empty(&self) -> bool {
        self.chunks.is_empty()
    }

    /// Get total file size
    pub fn total_size(&self) -> u64 {
        self.total_size
    }

    /// Get expected total chunk count
    pub fn total_chunks(&self) -> u64 {
        self.total_chunks
    }

    /// Check if all chunks have metadata stored
    pub fn is_complete(&self) -> bool {
        if self.total_chunks == 0 {
            return false;
        }
        self.chunks.len() as u64 == self.total_chunks
    }

    /// Get all chunk numbers that have metadata
    pub fn chunk_numbers(&self) -> Vec<u64> {
        let mut numbers: Vec<u64> = self.chunks.keys().copied().collect();
        numbers.sort_unstable();
        numbers
    }

    /// Find missing chunk numbers (gaps in sequence)
    pub fn missing_chunks(&self) -> Vec<u64> {
        if self.total_chunks == 0 {
            return Vec::new();
        }

        let mut missing = Vec::new();
        for chunk_num in 0..self.total_chunks {
            if !self.chunks.contains_key(&chunk_num) {
                missing.push(chunk_num);
            }
        }
        missing
    }

    /// Get an iterator over all chunk metadata, sorted by chunk number
    pub fn iter_sorted(&self) -> impl Iterator<Item = &ChunkMetadata> {
        let mut entries: Vec<_> = self.chunks.values().collect();
        entries.sort_by_key(|m| m.chunk_number);
        entries.into_iter()
    }

    /// Calculate total bytes covered by stored chunks
    pub fn bytes_stored(&self) -> u64 {
        self.chunks.values().map(|m| m.chunk_length as u64).sum()
    }

    /// Get the chunk metadata for the last chunk (EOF chunk)
    pub fn last_chunk(&self) -> Option<&ChunkMetadata> {
        self.chunks.values().find(|m| m.end_of_file_flag)
    }

    /// Clear all chunk metadata
    pub fn clear(&mut self) {
        self.chunks.clear();
    }

    /// Verify chunk sequence integrity (no overlaps, correct offsets)
    pub fn verify_integrity(&self) -> Result<(), String> {
        if self.is_empty() {
            return Ok(());
        }

        let mut sorted_chunks: Vec<_> = self.chunks.values().collect();
        sorted_chunks.sort_by_key(|m| m.chunk_number);

        // Check for sequential chunk numbers starting from 0
        for (i, metadata) in sorted_chunks.iter().enumerate() {
            if metadata.chunk_number != i as u64 {
                return Err(format!(
                    "Chunk number gap: expected {}, found {}",
                    i, metadata.chunk_number
                ));
            }
        }

        // Check offsets are sequential and non-overlapping
        for i in 0..sorted_chunks.len() - 1 {
            let current = sorted_chunks[i];
            let next = sorted_chunks[i + 1];

            if current.end_offset() != next.byte_offset {
                return Err(format!(
                    "Chunk offset mismatch: chunk {} ends at {}, chunk {} starts at {}",
                    current.chunk_number,
                    current.end_offset(),
                    next.chunk_number,
                    next.byte_offset
                ));
            }
        }

        // Check EOF flag is only on the last chunk
        let eof_count = sorted_chunks.iter().filter(|m| m.end_of_file_flag).count();
        if eof_count > 1 {
            return Err(format!("Multiple chunks marked as EOF: {}", eof_count));
        }

        if eof_count == 1 && !sorted_chunks.last().unwrap().end_of_file_flag {
            return Err("EOF flag set on non-final chunk".to_string());
        }

        Ok(())
    }
}

impl Default for ChunkTable {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_metadata(chunk_num: u64, offset: u64, length: u32, is_last: bool) -> ChunkMetadata {
        ChunkMetadata::new(
            chunk_num,
            offset,
            length,
            vec![0xAB; 32], // dummy checksum
            is_last,
        )
    }

    #[test]
    fn test_chunk_metadata_creation() {
        let metadata = create_test_metadata(5, 1024, 512, false);
        assert_eq!(metadata.chunk_number, 5);
        assert_eq!(metadata.byte_offset, 1024);
        assert_eq!(metadata.chunk_length, 512);
        assert_eq!(metadata.end_offset(), 1536);
        assert!(!metadata.is_last_chunk());
    }

    #[test]
    fn test_chunk_metadata_eof() {
        let metadata = create_test_metadata(10, 5000, 100, true);
        assert!(metadata.is_last_chunk());
        assert_eq!(metadata.end_offset(), 5100);
    }

    #[test]
    fn test_chunk_table_insert_and_get() {
        let mut table = ChunkTable::new();
        let metadata = create_test_metadata(0, 0, 1024, false);

        table.insert(metadata.clone());
        assert_eq!(table.len(), 1);
        assert_eq!(table.get(0), Some(&metadata));
        assert_eq!(table.get(1), None);
    }

    #[test]
    fn test_chunk_table_contains() {
        let mut table = ChunkTable::new();
        table.insert(create_test_metadata(5, 5120, 1024, false));

        assert!(table.contains(5));
        assert!(!table.contains(0));
        assert!(!table.contains(6));
    }

    #[test]
    fn test_chunk_table_remove() {
        let mut table = ChunkTable::new();
        let metadata = create_test_metadata(3, 3072, 1024, false);

        table.insert(metadata.clone());
        assert_eq!(table.len(), 1);

        let removed = table.remove(3);
        assert_eq!(removed, Some(metadata));
        assert_eq!(table.len(), 0);
        assert!(!table.contains(3));
    }

    #[test]
    fn test_chunk_table_is_complete() {
        let mut table = ChunkTable::new();
        table.set_file_info(3072, 3);

        assert!(!table.is_complete());

        table.insert(create_test_metadata(0, 0, 1024, false));
        table.insert(create_test_metadata(1, 1024, 1024, false));
        assert!(!table.is_complete());

        table.insert(create_test_metadata(2, 2048, 1024, true));
        assert!(table.is_complete());
    }

    #[test]
    fn test_chunk_table_missing_chunks() {
        let mut table = ChunkTable::new();
        table.set_file_info(5120, 5);

        table.insert(create_test_metadata(0, 0, 1024, false));
        table.insert(create_test_metadata(2, 2048, 1024, false));
        table.insert(create_test_metadata(4, 4096, 1024, true));

        let missing = table.missing_chunks();
        assert_eq!(missing, vec![1, 3]);
    }

    #[test]
    fn test_chunk_table_chunk_numbers() {
        let mut table = ChunkTable::new();
        table.insert(create_test_metadata(3, 3072, 1024, false));
        table.insert(create_test_metadata(0, 0, 1024, false));
        table.insert(create_test_metadata(1, 1024, 1024, false));

        let numbers = table.chunk_numbers();
        assert_eq!(numbers, vec![0, 1, 3]);
    }

    #[test]
    fn test_chunk_table_bytes_stored() {
        let mut table = ChunkTable::new();
        table.insert(create_test_metadata(0, 0, 1024, false));
        table.insert(create_test_metadata(1, 1024, 512, false));
        table.insert(create_test_metadata(2, 1536, 256, true));

        assert_eq!(table.bytes_stored(), 1792);
    }

    #[test]
    fn test_chunk_table_last_chunk() {
        let mut table = ChunkTable::new();
        table.insert(create_test_metadata(0, 0, 1024, false));
        table.insert(create_test_metadata(1, 1024, 1024, false));
        let last = create_test_metadata(2, 2048, 512, true);
        table.insert(last.clone());

        assert_eq!(table.last_chunk(), Some(&last));
    }

    #[test]
    fn test_chunk_table_clear() {
        let mut table = ChunkTable::new();
        table.insert(create_test_metadata(0, 0, 1024, false));
        table.insert(create_test_metadata(1, 1024, 1024, false));

        assert_eq!(table.len(), 2);
        table.clear();
        assert_eq!(table.len(), 0);
        assert!(table.is_empty());
    }

    #[test]
    fn test_chunk_table_verify_integrity_success() {
        let mut table = ChunkTable::new();
        table.insert(create_test_metadata(0, 0, 1024, false));
        table.insert(create_test_metadata(1, 1024, 1024, false));
        table.insert(create_test_metadata(2, 2048, 512, true));

        assert!(table.verify_integrity().is_ok());
    }

    #[test]
    fn test_chunk_table_verify_integrity_gap() {
        let mut table = ChunkTable::new();
        table.insert(create_test_metadata(0, 0, 1024, false));
        table.insert(create_test_metadata(2, 2048, 1024, false)); // missing chunk 1

        let result = table.verify_integrity();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("gap"));
    }

    #[test]
    fn test_chunk_table_verify_integrity_offset_mismatch() {
        let mut table = ChunkTable::new();
        table.insert(create_test_metadata(0, 0, 1024, false));
        table.insert(create_test_metadata(1, 2000, 1024, false)); // wrong offset

        let result = table.verify_integrity();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("offset mismatch"));
    }

    #[test]
    fn test_chunk_table_verify_integrity_multiple_eof() {
        let mut table = ChunkTable::new();
        table.insert(create_test_metadata(0, 0, 1024, true)); // EOF on first
        table.insert(create_test_metadata(1, 1024, 1024, true)); // EOF on second

        let result = table.verify_integrity();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Multiple chunks marked as EOF"));
    }

    #[test]
    fn test_chunk_table_serialization() {
        let mut table = ChunkTable::new();
        table.set_file_info(2048, 2);
        table.insert(create_test_metadata(0, 0, 1024, false));
        table.insert(create_test_metadata(1, 1024, 1024, true));

        // Serialize to JSON
        let json = serde_json::to_string(&table).unwrap();
        
        // Deserialize back
        let restored: ChunkTable = serde_json::from_str(&json).unwrap();
        
        assert_eq!(restored.len(), 2);
        assert_eq!(restored.total_size(), 2048);
        assert_eq!(restored.total_chunks(), 2);
        assert!(restored.is_complete());
    }
}

