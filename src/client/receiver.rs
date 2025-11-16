// Client-side file receiving logic

use std::path::{Path, PathBuf};
use std::fs::{File, OpenOptions};
use std::io::{Write, Seek, SeekFrom};
use std::collections::HashSet;
use crate::common::error::{Error, Result};
use crate::common::types::ChunkId;
use crate::protocol::chunk::{ChunkPacketParser, ChunkPacketView};
use crate::retransmission::missing::MissingChunkTracker;
use crate::protocol::control::ControlMessage;

/// Synchronization mode for chunk writes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncMode {
    /// Only flush to OS buffers (fastest, least durable)
    FlushOnly,
    /// Full fsync after each chunk (slowest, most durable)
    SyncAll,
    /// Sync every N chunks (balanced)
    SyncEvery(u64),
    /// Buffer everything in RAM, write once at end (fastest!)
    BufferedInMemory,
}

impl Default for SyncMode {
    fn default() -> Self {
        SyncMode::BufferedInMemory  // Changed to in-memory buffering by default
    }
}

/// Callback type for sending control messages
pub type ControlMessageSender = Box<dyn Fn(ControlMessage) -> Result<()> + Send>;

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
    sync_mode: SyncMode,
    finalized: bool,
    expected_file_hash: Option<Vec<u8>>,
    /// Session ID for control messages
    session_id: String,
    /// Missing chunk tracker for automatic retransmission
    missing_tracker: Option<MissingChunkTracker>,
    /// Control message sender (optional, for automatic re-request)
    control_sender: Option<ControlMessageSender>,
    /// Enable automatic retransmission on corruption
    auto_retransmit: bool,
    /// In-memory buffer for BufferedInMemory mode
    memory_buffer: Option<Vec<u8>>,
}

impl FileReceiver {
    /// Create a new file receiver with default sync mode (BufferedInMemory)
    /// 
    /// # Arguments
    /// * `output_dir` - Directory to save the file to
    /// * `filename` - Name of the output file
    /// * `file_size` - Expected total file size in bytes
    pub fn new(output_dir: &Path, filename: &str, file_size: u64) -> Result<Self> {
        Self::with_sync_mode(output_dir, filename, file_size, SyncMode::default())
    }
    
    /// Create a new file receiver with specified sync mode
    /// 
    /// # Arguments
    /// * `output_dir` - Directory to save the file to
    /// * `filename` - Name of the output file
    /// * `file_size` - Expected total file size in bytes
    /// * `sync_mode` - Synchronization mode for chunk writes
    pub fn with_sync_mode(
        output_dir: &Path,
        filename: &str,
        file_size: u64,
        sync_mode: SyncMode,
    ) -> Result<Self> {
        let final_file_path = output_dir.join(filename);
        let part_file_path = output_dir.join(format!("{}.part", filename));
        
        // Create or open .part file
        let part_file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(&part_file_path)?;
        
        // Initialize memory buffer for BufferedInMemory mode
        let memory_buffer = if matches!(sync_mode, SyncMode::BufferedInMemory) {
            if file_size > 0 {
                log::info!("Allocating {} MB in-memory buffer for fast reception", file_size / (1024 * 1024));
                Some(vec![0u8; file_size as usize])
            } else {
                Some(Vec::new())
            }
        } else {
            // Pre-allocate disk space for non-buffered modes
            if file_size > 0 {
                part_file.set_len(file_size)?;
            }
            None
        };
        
        Ok(Self {
            part_file,
            part_file_path,
            final_file_path,
            file_size,
            received_chunks: HashSet::new(),
            total_chunks: 0,
            bytes_received: 0,
            end_of_file_received: false,
            sync_mode,
            finalized: false,
            expected_file_hash: None,
            session_id: String::new(),
            missing_tracker: None,
            control_sender: None,
            auto_retransmit: false,
            memory_buffer,
        })
    }
    
