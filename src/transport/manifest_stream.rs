// Manifest stream implementation

use crate::common::error::{Error, Result};
use crate::protocol::messages::Manifest;
use prost::Message;

/// Maximum manifest size (10 MB - for very large files with many chunks)
const MAX_MANIFEST_SIZE: usize = 10 * 1024 * 1024;

/// Manifest sender for client-side operations
pub struct ManifestSender;

impl ManifestSender {
    /// Create a new manifest sender
    pub fn new() -> Self {
        Self
    }

    /// Send a manifest over a QUIC stream
    /// 
    /// # Arguments
    /// * `manifest` - The manifest to send
    /// * `send_fn` - Function to send data on the stream (stream_id, data, fin)
    /// 
    /// # Returns
    /// * `Ok(bytes_written)` - Number of bytes written
    /// * `Err(Error)` - If sending fails
    pub fn send_manifest<F>(
        &self,
        manifest: &Manifest,
        mut send_fn: F,
    ) -> Result<usize>
    where
        F: FnMut(&[u8], bool) -> Result<usize>,
    {
        // Encode manifest to protobuf bytes
        let encoded = manifest.encode_to_vec();
        
        // Check size limit
        if encoded.len() > MAX_MANIFEST_SIZE {
            return Err(Error::Protocol(format!(
                "Manifest too large: {} bytes (max: {})",
                encoded.len(),
                MAX_MANIFEST_SIZE
            )));
        }

        // Send the manifest with FIN flag set (complete message)
        let bytes_written = send_fn(&encoded, true)?;
        
        if bytes_written != encoded.len() {
            return Err(Error::Io(std::io::Error::new(
                std::io::ErrorKind::WriteZero,
                format!("Partial write: {}/{} bytes", bytes_written, encoded.len()),
            )));
        }

        Ok(bytes_written)
    }

    /// Send manifest using a buffer-based approach for large manifests
    /// Chunks the manifest into smaller writes if needed
    pub fn send_manifest_buffered<F>(
        &self,
        manifest: &Manifest,
        mut send_fn: F,
        buffer_size: usize,
    ) -> Result<usize>
    where
        F: FnMut(&[u8], bool) -> Result<usize>,
    {
        let encoded = manifest.encode_to_vec();
        
        if encoded.len() > MAX_MANIFEST_SIZE {
            return Err(Error::Protocol(format!(
                "Manifest too large: {} bytes",
                encoded.len()
            )));
        }

        let mut total_written = 0;
        let chunks = encoded.chunks(buffer_size);
        let chunk_count = chunks.len();
        
        for (idx, chunk) in chunks.enumerate() {
            let is_last = idx == chunk_count - 1;
            let written = send_fn(chunk, is_last)?;
            total_written += written;
            
            if written != chunk.len() {
                return Err(Error::Io(std::io::Error::new(
                    std::io::ErrorKind::WriteZero,
                    "Partial chunk write",
                )));
            }
        }

        Ok(total_written)
    }
}

impl Default for ManifestSender {
    fn default() -> Self {
        Self::new()
    }
}

/// Manifest receiver for server-side operations
pub struct ManifestReceiver {
    buffer: Vec<u8>,
    complete: bool,
}

impl ManifestReceiver {
    /// Create a new manifest receiver
    pub fn new() -> Self {
        Self {
            buffer: Vec::new(),
            complete: false,
        }
    }

    /// Create with pre-allocated capacity
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            buffer: Vec::with_capacity(capacity),
            complete: false,
        }
    }

    /// Receive data chunk from the stream
    /// 
    /// # Arguments
    /// * `data` - Data chunk received
    /// * `fin` - Whether this is the final chunk
    /// 
    /// # Returns
    /// * `Ok(Some(Manifest))` - If manifest is complete and valid
    /// * `Ok(None)` - If more data is needed
    /// * `Err(Error)` - If receive fails or data is invalid
    pub fn receive_chunk(&mut self, data: &[u8], fin: bool) -> Result<Option<Manifest>> {
        // Check size limit
        if self.buffer.len() + data.len() > MAX_MANIFEST_SIZE {
            return Err(Error::Protocol(format!(
                "Manifest data exceeds maximum size: {} bytes",
                MAX_MANIFEST_SIZE
            )));
        }

        // Append to buffer
        self.buffer.extend_from_slice(data);

        // If this is the final chunk, decode the manifest
        if fin {
            self.complete = true;
            let manifest = Manifest::decode(&self.buffer[..])?;
            Ok(Some(manifest))
        } else {
            Ok(None)
        }
    }

    /// Receive complete manifest from a stream using a recv function
    /// 
    /// # Arguments
    /// * `recv_fn` - Function to receive data from stream, returns (data, fin_flag)
    /// 
    /// # Returns
    /// * `Ok(Manifest)` - Decoded manifest
    /// * `Err(Error)` - If receiving or decoding fails
    pub fn receive_manifest<F>(&mut self, mut recv_fn: F) -> Result<Manifest>
    where
        F: FnMut(&mut [u8]) -> Result<(usize, bool)>,
    {
        let mut temp_buffer = vec![0u8; 8192]; // 8KB chunks
        
        loop {
            let (bytes_read, fin) = recv_fn(&mut temp_buffer)?;
            
            if bytes_read == 0 && !fin {
                return Err(Error::Io(std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    "Stream closed before manifest complete",
                )));
            }

            if let Some(manifest) = self.receive_chunk(&temp_buffer[..bytes_read], fin)? {
                return Ok(manifest);
            }

            if fin {
                // Should have received manifest by now
                return Err(Error::Protocol(
                    "Stream finished but manifest incomplete".to_string()
                ));
            }
        }
    }

    /// Get the current buffer size
    pub fn buffer_size(&self) -> usize {
        self.buffer.len()
    }

    /// Check if manifest receiving is complete
    pub fn is_complete(&self) -> bool {
        self.complete
    }

    /// Clear the receiver state for reuse
    pub fn reset(&mut self) {
        self.buffer.clear();
        self.complete = false;
    }
}

