// Parallel chunk processing for high-performance transfers
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::sync::Arc;
use crossbeam_channel::{bounded, Receiver, Sender};
use rayon::prelude::*;
use crate::common::error::{Error, Result};
use crate::common::types::DEFAULT_CHUNK_SIZE;
use crate::protocol::chunk::ChunkPacketBuilder;
use crate::chunking::compress::CompressionType;

/// Represents a raw chunk read from disk before compression
#[derive(Debug, Clone)]
pub struct RawChunk {
    pub chunk_id: u64,
    pub offset: u64,
    pub data: Vec<u8>,
    pub end_of_file: bool,
}

/// Represents a processed chunk ready to send
#[derive(Debug, Clone)]
pub struct ProcessedChunk {
    pub chunk_id: u64,
    pub packet: Vec<u8>,
    pub hash: Vec<u8>,
    pub end_of_file: bool,
}

/// Parallel file chunker that pre-reads and processes chunks in parallel
pub struct ParallelChunker {
    file_path: std::path::PathBuf,
    file_size: u64,
    chunk_size: usize,
    compression: CompressionType,
    total_chunks: u64,
    worker_threads: usize,
    pipeline_depth: usize,
}

impl ParallelChunker {
    /// Create a new parallel chunker
    pub fn new(
        file_path: &Path,
        chunk_size: Option<usize>,
        compression: CompressionType,
        worker_threads: Option<usize>,
    ) -> Result<Self> {
        let file = File::open(file_path)?;
        let file_size = file.metadata()?.len();
        let chunk_size = chunk_size.unwrap_or(DEFAULT_CHUNK_SIZE);
        let total_chunks = (file_size + chunk_size as u64 - 1) / chunk_size as u64;
        
        // Use number of CPU cores or specified thread count
        let worker_threads = worker_threads.unwrap_or_else(|| {
            num_cpus::get().max(2) // At least 2 threads
        });
        
        // Pipeline depth: how many chunks to buffer ahead
        let pipeline_depth = (worker_threads * 4).max(16).min(64);
        
        Ok(Self {
            file_path: file_path.to_path_buf(),
            file_size,
            chunk_size,
            compression,
            total_chunks,
            worker_threads,
            pipeline_depth,
        })
    }
    
    /// Get total number of chunks
    pub fn total_chunks(&self) -> u64 {
        self.total_chunks
    }
    
    /// Get file size
    pub fn file_size(&self) -> u64 {
        self.file_size
    }
    
    /// Process all chunks in parallel and return an iterator
    pub fn process_chunks(&self) -> Result<ParallelChunkIterator> {
        ParallelChunkIterator::new(
            &self.file_path,
            self.file_size,
            self.chunk_size,
            self.compression,
            self.total_chunks,
            self.pipeline_depth,
        )
    }
    
    /// Process chunks in batches for better cache locality
    pub fn process_batch(&self, start_chunk: u64, batch_size: usize) -> Result<Vec<ProcessedChunk>> {
        let end_chunk = (start_chunk + batch_size as u64).min(self.total_chunks);
        let chunks_to_read: Vec<u64> = (start_chunk..end_chunk).collect();
        
        // Read all chunks in the batch
        let raw_chunks: Vec<RawChunk> = chunks_to_read
            .par_iter()
            .filter_map(|&chunk_id| {
                self.read_chunk(chunk_id).ok()
            })
            .collect();
        
        // Process all chunks in parallel
        let processed: Vec<ProcessedChunk> = raw_chunks
            .into_par_iter()
            .filter_map(|raw| {
                self.process_raw_chunk(raw).ok()
            })
            .collect();
        
        Ok(processed)
    }
    
    /// Read a single chunk from disk
    fn read_chunk(&self, chunk_id: u64) -> Result<RawChunk> {
        let mut file = File::open(&self.file_path)?;
        let offset = chunk_id * self.chunk_size as u64;
        
        if offset >= self.file_size {
            return Err(Error::Protocol(format!("Chunk {} beyond file size", chunk_id)));
        }
        
        file.seek(SeekFrom::Start(offset))?;
        
        let remaining = self.file_size - offset;
        let to_read = std::cmp::min(remaining, self.chunk_size as u64) as usize;
        
        let mut buffer = vec![0u8; to_read];
        file.read_exact(&mut buffer)?;
        
        let end_of_file = offset + to_read as u64 >= self.file_size;
        
        Ok(RawChunk {
            chunk_id,
            offset,
            data: buffer,
            end_of_file,
        })
    }
    
    /// Process a raw chunk (compute hash, compress, build packet)
    fn process_raw_chunk(&self, raw: RawChunk) -> Result<ProcessedChunk> {
        // Compute hash
        let checksum = blake3::hash(&raw.data);
        let hash = checksum.as_bytes().to_vec();
        
        // Build packet with compression
        let mut builder = ChunkPacketBuilder::with_compression(self.compression);
        let packet = builder.build(
            raw.chunk_id,
            raw.offset,
            raw.data.len() as u32,
            &hash,
            raw.end_of_file,
            &raw.data,
        )?;
        
        Ok(ProcessedChunk {
            chunk_id: raw.chunk_id,
            packet,
            hash,
            end_of_file: raw.end_of_file,
        })
    }
}

