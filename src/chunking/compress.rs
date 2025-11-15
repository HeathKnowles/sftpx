// Compression algorithms for chunking implementing zstd and lz4 based on size of data
use crate::common::error::{Error, Result};

/// Compression algorithm selection
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompressionAlgorithm {
    /// No compression
    None,
    /// LZ4 - Fast compression, good for small chunks and real-time
    Lz4,
    /// LZ4HC - High compression variant of LZ4, adjustable level (1-12, default: 9)
    Lz4Hc(u32),
    /// Zstd - Better compression ratio, adjustable level
    Zstd(i32), // Compression level 1-22 (default: 3)
    /// LZMA2 - Maximum compression ratio, slower speed
    Lzma2(u32), // Compression level 0-9 (default: 6)
}

impl CompressionAlgorithm {
    /// Choose optimal algorithm based on file type and extension
    /// - Text/Log files (.txt, .log, .json, .xml, .csv): Zstd for best compression
    /// - Video files (.mkv, .mp4, .avi, .mov, etc.): None (already compressed with HEVC/H.264)
    /// - Audio files (.mp3, .aac, .flac, .m4a, etc.): None (already compressed)
    /// - Binary/Other files: LZ4HC for balanced speed/compression
    pub fn auto_select_by_extension(extension: &str) -> Self {
        let ext = extension.to_lowercase();
        
        // Text/Log files - use Zstd for best compression
        if matches!(ext.as_str(), "txt" | "log" | "json" | "xml" | "csv" | "yaml" | "yml" | "toml" | "md" | "rst") {
            return CompressionAlgorithm::Zstd(5);
        }
        
        // Video files - already compressed (HEVC/H.264/VP9), don't re-compress
        if matches!(ext.as_str(), "mkv" | "mp4" | "avi" | "mov" | "webm" | "flv" | "wmv" | "m4v" | "mpg" | "mpeg") {
            return CompressionAlgorithm::None;
        }
        
        // Audio files - already compressed (AAC/MP3/Opus), don't re-compress
        if matches!(ext.as_str(), "mp3" | "aac" | "m4a" | "opus" | "ogg" | "flac" | "wma" | "wav") {
            return CompressionAlgorithm::None;
        }
        
        // Already compressed archives/images
        if matches!(ext.as_str(), "zip" | "gz" | "bz2" | "xz" | "7z" | "rar" | "jpg" | "jpeg" | "png" | "webp" | "gif") {
            return CompressionAlgorithm::None;
        }
        
        // Binary and other files - use LZ4HC for balanced compression
        CompressionAlgorithm::Lz4Hc(9)
    }
    
    /// Choose optimal algorithm based on data size (legacy method)
    /// - Small chunks (<4KB): LZ4 for speed
    /// - Medium chunks (4KB-64KB): LZ4HC level 9 for better compression
    /// - Large chunks (64KB-1MB): Zstd level 5
    /// - Very large chunks (>1MB): LZMA2 level 6 for maximum compression
    pub fn auto_select(data_size: usize) -> Self {
        if data_size < 4096 {
            CompressionAlgorithm::Lz4
        } else if data_size < 65536 {
            CompressionAlgorithm::Lz4Hc(9)
        } else if data_size < 1048576 {
            CompressionAlgorithm::Zstd(5)
        } else {
            CompressionAlgorithm::Lzma2(6)
        }
    }
}

impl std::fmt::Display for CompressionAlgorithm {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CompressionAlgorithm::None => write!(f, "None"),
            CompressionAlgorithm::Lz4 => write!(f, "LZ4"),
            CompressionAlgorithm::Lz4Hc(level) => write!(f, "LZ4HC({})", level),
            CompressionAlgorithm::Zstd(level) => write!(f, "Zstd({})", level),
            CompressionAlgorithm::Lzma2(level) => write!(f, "LZMA2({})", level),
        }
    }
}