    /// Enable automatic retransmission on corruption
    /// 
    /// # Arguments
    /// * `session_id` - Session ID for control messages
    /// * `control_sender` - Callback to send control messages
    pub fn enable_auto_retransmit(
        &mut self,
        session_id: String,
        control_sender: ControlMessageSender,
    ) {
        self.session_id = session_id;
        self.control_sender = Some(control_sender);
        self.auto_retransmit = true;
        
        // Initialize missing tracker if we know total chunks
        if self.total_chunks > 0 && self.missing_tracker.is_none() {
            self.missing_tracker = Some(MissingChunkTracker::new(self.total_chunks));
        }
    }
    
    /// Disable automatic retransmission
    pub fn disable_auto_retransmit(&mut self) {
        self.auto_retransmit = false;
        self.missing_tracker = None;
        self.control_sender = None;
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
        if let Err(e) = chunk.verify_checksum() {
            log::error!("Chunk {} failed checksum verification: {:?}", chunk.chunk_id, e);
            
            // If auto-retransmit is enabled, send NACK and request retransmission
            if self.auto_retransmit {
                if let Some(tracker) = &mut self.missing_tracker {
                    tracker.mark_corrupted(chunk.chunk_id);
                }
                
                if let Some(sender) = &self.control_sender {
                    let nack = ControlMessage::nack(
                        self.session_id.clone(),
                        vec![chunk.chunk_id],
                        Some(format!("Checksum verification failed: {:?}", e)),
                    );
                    
                    if let Err(send_err) = sender(nack) {
                        log::error!("Failed to send NACK for chunk {}: {:?}", chunk.chunk_id, send_err);
                    } else {
                        log::info!("Sent NACK for corrupted chunk {}, will auto-retry", chunk.chunk_id);
                    }
                }
            }
            
            return Err(e);
        }
        
        // Check for duplicate
        if self.received_chunks.contains(&chunk.chunk_id) {
            log::warn!("Duplicate chunk {} received, ignoring", chunk.chunk_id);
            return Ok(chunk);
        }
        
        // Write chunk - either to memory buffer or disk
        match self.sync_mode {
            SyncMode::BufferedInMemory => {
                // Write to in-memory buffer (super fast!)
                if let Some(buffer) = &mut self.memory_buffer {
                    let start = chunk.byte_offset as usize;
                    let end = start + chunk.data.len();
                    
                    // Grow buffer if needed (for dynamic file sizes)
                    if end > buffer.len() {
                        buffer.resize(end, 0);
                    }
                    
                    buffer[start..end].copy_from_slice(&chunk.data);
                }
                // No disk I/O at all!
            }
            _ => {
                // Write to disk for other modes
                self.part_file.seek(SeekFrom::Start(chunk.byte_offset))?;
                self.part_file.write_all(&chunk.data)?;
                
                // Sync based on configured mode
                match self.sync_mode {
                    SyncMode::FlushOnly => {
                        self.part_file.flush()?;
                    }
                    SyncMode::SyncAll => {
                        self.part_file.flush()?;
                        self.part_file.sync_all()?;
                    }
                    SyncMode::SyncEvery(n) => {
                        self.part_file.flush()?;
                        if (chunk.chunk_id + 1) % n == 0 {
                            self.part_file.sync_all()?;
                        }
                    }
                    SyncMode::BufferedInMemory => unreachable!(),
                }
            }
        }
        
        // Update tracking
        self.received_chunks.insert(chunk.chunk_id);
        self.bytes_received += chunk.data.len() as u64;
        
        // Mark as successfully received in tracker
        if let Some(tracker) = &mut self.missing_tracker {
            tracker.mark_received(chunk.chunk_id);
        }
        
        if chunk.end_of_file {
            self.end_of_file_received = true;
            self.total_chunks = chunk.chunk_id + 1;
            
            // Initialize tracker now that we know total chunks
            if self.auto_retransmit && self.missing_tracker.is_none() {
                self.missing_tracker = Some(MissingChunkTracker::new(self.total_chunks));
                // Mark all already-received chunks
                if let Some(tracker) = &mut self.missing_tracker {
                    for &chunk_id in &self.received_chunks {
                        tracker.mark_received(chunk_id);
                    }
                }
            }
            
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
    
    /// Request retransmission of missing chunks (if auto-retransmit is enabled)
    /// 
    /// # Arguments
    /// * `batch_size` - Maximum number of chunks to request at once
    /// 
    /// # Returns
    /// Number of chunks requested, or 0 if auto-retransmit is disabled
    pub fn request_missing_chunks(&mut self, batch_size: usize) -> Result<usize> {
        if !self.auto_retransmit {
            return Ok(0);
        }
        
        let tracker = self.missing_tracker.as_mut().ok_or_else(|| {
            Error::Protocol("Missing chunk tracker not initialized".to_string())
        })?;
        
        let sender = self.control_sender.as_ref().ok_or_else(|| {
            Error::Protocol("Control sender not set".to_string())
        })?;
        
        // Get all missing chunks and mark them as needing retransmission
        let all_missing = tracker.get_missing();
        for chunk_id in &all_missing {
            tracker.mark_corrupted(*chunk_id);
        }
        
        // Get next batch of chunks to retry
        let chunk_ids = tracker.get_next_batch(batch_size);
        
        if chunk_ids.is_empty() {
            return Ok(0);
        }
        
        // Send retransmit request
        let request = ControlMessage::retransmit_request(
            self.session_id.clone(),
            chunk_ids.clone(),
        );
        
        sender(request)?;
        
        log::info!("Requested retransmission of {} chunks", chunk_ids.len());
        Ok(chunk_ids.len())
    }
    
    /// Check if any chunks have failed (exceeded max retries)
    pub fn has_failed_chunks(&self) -> bool {
        if let Some(tracker) = &self.missing_tracker {
            tracker.has_failed()
        } else {
            false
        }
    }
    
    /// Get list of chunks that have exceeded max retries
    pub fn get_failed_chunks(&self) -> Vec<ChunkId> {
        if let Some(tracker) = &self.missing_tracker {
            tracker.get_failed_chunks()
        } else {
            Vec::new()
        }
    }
    
    /// Set the expected file hash for verification
    /// This should be called with the hash from the manifest
    pub fn set_expected_hash(&mut self, hash: Vec<u8>) -> Result<()> {
        if hash.len() != 32 {
            return Err(Error::Protocol(format!(
                "Invalid hash size: {} (expected 32 bytes for BLAKE3)",
                hash.len()
            )));
        }
        self.expected_file_hash = Some(hash);
        Ok(())
    }
    
    /// Verify the complete file hash matches the expected hash
    /// This performs end-to-end integrity verification
    pub fn verify_file_hash(&mut self) -> Result<()> {
        let expected_hash = self.expected_file_hash.as_ref().ok_or_else(|| {
            Error::Protocol("No expected file hash set".to_string())
        })?;
        
        // Compute hash of the complete file
        self.part_file.seek(SeekFrom::Start(0))?;
        
        let mut hasher = blake3::Hasher::new();
        let mut buffer = vec![0u8; 65536]; // 64KB buffer
        
        loop {
            let bytes_read = std::io::Read::read(&mut self.part_file, &mut buffer)?;
            if bytes_read == 0 {
                break;
            }
            hasher.update(&buffer[..bytes_read]);
        }
        
        let computed_hash = hasher.finalize();
        
        if computed_hash.as_bytes() != expected_hash.as_slice() {
            return Err(Error::HashMismatch {
                expected: expected_hash.clone(),
                actual: computed_hash.as_bytes().to_vec(),
            });
        }
        
        log::info!(
            "File hash verification successful: {:02x?}",
            &computed_hash.as_bytes()[..8]
        );
        
        Ok(())
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
        
        // If using in-memory buffer, write it to disk NOW (single write!)
        if matches!(self.sync_mode, SyncMode::BufferedInMemory) {
            if let Some(buffer) = &self.memory_buffer {
                log::info!("Writing {} MB from memory to disk...", buffer.len() / (1024 * 1024));
                let write_start = std::time::Instant::now();
                
                self.part_file.seek(SeekFrom::Start(0))?;
                self.part_file.write_all(buffer)?;
                self.part_file.flush()?;
                self.part_file.sync_all()?;
                
                log::info!("Disk write completed in {:.2}s", write_start.elapsed().as_secs_f64());
            }
        } else {
            // For other modes, just flush what's already on disk
            self.part_file.flush()?;
            self.part_file.sync_all()?;
        }
        
        // Verify file hash if expected hash is set
        if self.expected_file_hash.is_some() {
            self.verify_file_hash()?;
        }
        
        // Close the file
        drop(std::mem::replace(
            &mut self.part_file,
            OpenOptions::new().write(true).open("/dev/null")?
        ));
        
        // Atomic rename
        std::fs::rename(&self.part_file_path, &self.final_file_path)?;
        
        // Mark as finalized so Drop won't clean up
        self.finalized = true;
        
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
    
    /// Abort the transfer and clean up the partial file
    /// This is useful when explicitly canceling a transfer
    pub fn abort(mut self) -> Result<()> {
        self.cleanup()
    }
    
    /// Clean up the partial file (internal method)
    fn cleanup(&mut self) -> Result<()> {
        if self.finalized {
            return Ok(()); // Already finalized, nothing to clean up
        }
        
        // Close the file first
        drop(std::mem::replace(
            &mut self.part_file,
            OpenOptions::new().write(true).open("/dev/null")?
        ));
        
        // Remove the .part file if it exists
        if self.part_file_path.exists() {
            match std::fs::remove_file(&self.part_file_path) {
                Ok(_) => {
                    log::info!("Cleaned up partial file: {}", self.part_file_path.display());
                }
                Err(e) => {
                    log::warn!(
                        "Failed to remove partial file {}: {}",
                        self.part_file_path.display(),
                        e
                    );
                }
            }
        }
        
        self.finalized = true; // Mark as cleaned up
        Ok(())
    }
}

/// Automatically clean up partial file on drop if not finalized
impl Drop for FileReceiver {
    fn drop(&mut self) {
        if !self.finalized {
            log::warn!(
                "FileReceiver dropped without finalization, cleaning up partial file: {}",
                self.part_file_path.display()
            );
            let _ = self.cleanup();
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
        assert_eq!(receiver.sync_mode, SyncMode::FlushOnly);
    }
    
    #[test]
    fn test_receiver_with_sync_mode() {
        let temp_dir = TempDir::new().unwrap();
        let receiver = FileReceiver::with_sync_mode(
            temp_dir.path(),
            "test.dat",
            1024,
            SyncMode::SyncAll
        ).unwrap();
        
        assert_eq!(receiver.sync_mode, SyncMode::SyncAll);
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
    
    #[test]
    fn test_cleanup_on_drop() {
        let temp_dir = TempDir::new().unwrap();
        let part_file_path = temp_dir.path().join("test.dat.part");
        
        {
            let _receiver = FileReceiver::new(temp_dir.path(), "test.dat", 100).unwrap();
            // Receiver dropped without finalization
            assert!(part_file_path.exists());
        }
        
        // .part file should be cleaned up
        assert!(!part_file_path.exists());
    }
    
    #[test]
    fn test_explicit_abort() {
        let temp_dir = TempDir::new().unwrap();
        let part_file_path = temp_dir.path().join("test.dat.part");
        
        let receiver = FileReceiver::new(temp_dir.path(), "test.dat", 100).unwrap();
        assert!(part_file_path.exists());
        
        // Explicitly abort
        receiver.abort().unwrap();
        
        // .part file should be removed
        assert!(!part_file_path.exists());
    }
    
    #[test]
    fn test_no_cleanup_after_finalize() {
        let temp_dir = TempDir::new().unwrap();
        let final_file_path = temp_dir.path().join("test.dat");
        let part_file_path = temp_dir.path().join("test.dat.part");
        
        let mut receiver = FileReceiver::new(temp_dir.path(), "test.dat", 20).unwrap();
        
        // Send a complete chunk
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
        
        receiver.receive_chunk(&packet).unwrap();
        receiver.finalize().unwrap();
        
        // Final file should exist, part file should not
        assert!(final_file_path.exists());
        assert!(!part_file_path.exists());
        
        // Drop should not try to clean up again
        drop(receiver);
        assert!(final_file_path.exists());
    }
    
    #[test]
    fn test_sync_modes() {
        let temp_dir = TempDir::new().unwrap();
        let mut builder = ChunkPacketBuilder::new();
        
        // Test FlushOnly mode
        let mut receiver = FileReceiver::with_sync_mode(
            temp_dir.path(),
            "flush.dat",
            100,
            SyncMode::FlushOnly
        ).unwrap();
        
        let data = b"test";
        let checksum = blake3::hash(data);
        let packet = builder.build(0, 0, data.len() as u32, checksum.as_bytes(), false, data).unwrap();
        receiver.receive_chunk(&packet).unwrap();
        
        // Test SyncAll mode
        let mut receiver = FileReceiver::with_sync_mode(
            temp_dir.path(),
            "sync.dat",
            100,
            SyncMode::SyncAll
        ).unwrap();
        
        receiver.receive_chunk(&packet).unwrap();
        
        // Test SyncEvery mode
        let mut receiver = FileReceiver::with_sync_mode(
            temp_dir.path(),
            "syncevery.dat",
            100,
            SyncMode::SyncEvery(5)
        ).unwrap();
        
        receiver.receive_chunk(&packet).unwrap();
    }
    
    #[test]
    fn test_file_hash_verification_success() {
        let temp_dir = TempDir::new().unwrap();
        let data = b"Test data for hash verification";
        let mut receiver = FileReceiver::new(temp_dir.path(), "test.dat", data.len() as u64).unwrap();
        
        // Send a complete chunk
        let mut builder = ChunkPacketBuilder::new();
        let checksum = blake3::hash(data);
        
        let packet = builder.build(
            0,
            0,
            data.len() as u32,
            checksum.as_bytes(),
            true,
            data
        ).unwrap();
        
        receiver.receive_chunk(&packet).unwrap();
        
        // Compute expected file hash
        let file_hash = blake3::hash(data);
        receiver.set_expected_hash(file_hash.as_bytes().to_vec()).unwrap();
        
        // Finalize should succeed with correct hash
        let result = receiver.finalize();
        assert!(result.is_ok());
    }
    
    #[test]
    fn test_file_hash_verification_mismatch() {
        let temp_dir = TempDir::new().unwrap();
        let mut receiver = FileReceiver::new(temp_dir.path(), "test.dat", 50).unwrap();
        
        // Send a complete chunk
        let mut builder = ChunkPacketBuilder::new();
        let data = b"Test data";
        let checksum = blake3::hash(data);
        
        let packet = builder.build(
            0,
            0,
            data.len() as u32,
            checksum.as_bytes(),
            true,
            data
        ).unwrap();
        
        receiver.receive_chunk(&packet).unwrap();
        
        // Set wrong expected hash
        let wrong_hash = blake3::hash(b"Wrong data");
        receiver.set_expected_hash(wrong_hash.as_bytes().to_vec()).unwrap();
        
        // Finalize should fail with hash mismatch
        let result = receiver.finalize();
        assert!(result.is_err());
        match result {
            Err(Error::HashMismatch { .. }) => (),
            _ => panic!("Expected HashMismatch error"),
        }
    }
    
    #[test]
    fn test_set_expected_hash_invalid_size() {
        let temp_dir = TempDir::new().unwrap();
        let mut receiver = FileReceiver::new(temp_dir.path(), "test.dat", 100).unwrap();
        
        let invalid_hash = vec![0u8; 16]; // Wrong size
        let result = receiver.set_expected_hash(invalid_hash);
        assert!(result.is_err());
    }
    
    #[test]
    fn test_finalize_without_hash_verification() {
        let temp_dir = TempDir::new().unwrap();
        let mut receiver = FileReceiver::new(temp_dir.path(), "test.dat", 20).unwrap();
        
        // Send complete chunk without setting expected hash
        let mut builder = ChunkPacketBuilder::new();
        let data = b"No hash check";
        let checksum = blake3::hash(data);
        
        let packet = builder.build(
            0,
            0,
            data.len() as u32,
            checksum.as_bytes(),
            true,
            data
        ).unwrap();
        
        receiver.receive_chunk(&packet).unwrap();
        
        // Should succeed even without hash verification
        let result = receiver.finalize();
        assert!(result.is_ok());
    }
    
    #[test]
    fn test_auto_retransmit_on_corruption() {
        use std::sync::{Arc, Mutex};
        
        let temp_dir = tempfile::tempdir().unwrap();
        let mut receiver = FileReceiver::new(
            temp_dir.path(),
            "test.dat",
            100,
        ).unwrap();
        
        // Track sent control messages
        let sent_messages = Arc::new(Mutex::new(Vec::new()));
        let sent_clone = sent_messages.clone();
        
        // Enable auto-retransmit
        receiver.enable_auto_retransmit(
            "test-session".to_string(),
            Box::new(move |msg| {
                sent_clone.lock().unwrap().push(msg);
                Ok(())
            }),
        );
        
        // Create a chunk with valid checksum
        let data = vec![1, 2, 3, 4, 5];
        let checksum = blake3::hash(&data);
        let mut builder = ChunkPacketBuilder::new();
        let chunk_bytes = builder.build(
            0,
            0,
            data.len() as u32,
            checksum.as_bytes(),
            false,
            &data,
        ).unwrap();
        
        // Corrupt the checksum by modifying the encoded bytes
        let mut corrupted_bytes = chunk_bytes.clone();
        if corrupted_bytes.len() > 10 {
            corrupted_bytes[10] ^= 0xFF;
        }
        
        // Receiving corrupted chunk should fail and send NACK
        let result = receiver.receive_chunk(&corrupted_bytes);
        assert!(result.is_err());
        
        // Check that NACK was sent
        let messages = sent_messages.lock().unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].get_type(), crate::protocol::control::ControlMessageType::Nack);
        assert_eq!(messages[0].chunk_ids, vec![0]);
    }
    
    #[test]
    fn test_request_missing_chunks() {
        use std::sync::{Arc, Mutex};
        
        let temp_dir = tempfile::tempdir().unwrap();
        let mut receiver = FileReceiver::new(
            temp_dir.path(),
            "test.dat",
            150,
        ).unwrap();
        
        // Track sent control messages
        let sent_messages = Arc::new(Mutex::new(Vec::new()));
        let sent_clone = sent_messages.clone();
        
        // Enable auto-retransmit
        receiver.enable_auto_retransmit(
            "test-session".to_string(),
            Box::new(move |msg| {
                sent_clone.lock().unwrap().push(msg);
                Ok(())
            }),
        );
        
        // Receive chunk 0
        let data0 = vec![1; 50];
        let checksum0 = blake3::hash(&data0);
        let mut builder0 = ChunkPacketBuilder::new();
        let chunk0_bytes = builder0.build(
            0,
            0,
            data0.len() as u32,
            checksum0.as_bytes(),
            false,
            &data0,
        ).unwrap();
        receiver.receive_chunk(&chunk0_bytes).unwrap();
        
        // Receive chunk 2 (skip chunk 1)
        let data2 = vec![3; 50];
        let checksum2 = blake3::hash(&data2);
        let mut builder2 = ChunkPacketBuilder::new();
        let chunk2_bytes = builder2.build(
            2,
            100,
            data2.len() as u32,
            checksum2.as_bytes(),
            true,
            &data2,
        ).unwrap();
        receiver.receive_chunk(&chunk2_bytes).unwrap();
        
        // Clear previous messages
        sent_messages.lock().unwrap().clear();
        
        // Request missing chunks (chunk 1)
        let count = receiver.request_missing_chunks(10).unwrap();
        assert_eq!(count, 1);
        
        // Check that retransmit request was sent
        let messages = sent_messages.lock().unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].get_type(), crate::protocol::control::ControlMessageType::RetransmitRequest);
        assert_eq!(messages[0].chunk_ids, vec![1]);
    }
    
    #[test]
    fn test_disable_auto_retransmit() {
        let temp_dir = tempfile::tempdir().unwrap();
        let mut receiver = FileReceiver::new(
            temp_dir.path(),
            "test.dat",
            100,
        ).unwrap();
        
        // Enable then disable
        receiver.enable_auto_retransmit(
            "test-session".to_string(),
            Box::new(|_| Ok(())),
        );
        assert!(receiver.auto_retransmit);
        
        receiver.disable_auto_retransmit();
        assert!(!receiver.auto_retransmit);
        assert!(receiver.missing_tracker.is_none());
        assert!(receiver.control_sender.is_none());
    }
}
