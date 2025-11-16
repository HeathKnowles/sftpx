// Chunk compression implementations using Zstd only
use crate::common::error::{Error, Result};

/// Compression algorithm type - Zstd only
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompressionType {
    None = 0,
    Zstd = 1,
}

impl CompressionType {
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(CompressionType::None),
            1 => Some(CompressionType::Zstd),
            _ => None,
        }
    }
    
    pub fn as_u8(&self) -> u8 {
        *self as u8
    }
}

/// Trait for chunk compression
pub trait ChunkCompressor: Send + Sync {
    fn compress(&self, data: &[u8]) -> Result<Vec<u8>>;
    fn decompress(&self, data: &[u8], original_size: usize) -> Result<Vec<u8>>;
    fn compression_type(&self) -> CompressionType;
}

/// No compression (passthrough)
pub struct NoneCompressor;

impl ChunkCompressor for NoneCompressor {
    fn compress(&self, data: &[u8]) -> Result<Vec<u8>> {
        Ok(data.to_vec())
    }
    
    fn decompress(&self, data: &[u8], _original_size: usize) -> Result<Vec<u8>> {
        Ok(data.to_vec())
    }
    
    fn compression_type(&self) -> CompressionType {
        CompressionType::None
    }
}
/// Zstd compression - fast and efficient
pub struct ZstdCompressor {
    compression_level: i32,
}

impl ZstdCompressor {
    pub fn new(level: i32) -> Self {
        Self {
            compression_level: level.clamp(1, 22),
        }
    }
}

impl Default for ZstdCompressor {
    fn default() -> Self {
        Self::new(3) // Default to level 3 (balanced speed/compression)
    }
}

impl ChunkCompressor for ZstdCompressor {
    fn compress(&self, data: &[u8]) -> Result<Vec<u8>> {
        zstd::encode_all(data, self.compression_level)
            .map_err(|e| Error::Compression(format!("Zstd compression failed: {}", e)))
    }
    
    fn decompress(&self, data: &[u8], _original_size: usize) -> Result<Vec<u8>> {
        zstd::decode_all(data)
            .map_err(|e| Error::Decompression(format!("Zstd decompression failed: {}", e)))
    }
    
    fn compression_type(&self) -> CompressionType {
        CompressionType::Zstd
    }
}

/// Factory for creating compressors
pub fn create_compressor(compression_type: CompressionType) -> Box<dyn ChunkCompressor> {
    match compression_type {
        CompressionType::None => Box::new(NoneCompressor),
        CompressionType::Zstd => Box::new(ZstdCompressor::default()),
    }
}

/// Helper to compress chunk data
pub fn compress_chunk(data: &[u8], compression_type: CompressionType) -> Result<Vec<u8>> {
    let compressor = create_compressor(compression_type);
    compressor.compress(data)
}

/// Helper to decompress chunk data
pub fn decompress_chunk(data: &[u8], original_size: usize, compression_type: CompressionType) -> Result<Vec<u8>> {
    let compressor = create_compressor(compression_type);
    compressor.decompress(data, original_size)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_none_compression() {
        let data = b"Hello, World!";
        let compressor = NoneCompressor;
        
        let compressed = compressor.compress(data).unwrap();
        assert_eq!(compressed, data);
        
        let decompressed = compressor.decompress(&compressed, data.len()).unwrap();
        assert_eq!(decompressed, data);
    }

    #[test]
    fn test_zstd_compression() {
        let data = b"Hello, World! ".repeat(100);
        let compressor = ZstdCompressor::default();
        
        let compressed = compressor.compress(&data).unwrap();
        assert!(compressed.len() < data.len());
        
        let decompressed = compressor.decompress(&compressed, data.len()).unwrap();
        assert_eq!(decompressed, data);
    }

    #[test]
    fn test_zstd_levels() {
        let data = b"Hello, World! ".repeat(100);
        
        // Test different compression levels
        let compressor_fast = ZstdCompressor::new(1);
        let compressor_balanced = ZstdCompressor::new(3);
        let compressor_high = ZstdCompressor::new(10);
        
        let compressed_fast = compressor_fast.compress(&data).unwrap();
        let compressed_balanced = compressor_balanced.compress(&data).unwrap();
        let compressed_high = compressor_high.compress(&data).unwrap();
        
        // Higher levels should compress better
        assert!(compressed_high.len() <= compressed_balanced.len());
        assert!(compressed_balanced.len() <= compressed_fast.len());
        
        // All should decompress correctly
        assert_eq!(compressor_fast.decompress(&compressed_fast, data.len()).unwrap(), data);
        assert_eq!(compressor_balanced.decompress(&compressed_balanced, data.len()).unwrap(), data);
        assert_eq!(compressor_high.decompress(&compressed_high, data.len()).unwrap(), data);
    }

    #[test]
    fn test_compression_type_conversion() {
        assert_eq!(CompressionType::from_u8(0), Some(CompressionType::None));
        assert_eq!(CompressionType::from_u8(1), Some(CompressionType::Zstd));
        assert_eq!(CompressionType::from_u8(99), None);
        
        assert_eq!(CompressionType::None.as_u8(), 0);
        assert_eq!(CompressionType::Zstd.as_u8(), 1);
    }
}