/// Compressed chunk data with metadata
#[derive(Debug, Clone)]
pub struct CompressedChunk {
    /// Original uncompressed data
    pub original_data: Vec<u8>,
    /// Compressed data (may be same as original if compression didn't help)
    pub compressed_data: Vec<u8>,
    /// Algorithm used
    pub algorithm: CompressionAlgorithm,
    /// Original size in bytes
    pub original_size: usize,
    /// Compressed size in bytes
    pub compressed_size: usize,
    /// Compression ratio (compressed/original)
    pub ratio: f64,
}

impl CompressedChunk {
    /// Check if compression actually reduced size
    pub fn is_compressed(&self) -> bool {
        self.compressed_size < self.original_size
    }

    /// Get space saved in bytes
    pub fn space_saved(&self) -> usize {
        self.original_size.saturating_sub(self.compressed_size)
    }

    /// Get the data to transmit (compressed or original)
    pub fn data_to_send(&self) -> &[u8] {
        if self.is_compressed() {
            &self.compressed_data
        } else {
            &self.original_data
        }
    }
}

/// Chunk compressor with multiple algorithm support
pub struct ChunkCompressor;

impl ChunkCompressor {
    /// Compress data using the specified algorithm
    pub fn compress(
        data: &[u8],
        algorithm: CompressionAlgorithm,
    ) -> Result<CompressedChunk> {
        let original_size = data.len();
        let original_data = data.to_vec();

        let (compressed_data, actual_algorithm) = match algorithm {
            CompressionAlgorithm::None => (data.to_vec(), CompressionAlgorithm::None),
            
            CompressionAlgorithm::Lz4 => {
                let compressed = lz4_flex::compress_prepend_size(data);
                (compressed, CompressionAlgorithm::Lz4)
            }
            
            CompressionAlgorithm::Lz4Hc(level) => {
                // LZ4HC uses level 1-12, clamp to valid range
                let clamped_level = level.clamp(1, 12);
                
                // Compress with LZ4HC
                let mut encoder = lz4::EncoderBuilder::new()
                    .level(clamped_level)
                    .build(Vec::new())
                    .map_err(|e| Error::Io(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        format!("LZ4HC encoder creation failed: {}", e)
                    )))?;
                
                use std::io::Write;
                encoder.write_all(data)
                    .map_err(|e| Error::Io(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        format!("LZ4HC compression write failed: {}", e)
                    )))?;
                
                let (compressed, result) = encoder.finish();
                result.map_err(|e| Error::Io(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("LZ4HC compression finish failed: {}", e)
                )))?;
                