/// Iterator that produces processed chunks with parallel pipeline
pub struct ParallelChunkIterator {
    receiver: Receiver<Result<ProcessedChunk>>,
    _worker_handle: std::thread::JoinHandle<()>,
}

impl ParallelChunkIterator {
    fn new(
        file_path: &Path,
        file_size: u64,
        chunk_size: usize,
        compression: CompressionType,
        total_chunks: u64,
        pipeline_depth: usize,
    ) -> Result<Self> {
        let (tx, rx) = bounded(pipeline_depth);
        let file_path = file_path.to_path_buf();
        
        // Spawn worker thread that orchestrates the pipeline
        let worker_handle = std::thread::spawn(move || {
            let chunker = ParallelChunker {
                file_path: file_path.clone(),
                file_size,
                chunk_size,
                compression,
                total_chunks,
                worker_threads: num_cpus::get(),
                pipeline_depth,
            };
            
            // Process chunks in batches for better performance
            let batch_size = 8; // Process 8 chunks at a time
            let mut current_chunk = 0;
            
            while current_chunk < total_chunks {
                let batch_end = (current_chunk + batch_size as u64).min(total_chunks);
                
                // Process batch in parallel
                match chunker.process_batch(current_chunk, batch_size) {
                    Ok(mut chunks) => {
                        // Sort by chunk_id to maintain order
                        chunks.sort_by_key(|c| c.chunk_id);
                        
                        // Send chunks in order
                        for chunk in chunks {
                            if tx.send(Ok(chunk)).is_err() {
                                return; // Receiver dropped
                            }
                        }
                    }
                    Err(e) => {
                        let _ = tx.send(Err(e));
                        return;
                    }
                }
                
                current_chunk = batch_end;
            }
        });
        
        Ok(Self {
            receiver: rx,
            _worker_handle: worker_handle,
        })
    }
}

impl Iterator for ParallelChunkIterator {
    type Item = Result<ProcessedChunk>;
    
    fn next(&mut self) -> Option<Self::Item> {
        self.receiver.recv().ok()
    }
}

/// Pre-compute all chunk hashes in parallel (for manifest generation)
pub fn compute_chunk_hashes_parallel(
    file_path: &Path,
    chunk_size: usize,
) -> Result<Vec<Vec<u8>>> {
    let file = File::open(file_path)?;
    let file_size = file.metadata()?.len();
    let total_chunks = (file_size + chunk_size as u64 - 1) / chunk_size as u64;
    
    let chunk_ids: Vec<u64> = (0..total_chunks).collect();
    
    // Read and hash all chunks in parallel
    let hashes: Vec<Vec<u8>> = chunk_ids
        .par_iter()
        .filter_map(|&chunk_id| {
            let mut file = File::open(file_path).ok()?;
            let offset = chunk_id * chunk_size as u64;
            file.seek(SeekFrom::Start(offset)).ok()?;
            
            let remaining = file_size - offset;
            let to_read = std::cmp::min(remaining, chunk_size as u64) as usize;
            
            let mut buffer = vec![0u8; to_read];
            file.read_exact(&mut buffer).ok()?;
            
            let checksum = blake3::hash(&buffer);
            Some(checksum.as_bytes().to_vec())
        })
        .collect();
    
    if hashes.len() != total_chunks as usize {
        return Err(Error::Protocol("Failed to hash all chunks".into()));
    }
    
    Ok(hashes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;
    
    #[test]
    fn test_parallel_chunker() {
        // Create test file
        let mut temp_file = NamedTempFile::new().unwrap();
        let data = vec![0u8; 5 * 1024 * 1024]; // 5MB
        temp_file.write_all(&data).unwrap();
        temp_file.flush().unwrap();
        
        let chunker = ParallelChunker::new(
            temp_file.path(),
            Some(1024 * 1024), // 1MB chunks
            CompressionType::None,
            Some(4),
        ).unwrap();
        
        assert_eq!(chunker.total_chunks(), 5);
        
        let mut iter = chunker.process_chunks().unwrap();
        let mut count = 0;
        while let Some(result) = iter.next() {
            assert!(result.is_ok());
            count += 1;
        }
        assert_eq!(count, 5);
    }
    
    #[test]
    fn test_parallel_hash_computation() {
        let mut temp_file = NamedTempFile::new().unwrap();
        let data = vec![42u8; 3 * 1024 * 1024]; // 3MB
        temp_file.write_all(&data).unwrap();
        temp_file.flush().unwrap();
        
        let hashes = compute_chunk_hashes_parallel(
            temp_file.path(),
            1024 * 1024, // 1MB chunks
        ).unwrap();
        
        assert_eq!(hashes.len(), 3);
        assert_eq!(hashes[0].len(), 32); // Blake3 hash size
    }
}
