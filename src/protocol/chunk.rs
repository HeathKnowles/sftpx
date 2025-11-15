// Chunk packet structures using Protocol Buffers
use crate::common::error::{Error, Result};
use crate::proto::sftpx::protocol::ChunkPacket;
use crate::chunking::compress::{CompressionType, compress_chunk};
use prost::Message;

/// Builder for creating chunk packets using Protocol Buffers
pub struct ChunkPacketBuilder {
    compression: CompressionType,
}

impl ChunkPacketBuilder {
    /// Create a new chunk packet builder
    pub fn new() -> Self {
        Self {
            compression: CompressionType::None,
        }
    }
    
    /// Create a new chunk packet builder with compression
    pub fn with_compression(compression: CompressionType) -> Self {
        Self { compression }
    }

    /// Create a new chunk packet builder with specified capacity (ignored in protobuf)
    pub fn with_capacity(_capacity: usize) -> Self {
        Self::new()
    }

    /// Build a chunk packet with optional compression
    /// 
    /// # Arguments
    /// * `chunk_id` - Unique chunk identifier (chunk number)
    /// * `byte_offset` - Starting byte offset in the file
    /// * `chunk_length` - Length of the chunk data (original size)
    /// * `checksum` - Blake3 checksum of the ORIGINAL (uncompressed) chunk data
    /// * `end_of_file` - True if this is the last chunk
    /// * `data` - The actual chunk data (will be compressed if compression is enabled)
    pub fn build(
        &mut self,
        chunk_id: u64,
        byte_offset: u64,
        chunk_length: u32,
        checksum: &[u8],
        end_of_file: bool,
        data: &[u8],
    ) -> Result<Vec<u8>> {
        if data.len() != chunk_length as usize {
            return Err(Error::Protocol(format!(
                "Data length {} doesn't match chunk_length {}",
                data.len(),
                chunk_length
            )));
        }

        // Compress data if compression is enabled
        let (final_data, compression_type, original_size) = if self.compression != CompressionType::None {
            let compressed = compress_chunk(data, self.compression)?;
            // Only use compression if it actually reduces size
            if compressed.len() < data.len() {
                (compressed, self.compression.as_u8() as u32, chunk_length)
            } else {
                // Compression didn't help, use original
                (data.to_vec(), CompressionType::None.as_u8() as u32, chunk_length)
            }
        } else {
            (data.to_vec(), CompressionType::None.as_u8() as u32, chunk_length)
        };

        let packet = ChunkPacket {
            chunk_id,
            byte_offset,
            chunk_length: final_data.len() as u32,  // Actual (possibly compressed) size
            checksum: checksum.to_vec(),
            end_of_file,
            data: final_data,
            compression_type,
            original_size,
        };

        let mut buffer = Vec::with_capacity(packet.encoded_len());
        packet.encode(&mut buffer)
            .map_err(|e| Error::SerializationError(format!("Failed to encode chunk packet: {}", e)))?;

        Ok(buffer)
    }
}

impl Default for ChunkPacketBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Parser for reading chunk packets from Protocol Buffers
pub struct ChunkPacketParser;

impl ChunkPacketParser {
    /// Parse a chunk packet from bytes and decompress if necessary
    pub fn parse(data: &[u8]) -> Result<ChunkPacketView> {
        use crate::chunking::compress::{CompressionType, decompress_chunk};
        
        let packet = ChunkPacket::decode(data)
            .map_err(|e| Error::DeserializationError(format!("Failed to decode chunk packet: {}", e)))?;

        // Decompress data if compressed
        let final_data = if packet.compression_type != 0 {
            let compression_type = CompressionType::from_u8(packet.compression_type as u8)
                .ok_or_else(|| Error::Decompression(format!("Unknown compression type: {}", packet.compression_type)))?;
            
            decompress_chunk(&packet.data, packet.original_size as usize, compression_type)?
        } else {
            packet.data
        };

        Ok(ChunkPacketView {
            chunk_id: packet.chunk_id,
            byte_offset: packet.byte_offset,
            chunk_length: final_data.len() as u32,  // Actual decompressed size
            checksum: packet.checksum,
            end_of_file: packet.end_of_file,
            data: final_data,
        })
    }

    /// Verify that the data is a valid chunk packet
    pub fn verify(data: &[u8]) -> Result<()> {
        ChunkPacket::decode(data)
            .map_err(|e| Error::DeserializationError(format!("Invalid chunk packet: {}", e)))?;
        Ok(())
    }
}

/// View of a parsed chunk packet (owned data, decompressed if needed)
#[derive(Debug, Clone)]
pub struct ChunkPacketView {
    pub chunk_id: u64,
    pub byte_offset: u64,
    pub chunk_length: u32,  // Decompressed size
    pub checksum: Vec<u8>,  // Hash of original (decompressed) data
    pub end_of_file: bool,
    pub data: Vec<u8>,      // Decompressed data
}

impl ChunkPacketView {
    /// Verify the checksum of the chunk data
    pub fn verify_checksum(&self) -> Result<()> {
        let computed = blake3::hash(&self.data);
        if computed.as_bytes() != self.checksum.as_slice() {
            return Err(Error::Protocol("Checksum mismatch".to_string()));
        }
        Ok(())
    }

    /// Get the size of the chunk data
    pub fn data_size(&self) -> usize {
        self.data.len()
    }

    /// Check if sizes are consistent
    pub fn is_valid(&self) -> bool {
        self.data.len() == self.chunk_length as usize
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chunk_packet_build_and_parse() {
        let mut builder = ChunkPacketBuilder::new();
        let data = b"Hello, world!";
        let checksum = vec![1, 2, 3, 4, 5, 6, 7, 8];

        let packet = builder
            .build(0, 0, data.len() as u32, &checksum, false, data)
            .unwrap();

        let parsed = ChunkPacketParser::parse(&packet).unwrap();
        assert_eq!(parsed.chunk_id, 0);
        assert_eq!(parsed.byte_offset, 0);
        assert_eq!(parsed.chunk_length, data.len() as u32);
        assert_eq!(parsed.checksum, checksum);
        assert_eq!(parsed.end_of_file, false);
        assert_eq!(parsed.data, data);
        assert!(parsed.is_valid());
    }

    #[test]
    fn test_end_of_file_flag() {
        let mut builder = ChunkPacketBuilder::new();
        let data = b"Last chunk";
        let checksum = vec![1, 2, 3, 4];

        let packet = builder
            .build(99, 1024, data.len() as u32, &checksum, true, data)
            .unwrap();

        let parsed = ChunkPacketParser::parse(&packet).unwrap();
        assert_eq!(parsed.chunk_id, 99);
        assert_eq!(parsed.byte_offset, 1024);
        assert!(parsed.end_of_file);
    }
}