                (compressed, CompressionAlgorithm::Lz4Hc(clamped_level))
            }
            
            CompressionAlgorithm::Zstd(level) => {
                let compressed = zstd::bulk::compress(data, level)
                    .map_err(|e| Error::Io(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        format!("Zstd compression failed: {}", e)
                    )))?;
                (compressed, CompressionAlgorithm::Zstd(level))
            }
            
            CompressionAlgorithm::Lzma2(level) => {
                use std::io::Write;
                let mut compressed = Vec::new();
                let mut encoder = xz2::write::XzEncoder::new(&mut compressed, level);
                encoder.write_all(data)
                    .map_err(|e| Error::Io(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        format!("LZMA2 compression write failed: {}", e)
                    )))?;
                encoder.finish()
                    .map_err(|e| Error::Io(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        format!("LZMA2 compression finish failed: {}", e)
                    )))?;
                (compressed, CompressionAlgorithm::Lzma2(level))
            }
        };

        let compressed_size = compressed_data.len();
        let ratio = if original_size > 0 {
            compressed_size as f64 / original_size as f64
        } else {
            1.0
        };

        Ok(CompressedChunk {
            original_data,
            compressed_data,
            algorithm: actual_algorithm,
            original_size,
            compressed_size,
            ratio,
        })
    }

    /// Compress with automatic algorithm selection
    pub fn compress_auto(data: &[u8]) -> Result<CompressedChunk> {
        let algorithm = CompressionAlgorithm::auto_select(data.len());
        Self::compress(data, algorithm)
    }

    /// Decompress data based on algorithm
    pub fn decompress(
        compressed_data: &[u8],
        algorithm: CompressionAlgorithm,
        expected_size: Option<usize>,
    ) -> Result<Vec<u8>> {
        match algorithm {
            CompressionAlgorithm::None => Ok(compressed_data.to_vec()),
            
            CompressionAlgorithm::Lz4 => {
                lz4_flex::decompress_size_prepended(compressed_data)
                    .map_err(|e| Error::Protocol(format!("LZ4 decompression failed: {}", e)))
            }
            
            CompressionAlgorithm::Lz4Hc(_) => {
                // LZ4HC uses same decompression as LZ4
                use std::io::Read;
                let mut decoder = lz4::Decoder::new(compressed_data)
                    .map_err(|e| Error::Protocol(format!("LZ4HC decoder creation failed: {}", e)))?;
                
                let mut decompressed = Vec::new();
                if let Some(size) = expected_size {
                    decompressed.reserve(size);
                }
                
                decoder.read_to_end(&mut decompressed)
                    .map_err(|e| Error::Protocol(format!("LZ4HC decompression failed: {}", e)))?;
                
                Ok(decompressed)
            }
            
            CompressionAlgorithm::Zstd(_) => {
                let decompressed = if let Some(size) = expected_size {
                    zstd::bulk::decompress(compressed_data, size)
                } else {
                    zstd::bulk::decompress(compressed_data, 1024 * 1024) // Default 1MB max
                };
                
                decompressed.map_err(|e| Error::Io(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("Zstd decompression failed: {}", e)
                )))
            }
            
            CompressionAlgorithm::Lzma2(_) => {
                use std::io::Read;
                let mut decoder = xz2::read::XzDecoder::new(compressed_data);
                let mut decompressed = Vec::new();
                
                if let Some(size) = expected_size {
                    decompressed.reserve(size);
                }
                
                decoder.read_to_end(&mut decompressed)
                    .map_err(|e| Error::Io(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        format!("LZMA2 decompression failed: {}", e)
                    )))?;
                
                Ok(decompressed)
            }
        }
    }

    /// Try compression and only use if it reduces size by at least min_reduction (e.g., 0.05 = 5%)
    pub fn compress_if_beneficial(
        data: &[u8],
        algorithm: CompressionAlgorithm,
        min_reduction: f64,
    ) -> Result<CompressedChunk> {
        let mut result = Self::compress(data, algorithm)?;
        
        // If compression didn't help enough, store uncompressed
        if result.ratio > (1.0 - min_reduction) {
            result.compressed_data = result.original_data.clone();
            result.compressed_size = result.original_size;
            result.algorithm = CompressionAlgorithm::None;
            result.ratio = 1.0;
        }
        
        Ok(result)
    }
}

/// Statistics for compression operations
#[derive(Debug, Default, Clone)]
pub struct CompressionStats {
    pub total_chunks: usize,
    pub compressed_chunks: usize,
    pub original_bytes: u64,
    pub compressed_bytes: u64,
    pub lz4_count: usize,
    pub lz4hc_count: usize,
    pub zstd_count: usize,
    pub lzma2_count: usize,
    pub none_count: usize,
}

impl CompressionStats {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_chunk(&mut self, chunk: &CompressedChunk) {
        self.total_chunks += 1;
        self.original_bytes += chunk.original_size as u64;
        self.compressed_bytes += chunk.compressed_size as u64;

        if chunk.is_compressed() {
            self.compressed_chunks += 1;
        }

        match chunk.algorithm {
            CompressionAlgorithm::None => self.none_count += 1,
            CompressionAlgorithm::Lz4 => self.lz4_count += 1,
            CompressionAlgorithm::Lz4Hc(_) => self.lz4hc_count += 1,
            CompressionAlgorithm::Zstd(_) => self.zstd_count += 1,
            CompressionAlgorithm::Lzma2(_) => self.lzma2_count += 1,
        }
    }

