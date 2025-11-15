// Client-side file receiving logic

use std::path::{Path, PathBuf};
use std::fs::{File, OpenOptions};
use std::io::{Write, Seek, SeekFrom};
use std::collections::HashSet;
use crate::common::error::{Error, Result};
use crate::common::types::ChunkId;
use crate::protocol::chunk::{ChunkPacketParser, ChunkPacketView};

/// File receiver for assembling chunks into a complete file
pub struct FileReceiver {
    part_file: File,
    part_file_path: PathBuf,
    final_file_path: PathBuf,
    file_size: u64,
    received_chunks: HashSet<ChunkId>,
    total_chunks: u64,
    bytes_received: u64,
    end_of_file_received: bool,
}

impl FileReceiver {
    /// Create a new file receiver
    /// 
    /// # Arguments
    /// * `output_dir` - Directory to save the file to
    /// * `filename` - Name of the output file
    /// * `file_size` - Expected total file size in bytes
    pub fn new(output_dir: &Path, filename: &str, file_size: u64) -> Result<Self> {
        let final_file_path = output_dir.join(filename);
        let part_file_path = output_dir.join(format!("{}.part", filename));
        
        // Create or open .part file
        let part_file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(&part_file_path)?;
        
        // Pre-allocate space if possible
        if file_size > 0 {
            part_file.set_len(file_size)?;
        }
        
        Ok(Self {
            part_file,
            part_file_path,
            final_file_path,
            file_size,
            received_chunks: HashSet::new(),
            total_chunks: 0,
            bytes_received: 0,
            end_of_file_received: false,
        })
    }
    
    /// Receive and process a chunk packet
    /// 
    /// # Arguments
    /// * `chunk_data` - Raw FlatBuffer-encoded chunk packet
    /// 
    /// # Returns
    /// The parsed chunk packet view
    pub fn receive_chunk(&mut self, chunk_data: &[u8]) -> Result<ChunkPacketView> {
        // Parse the chunk packet
        let chunk = ChunkPacketParser::parse(chunk_data)?;
        
        // Validate the chunk
        if !chunk.is_valid() {
            return Err(Error::Protocol(format!(
                "Invalid chunk: data size {} != chunk_length {}",
                chunk.data.len(),
                chunk.chunk_length
            )));
        }
        
        // Verify checksum
        chunk.verify_checksum()?;
        
        // Check for duplicate
        if self.received_chunks.contains(&chunk.chunk_id) {
            log::warn!("Duplicate chunk {} received, ignoring", chunk.chunk_id);
            return Ok(chunk);
        }
        
        // Write chunk to file at correct offset
        self.part_file.seek(SeekFrom::Start(chunk.byte_offset))?;
        self.part_file.write_all(&chunk.data)?;
        self.part_file.flush()?;
        
        // Update tracking
        self.received_chunks.insert(chunk.chunk_id);
        self.bytes_received += chunk.data.len() as u64;
        
        if chunk.end_of_file {
            self.end_of_file_received = true;
            self.total_chunks = chunk.chunk_id + 1;
            
            log::info!(
                "Received final chunk {} of {} (EOF)",
                chunk.chunk_id,
                self.total_chunks
            );
        }
        
        log::debug!(
            "Received chunk {}: {} bytes at offset {} (total: {} bytes, {:.1}%)",
            chunk.chunk_id,
            chunk.data.len(),
            chunk.byte_offset,
            self.bytes_received,
            self.progress() * 100.0
        );
        
        Ok(chunk)
    }
    
    /// Check if the transfer is complete
    pub fn is_complete(&self) -> bool {
        if !self.end_of_file_received {
            return false;
        }
        
        // Check if we have all chunks
        self.received_chunks.len() as u64 == self.total_chunks
    }
    
    /// Get the progress as a ratio (0.0 to 1.0)
    pub fn progress(&self) -> f64 {
        if self.file_size == 0 {
            return if self.is_complete() { 1.0 } else { 0.0 };
        }
        
        (self.bytes_received as f64 / self.file_size as f64).min(1.0)
    }
    