impl Default for ManifestReceiver {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_manifest() -> Manifest {
        Manifest {
            session_id: "test-session-123".to_string(),
            file_name: "test.txt".to_string(),
            file_size: 1024,
            chunk_size: 256,
            total_chunks: 4,
            file_hash: vec![0u8; 32], // BLAKE3 hash
            chunk_hashes: vec![vec![0u8; 32]; 4],
            compression: "none".to_string(),
            original_size: Some(1024),
        }
    }

    #[test]
    fn test_manifest_sender_basic() {
        let sender = ManifestSender::new();
        let manifest = create_test_manifest();
        
        let mut sent_data = Vec::new();
        let mut fin_received = false;
        
        let result = sender.send_manifest(&manifest, |data, fin| {
            sent_data.extend_from_slice(data);
            fin_received = fin;
            Ok(data.len())
        });
        
        assert!(result.is_ok());
        assert!(fin_received);
        assert!(!sent_data.is_empty());
        
        // Verify we can decode what was sent
        let decoded = Manifest::decode(&sent_data[..]).unwrap();
        assert_eq!(decoded.session_id, manifest.session_id);
        assert_eq!(decoded.file_name, manifest.file_name);
    }

    #[test]
    fn test_manifest_receiver_single_chunk() {
        let manifest = create_test_manifest();
        let encoded = manifest.encode_to_vec();
        
        let mut receiver = ManifestReceiver::new();
        let result = receiver.receive_chunk(&encoded, true);
        
        assert!(result.is_ok());
        let received = result.unwrap();
        assert!(received.is_some());
        
        let decoded = received.unwrap();
        assert_eq!(decoded.session_id, manifest.session_id);
        assert_eq!(decoded.file_size, manifest.file_size);
    }

    #[test]
    fn test_manifest_receiver_multiple_chunks() {
        let manifest = create_test_manifest();
        let encoded = manifest.encode_to_vec();
        
        let mut receiver = ManifestReceiver::new();
        
        // Split into 3 chunks
        let chunk_size = encoded.len() / 3;
        let chunk1 = &encoded[0..chunk_size];
        let chunk2 = &encoded[chunk_size..chunk_size * 2];
        let chunk3 = &encoded[chunk_size * 2..];
        
        // Receive first two chunks
        assert!(receiver.receive_chunk(chunk1, false).unwrap().is_none());
        assert!(receiver.receive_chunk(chunk2, false).unwrap().is_none());
        
        // Receive final chunk
        let result = receiver.receive_chunk(chunk3, true).unwrap();
        assert!(result.is_some());
        
        let decoded = result.unwrap();
        assert_eq!(decoded.session_id, manifest.session_id);
    }

    #[test]
    fn test_sender_receiver_roundtrip() {
        let sender = ManifestSender::new();
        let mut receiver = ManifestReceiver::new();
        let original = create_test_manifest();
        
        // Send
        let mut transmitted_data = Vec::new();
        sender.send_manifest(&original, |data, _fin| {
            transmitted_data.extend_from_slice(data);
            Ok(data.len())
        }).unwrap();
        
        // Receive
        let received = receiver.receive_chunk(&transmitted_data, true).unwrap().unwrap();
        
        // Verify
        assert_eq!(received.session_id, original.session_id);
        assert_eq!(received.file_name, original.file_name);
        assert_eq!(received.file_size, original.file_size);
        assert_eq!(received.total_chunks, original.total_chunks);
    }

    #[test]
    fn test_manifest_size_limit() {
        let sender = ManifestSender::new();
        
        // Create a manifest with way too many chunk hashes
        let huge_manifest = Manifest {
            session_id: "test".to_string(),
            file_name: "huge.bin".to_string(),
            file_size: u64::MAX,
            chunk_size: 1024,
            total_chunks: 1_000_000, // 1 million chunks
            file_hash: vec![0u8; 32],
            chunk_hashes: vec![vec![0u8; 32]; 1_000_000], // ~32 MB of hashes
            compression: "none".to_string(),
            original_size: None,
        };
        
        let result = sender.send_manifest(&huge_manifest, |_data, _fin| Ok(0));
        assert!(result.is_err());
    }

    #[test]
    fn test_receiver_reset() {
        let mut receiver = ManifestReceiver::new();
        receiver.receive_chunk(&[1, 2, 3], false).unwrap();
        
        assert_eq!(receiver.buffer_size(), 3);
        assert!(!receiver.is_complete());
        
        receiver.reset();
        
        assert_eq!(receiver.buffer_size(), 0);
        assert!(!receiver.is_complete());
    }
}
