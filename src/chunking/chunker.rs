// Chunking algorithm implementation
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use crate::common::error::{Error, Result};
use crate::common::types::DEFAULT_CHUNK_SIZE;
use crate::protocol::chunk::ChunkPacketBuilder;
use crate::chunking::compress::CompressionType;

/// File chunker that splits files into fixed-size chunks with metadata
pub struct FileChunker {
    file: File,
    file_size: u64,
    chunk_size: usize,
    current_chunk: u64,
    bytes_read: u64,
    builder: ChunkPacketBuilder,
}

impl FileChunker {
    /// Create a new file chunker
    /// 
    /// # Arguments
    /// * `file_path` - Path to the file to chunk
    /// * `chunk_size` - Size of each chunk in bytes (optional, defaults to DEFAULT_CHUNK_SIZE)
    pub fn new(file_path: &Path, chunk_size: Option<usize>) -> Result<Self> {
        Self::with_compression(file_path, chunk_size, CompressionType::None)
    }
    
    /// Create a new file chunker with compression
    /// 
    /// # Arguments
    /// * `file_path` - Path to the file to chunk
    /// * `chunk_size` - Size of each chunk in bytes (optional, defaults to DEFAULT_CHUNK_SIZE)
    /// * `compression` - Compression type to use
    pub fn with_compression(
        file_path: &Path,
        chunk_size: Option<usize>,
        compression: CompressionType,
    ) -> Result<Self> {
        let mut file = File::open(file_path)?;
        let file_size = file.metadata()?.len();
        let chunk_size = chunk_size.unwrap_or(DEFAULT_CHUNK_SIZE);

        // Seek to beginning
        file.seek(SeekFrom::Start(0))?;

        Ok(Self {
            file,
            file_size,
            chunk_size,
            current_chunk: 0,
            bytes_read: 0,
            builder: ChunkPacketBuilder::with_compression(compression),
        })
    }

    /// Get the total number of chunks for this file
    pub fn total_chunks(&self) -> u64 {
        (self.file_size + self.chunk_size as u64 - 1) / self.chunk_size as u64
    }

    /// Get the file size
    pub fn file_size(&self) -> u64 {
        self.file_size
    }

    /// Get the current chunk number
    pub fn current_chunk(&self) -> u64 {
        self.current_chunk
    }

    /// Get the number of bytes read so far
    pub fn bytes_read(&self) -> u64 {
        self.bytes_read
    }

    /// Calculate progress as a percentage (0.0 to 1.0)
    pub fn progress(&self) -> f64 {
        if self.file_size == 0 {
            return 1.0;
        }
        self.bytes_read as f64 / self.file_size as f64
    }

    /// Read and create the next chunk packet
    /// Returns None when all chunks have been read
    pub fn next_chunk(&mut self) -> Result<Option<Vec<u8>>> {
        if self.bytes_read >= self.file_size {
            return Ok(None);
        }

        // Calculate how much to read
        let remaining = self.file_size - self.bytes_read;
        let to_read = std::cmp::min(remaining, self.chunk_size as u64) as usize;

        // Read data
        let mut buffer = vec![0u8; to_read];
        self.file.read_exact(&mut buffer)?;

        // Calculate checksum
        let checksum = blake3::hash(&buffer);
        let checksum_bytes = checksum.as_bytes().to_vec();

        // Check if this is the last chunk
        let end_of_file = self.bytes_read + to_read as u64 >= self.file_size;

        // Build the chunk packet
        let packet = self.builder.build(
            self.current_chunk,
            self.bytes_read,
            to_read as u32,
            &checksum_bytes,
            end_of_file,
            &buffer,
        )?;

        // Update state
        self.current_chunk += 1;
        self.bytes_read += to_read as u64;

        Ok(Some(packet))
    }

    /// Reset to the beginning of the file
    pub fn reset(&mut self) -> Result<()> {
        self.file.seek(SeekFrom::Start(0))?;
        self.current_chunk = 0;
        self.bytes_read = 0;
        Ok(())
    }

    /// Seek to a specific chunk
    pub fn seek_to_chunk(&mut self, chunk_id: u64) -> Result<()> {
        let offset = chunk_id * self.chunk_size as u64;
        if offset > self.file_size {
            return Err(Error::Protocol(format!("Chunk {} beyond file size", chunk_id)));
        }

        self.file.seek(SeekFrom::Start(offset))?;
        self.current_chunk = chunk_id;
        self.bytes_read = offset;
        Ok(())
    }

    /// Create an iterator over all chunks
    pub fn iter(&mut self) -> ChunkIterator<'_> {
        ChunkIterator { chunker: self }
    }
}

/// Iterator over file chunks
pub struct ChunkIterator<'a> {
    chunker: &'a mut FileChunker,
}

impl<'a> Iterator for ChunkIterator<'a> {
    type Item = Result<Vec<u8>>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.chunker.next_chunk() {
            Ok(Some(chunk)) => Some(Ok(chunk)),
            Ok(None) => None,
            Err(e) => Some(Err(e)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn create_test_file(size: usize) -> NamedTempFile {
        let mut file = NamedTempFile::new().unwrap();
        let data = vec![0xAB; size];
        file.write_all(&data).unwrap();
        file.flush().unwrap();
        file
    }

    #[test]
    fn test_chunker_small_file() {
        let test_file = create_test_file(1024);
        let mut chunker = FileChunker::new(test_file.path(), Some(512)).unwrap();

        assert_eq!(chunker.total_chunks(), 2);
        assert_eq!(chunker.file_size(), 1024);

        // First chunk
        let _chunk1 = chunker.next_chunk().unwrap().unwrap();
        assert_eq!(chunker.current_chunk(), 1);
        assert_eq!(chunker.bytes_read(), 512);

        // Second chunk
        let _chunk2 = chunker.next_chunk().unwrap().unwrap();
        assert_eq!(chunker.current_chunk(), 2);
        assert_eq!(chunker.bytes_read(), 1024);

        // No more chunks
        assert!(chunker.next_chunk().unwrap().is_none());
    }

    #[test]
    fn test_chunker_exact_chunks() {
        let test_file = create_test_file(2048);
        let mut chunker = FileChunker::new(test_file.path(), Some(512)).unwrap();

        assert_eq!(chunker.total_chunks(), 4);

        let mut count = 0;
        while chunker.next_chunk().unwrap().is_some() {
            count += 1;
        }
        assert_eq!(count, 4);
    }

    #[test]
    fn test_chunker_progress() {
        let test_file = create_test_file(1000);
        let mut chunker = FileChunker::new(test_file.path(), Some(100)).unwrap();

        assert_eq!(chunker.progress(), 0.0);
        
        chunker.next_chunk().unwrap();
        assert_eq!(chunker.progress(), 0.1);
        
        for _ in 0..9 {
            chunker.next_chunk().unwrap();
        }
        assert_eq!(chunker.progress(), 1.0);
    }

    #[test]
    fn test_chunker_reset() {
        let test_file = create_test_file(1000);
        let mut chunker = FileChunker::new(test_file.path(), Some(500)).unwrap();

        chunker.next_chunk().unwrap();
        assert_eq!(chunker.bytes_read(), 500);

        chunker.reset().unwrap();
        assert_eq!(chunker.bytes_read(), 0);
        assert_eq!(chunker.current_chunk(), 0);
    }
}