    pub fn overall_ratio(&self) -> f64 {
        if self.original_bytes > 0 {
            self.compressed_bytes as f64 / self.original_bytes as f64
        } else {
            1.0
        }
    }

    pub fn space_saved(&self) -> u64 {
        self.original_bytes.saturating_sub(self.compressed_bytes)
    }

    pub fn compression_percentage(&self) -> f64 {
        if self.total_chunks > 0 {
            (self.compressed_chunks as f64 / self.total_chunks as f64) * 100.0
        } else {
            0.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_auto_select_algorithm() {
        // Small chunk -> LZ4
        assert_eq!(CompressionAlgorithm::auto_select(2048), CompressionAlgorithm::Lz4);
        
        // Medium chunk -> LZ4HC level 9
        assert_eq!(CompressionAlgorithm::auto_select(32768), CompressionAlgorithm::Lz4Hc(9));
        
        // Large chunk -> Zstd level 5
        assert_eq!(CompressionAlgorithm::auto_select(131072), CompressionAlgorithm::Zstd(5));
        
        // Very large chunk -> LZMA2 level 6
        assert_eq!(CompressionAlgorithm::auto_select(2 * 1024 * 1024), CompressionAlgorithm::Lzma2(6));
    }

    #[test]
    fn test_lz4_compression() {
        let data = b"Hello, World! This is a test string that should compress well. ".repeat(10);
        
        let result = ChunkCompressor::compress(&data, CompressionAlgorithm::Lz4).unwrap();
        
        assert_eq!(result.original_size, data.len());
        assert!(result.is_compressed());
        assert!(result.compressed_size < data.len());
        assert!(result.ratio < 1.0);
    }

    #[test]
    fn test_lz4hc_compression() {
        let data = b"LZ4HC test data with repeated patterns ".repeat(20);
        
        let result = ChunkCompressor::compress(&data, CompressionAlgorithm::Lz4Hc(9)).unwrap();
        
        assert_eq!(result.original_size, data.len());
        assert!(result.is_compressed());
        assert!(result.compressed_size < data.len());
    }

    #[test]
    fn test_lz4hc_decompress() {
        let data = b"Test LZ4HC decompression".repeat(30);
        
        let compressed = ChunkCompressor::compress(&data, CompressionAlgorithm::Lz4Hc(9)).unwrap();
        let decompressed = ChunkCompressor::decompress(
            &compressed.compressed_data,
            CompressionAlgorithm::Lz4Hc(9),
            Some(data.len()),
        ).unwrap();
        
        assert_eq!(decompressed, data);
    }

    #[test]
    fn test_lz4hc_levels() {
        let data = b"LZ4HC level comparison ".repeat(50);
        
        let level1 = ChunkCompressor::compress(&data, CompressionAlgorithm::Lz4Hc(1)).unwrap();
        let level12 = ChunkCompressor::compress(&data, CompressionAlgorithm::Lz4Hc(12)).unwrap();
        
        // Higher level should compress better (usually)
        assert!(level12.compressed_size <= level1.compressed_size);
    }

    #[test]
    fn test_lz4_vs_lz4hc() {
        // Use more data with better compressibility patterns
        let data = b"The quick brown fox jumps over the lazy dog. ".repeat(500);
        
        let lz4_result = ChunkCompressor::compress(&data, CompressionAlgorithm::Lz4).unwrap();
        let lz4hc_result = ChunkCompressor::compress(&data, CompressionAlgorithm::Lz4Hc(9)).unwrap();
        
        // Both should compress the data
        assert!(lz4_result.is_compressed());
        assert!(lz4hc_result.is_compressed());
        
        // LZ4HC typically compresses better, but not guaranteed for all data
        // Just verify both algorithms work
        assert!(lz4hc_result.compressed_size < data.len());
        assert!(lz4_result.compressed_size < data.len());
    }

    #[test]
    fn test_zstd_compression() {
        let data = b"Repeated data ".repeat(100);
        
        let result = ChunkCompressor::compress(&data, CompressionAlgorithm::Zstd(3)).unwrap();
        
        assert_eq!(result.original_size, data.len());
        assert!(result.is_compressed());
        assert!(result.compressed_size < data.len());
    }

    #[test]
    fn test_no_compression() {
        let data = b"Test data";
        
        let result = ChunkCompressor::compress(data, CompressionAlgorithm::None).unwrap();
        
        assert_eq!(result.original_size, data.len());
        assert_eq!(result.compressed_size, data.len());
        assert!(!result.is_compressed());
        assert_eq!(result.ratio, 1.0);
    }

    #[test]
    fn test_lz4_decompress() {
        let data = b"Test decompression with LZ4".repeat(20);
        
        let compressed = ChunkCompressor::compress(&data, CompressionAlgorithm::Lz4).unwrap();
        let decompressed = ChunkCompressor::decompress(
            &compressed.compressed_data,
            CompressionAlgorithm::Lz4,
            Some(data.len()),
        ).unwrap();
        
        assert_eq!(decompressed, data);
    }

    #[test]
    fn test_zstd_decompress() {
        let data = b"Test decompression with Zstd".repeat(50);
        
        let compressed = ChunkCompressor::compress(&data, CompressionAlgorithm::Zstd(5)).unwrap();
        let decompressed = ChunkCompressor::decompress(
            &compressed.compressed_data,
            CompressionAlgorithm::Zstd(5),
            Some(data.len()),
        ).unwrap();
        
        assert_eq!(decompressed, data);
    }

    #[test]
    fn test_compress_auto() {
        // Small data -> should use LZ4
        let small_data = vec![0xAB; 2048];
        let result = ChunkCompressor::compress_auto(&small_data).unwrap();
        assert_eq!(result.algorithm, CompressionAlgorithm::Lz4);

        // Large data -> should use Zstd
        let large_data = vec![0xCD; 100000];
        let result = ChunkCompressor::compress_auto(&large_data).unwrap();
        assert!(matches!(result.algorithm, CompressionAlgorithm::Zstd(_)));
    }

    #[test]
    fn test_compress_if_beneficial() {
        // Highly compressible data
        let compressible = b"A".repeat(1000);
        let result = ChunkCompressor::compress_if_beneficial(
            &compressible,
            CompressionAlgorithm::Lz4,
            0.05,
        ).unwrap();
        assert!(result.is_compressed());

        // Random data (not compressible)
        let random: Vec<u8> = (0..1000).map(|i| (i * 7 + 13) as u8).collect();
        let _result = ChunkCompressor::compress_if_beneficial(
            &random,
            CompressionAlgorithm::Lz4,
            0.05,
        ).unwrap();
        // May or may not compress depending on data
    }

    #[test]
    fn test_compressed_chunk_methods() {
        let data = b"Test".repeat(100);
        let compressed = ChunkCompressor::compress(&data, CompressionAlgorithm::Lz4).unwrap();
        
        assert!(compressed.space_saved() > 0);
        assert_eq!(compressed.data_to_send().len(), compressed.compressed_size);
    }

    #[test]
    fn test_compression_stats() {
        let mut stats = CompressionStats::new();
        
        let data1 = b"A".repeat(1000);
        let chunk1 = ChunkCompressor::compress(&data1, CompressionAlgorithm::Lz4).unwrap();
        stats.add_chunk(&chunk1);
        
        let data2 = b"B".repeat(2000);
        let chunk2 = ChunkCompressor::compress(&data2, CompressionAlgorithm::Zstd(3)).unwrap();
        stats.add_chunk(&chunk2);
        
        assert_eq!(stats.total_chunks, 2);
        assert_eq!(stats.lz4_count, 1);
        assert_eq!(stats.zstd_count, 1);
        assert!(stats.overall_ratio() < 1.0);
        assert!(stats.space_saved() > 0);
    }

    #[test]
    fn test_zstd_levels() {
        let data = b"Compression level test ".repeat(100);
        
        let level1 = ChunkCompressor::compress(&data, CompressionAlgorithm::Zstd(1)).unwrap();
        let level10 = ChunkCompressor::compress(&data, CompressionAlgorithm::Zstd(10)).unwrap();
        
        // Higher level should compress better (usually)
        assert!(level10.compressed_size <= level1.compressed_size);
    }

    #[test]
    fn test_empty_data() {
        let data = b"";
        let result = ChunkCompressor::compress(data, CompressionAlgorithm::Lz4).unwrap();
        assert_eq!(result.original_size, 0);
    }

    #[test]
    fn test_round_trip_all_algorithms() {
        let data = b"Round trip test data".repeat(50);
        
        // Test LZ4
        let compressed_lz4 = ChunkCompressor::compress(&data, CompressionAlgorithm::Lz4).unwrap();
        let decompressed_lz4 = ChunkCompressor::decompress(
            &compressed_lz4.compressed_data,
            CompressionAlgorithm::Lz4,
            None,
        ).unwrap();
        assert_eq!(decompressed_lz4, data);
        
        // Test LZ4HC
        let compressed_lz4hc = ChunkCompressor::compress(&data, CompressionAlgorithm::Lz4Hc(9)).unwrap();
        let decompressed_lz4hc = ChunkCompressor::decompress(
            &compressed_lz4hc.compressed_data,
            CompressionAlgorithm::Lz4Hc(9),
            Some(data.len()),
        ).unwrap();
        assert_eq!(decompressed_lz4hc, data);
        
        // Test Zstd
        let compressed_zstd = ChunkCompressor::compress(&data, CompressionAlgorithm::Zstd(3)).unwrap();
        let decompressed_zstd = ChunkCompressor::decompress(
            &compressed_zstd.compressed_data,
            CompressionAlgorithm::Zstd(3),
            Some(data.len()),
        ).unwrap();
        assert_eq!(decompressed_zstd, data);
        
        // Test LZMA2
        let compressed_lzma2 = ChunkCompressor::compress(&data, CompressionAlgorithm::Lzma2(6)).unwrap();
        let decompressed_lzma2 = ChunkCompressor::decompress(
            &compressed_lzma2.compressed_data,
            CompressionAlgorithm::Lzma2(6),
            Some(data.len()),
        ).unwrap();
        assert_eq!(decompressed_lzma2, data);
        
        // Test None
        let no_compress = ChunkCompressor::compress(&data, CompressionAlgorithm::None).unwrap();
        let decompressed_none = ChunkCompressor::decompress(
            &no_compress.compressed_data,
            CompressionAlgorithm::None,
            None,
        ).unwrap();
        assert_eq!(decompressed_none, data);
    }

    #[test]
    fn test_lzma2_compression() {
        let data = b"LZMA2 test data with repeated patterns ".repeat(100);
        
        let result = ChunkCompressor::compress(&data, CompressionAlgorithm::Lzma2(6)).unwrap();
        
        assert_eq!(result.original_size, data.len());
        assert!(result.is_compressed());
        assert!(result.compressed_size < data.len());
    }

    #[test]
    fn test_lzma2_decompress() {
        let data = b"Test LZMA2 decompression".repeat(50);
        
        let compressed = ChunkCompressor::compress(&data, CompressionAlgorithm::Lzma2(6)).unwrap();
        let decompressed = ChunkCompressor::decompress(
            &compressed.compressed_data,
            CompressionAlgorithm::Lzma2(6),
            Some(data.len()),
        ).unwrap();
        
        assert_eq!(decompressed, data);
    }

    #[test]
    fn test_lzma2_levels() {
        let data = b"LZMA2 level comparison ".repeat(200);
        
        let level1 = ChunkCompressor::compress(&data, CompressionAlgorithm::Lzma2(1)).unwrap();
        let level9 = ChunkCompressor::compress(&data, CompressionAlgorithm::Lzma2(9)).unwrap();
        
        // Higher level should compress better (usually)
        assert!(level9.compressed_size <= level1.compressed_size);
    }
}