    /// Get missing chunk IDs
    pub fn missing_chunks(&self) -> Vec<ChunkId> {
        if !self.end_of_file_received {
            return Vec::new();
        }
        
        (0..self.total_chunks)
            .filter(|id| !self.received_chunks.contains(id))
            .collect()
    }
    
    /// Finalize the transfer
    /// This verifies completeness and atomically renames the .part file
    pub fn finalize(&mut self) -> Result<PathBuf> {
        if !self.is_complete() {
            let missing = self.missing_chunks();
            return Err(Error::Protocol(format!(
                "Transfer incomplete: {} missing chunks: {:?}",
                missing.len(),
                &missing[..missing.len().min(10)]
            )));
        }
        
        // Flush and sync
        self.part_file.flush()?;
        self.part_file.sync_all()?;
        
        // Close the file
        drop(std::mem::replace(
            &mut self.part_file,
            OpenOptions::new().write(true).open("/dev/null")?
        ));
        
        // Atomic rename
        std::fs::rename(&self.part_file_path, &self.final_file_path)?;
        
        log::info!(
            "Transfer finalized: {} ({} bytes, {} chunks)",
            self.final_file_path.display(),
            self.bytes_received,
            self.total_chunks
        );
        
        Ok(self.final_file_path.clone())
    }
    
    /// Get statistics about the transfer
    pub fn stats(&self) -> ReceiverStats {
        ReceiverStats {
            bytes_received: self.bytes_received,
            chunks_received: self.received_chunks.len() as u64,
            total_chunks: self.total_chunks,
            is_complete: self.is_complete(),
            progress: self.progress(),
        }
    }
}

/// Statistics about a file receiving session
#[derive(Debug, Clone)]
pub struct ReceiverStats {
    pub bytes_received: u64,
    pub chunks_received: u64,
    pub total_chunks: u64,
    pub is_complete: bool,
    pub progress: f64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use crate::protocol::chunk::ChunkPacketBuilder;

    #[test]
    fn test_receiver_creation() {
        let temp_dir = TempDir::new().unwrap();
        let receiver = FileReceiver::new(temp_dir.path(), "test.dat", 1024).unwrap();
        
        assert_eq!(receiver.file_size, 1024);
        assert_eq!(receiver.bytes_received, 0);
        assert!(!receiver.is_complete());
    }

    #[test]
    fn test_receive_single_chunk() {
        let temp_dir = TempDir::new().unwrap();
        let mut receiver = FileReceiver::new(temp_dir.path(), "test.dat", 100).unwrap();
        
        // Create a chunk packet
        let mut builder = ChunkPacketBuilder::new();
        let data = b"Hello, World!";
        let checksum = blake3::hash(data);
        
        let packet = builder.build(
            0,
            0,
            data.len() as u32,
            checksum.as_bytes(),
            true,
            data
        ).unwrap();
        
        // Receive the chunk
        let chunk = receiver.receive_chunk(&packet).unwrap();
        assert_eq!(chunk.chunk_id, 0);
        assert_eq!(chunk.data, data);
        assert!(chunk.end_of_file);
        assert!(receiver.is_complete());
    }

    #[test]
    fn test_receive_multiple_chunks() {
        let temp_dir = TempDir::new().unwrap();
        let mut receiver = FileReceiver::new(temp_dir.path(), "test.dat", 200).unwrap();
        
        let mut builder = ChunkPacketBuilder::new();
        
        // Send 2 chunks
        for i in 0..2 {
            let data = format!("Chunk {}", i).into_bytes();
            let checksum = blake3::hash(&data);
            let is_last = i == 1;
            
            let packet = builder.build(
                i,
                i * 100,
                data.len() as u32,
                checksum.as_bytes(),
                is_last,
                &data
            ).unwrap();
            
            receiver.receive_chunk(&packet).unwrap();
        }
        
        assert!(receiver.is_complete());
        assert_eq!(receiver.received_chunks.len(), 2);
    }
}
