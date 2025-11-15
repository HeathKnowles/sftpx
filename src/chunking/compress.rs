// Chunk compression implementations
use crate::common::error::{Error, Result};

/// Compression algorithm type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompressionType {
    None = 0,
    Lz4 = 1,
    Lz4Hc = 2,    // High compression variant
    Zstd = 3,
}

impl CompressionType {
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(CompressionType::None),
            1 => Some(CompressionType::Lz4),
            2 => Some(CompressionType::Lz4Hc),
            3 => Some(CompressionType::Zstd),
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

/// LZ4 fast compression
pub struct Lz4Compressor;

impl ChunkCompressor for Lz4Compressor {
    fn compress(&self, data: &[u8]) -> Result<Vec<u8>> {
        lz4::block::compress(data, None, false)
            .map_err(|e| Error::Compression(format!("LZ4 compression failed: {}", e)))
    }
    
    fn decompress(&self, data: &[u8], original_size: usize) -> Result<Vec<u8>> {
        lz4::block::decompress(data, Some(original_size as i32))
            .map_err(|e| Error::Decompression(format!("LZ4 decompression failed: {}", e)))
    }
    
    fn compression_type(&self) -> CompressionType {
        CompressionType::Lz4
    }
}

/// LZ4-HC (high compression) variant
pub struct Lz4HcCompressor {
    compression_level: i32,
}

impl Lz4HcCompressor {
    pub fn new(level: i32) -> Self {
        Self {
            compression_level: level.clamp(1, 12),
        }
    }
}

impl Default for Lz4HcCompressor {
    fn default() -> Self {
        Self::new(9) // Default to level 9
    }
}

impl ChunkCompressor for Lz4HcCompressor {
    fn compress(&self, data: &[u8]) -> Result<Vec<u8>> {
        lz4::block::compress(data, Some(lz4::block::CompressionMode::HIGHCOMPRESSION(self.compression_level)), false)
            .map_err(|e| Error::Compression(format!("LZ4-HC compression failed: {}", e)))
    }
    
    fn decompress(&self, data: &[u8], original_size: usize) -> Result<Vec<u8>> {
        lz4::block::decompress(data, Some(original_size as i32))
            .map_err(|e| Error::Decompression(format!("LZ4-HC decompression failed: {}", e)))
    }
    
    fn compression_type(&self) -> CompressionType {
        CompressionType::Lz4Hc
    }
}

/// Zstd compression
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
        Self::new(3) // Default to level 3 (fast)
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
        CompressionType::Lz4 => Box::new(Lz4Compressor),
        CompressionType::Lz4Hc => Box::new(Lz4HcCompressor::default()),
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
    fn test_lz4_compression() {
        let data = b"Hello, World! ".repeat(100);
        let compressor = Lz4Compressor;
        
        let compressed = compressor.compress(&data).unwrap();
        assert!(compressed.len() < data.len());
        
        let decompressed = compressor.decompress(&compressed, data.len()).unwrap();
        assert_eq!(decompressed, data);
    }

    #[test]
    fn test_lz4hc_compression() {
        let data = b"Hello, World! ".repeat(100);
        let compressor = Lz4HcCompressor::default();
        
        let compressed = compressor.compress(&data).unwrap();
        assert!(compressed.len() < data.len());
        
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
    fn test_compression_type_conversion() {
        assert_eq!(CompressionType::from_u8(0), Some(CompressionType::None));
        assert_eq!(CompressionType::from_u8(1), Some(CompressionType::Lz4));
        assert_eq!(CompressionType::from_u8(2), Some(CompressionType::Lz4Hc));
        assert_eq!(CompressionType::from_u8(3), Some(CompressionType::Zstd));
        assert_eq!(CompressionType::from_u8(99), None);
        
        assert_eq!(CompressionType::None.as_u8(), 0);
        assert_eq!(CompressionType::Lz4.as_u8(), 1);
    }
}
