// Server-side file sending logic

use super::connection::ServerConnection;
use crate::chunking::FileChunker;
use crate::common::error::Result;
use std::path::Path;

/// Handles sending files to connected clients using chunked transfer
pub struct DataSender {
    total_bytes_sent: u64,
    total_chunks_sent: u64,
}

impl DataSender {
    /// Create a new data sender
    pub fn new() -> Self {
        Self {
            total_bytes_sent: 0,
            total_chunks_sent: 0,
        }
    }

    /// Send a file in chunks over a data stream
    /// 
    /// # Arguments
    /// * `connection` - The server connection to send data on
    /// * `stream_id` - The data stream ID to send chunks on
    /// * `file_path` - Path to the file to send
    /// * `chunk_size` - Optional chunk size (uses DEFAULT_CHUNK_SIZE if None)
    /// 
    /// # Returns
    /// Total number of bytes sent
    pub fn send_file(
        &mut self,
        connection: &mut ServerConnection,
        stream_id: u64,
        file_path: &Path,
        chunk_size: Option<usize>,
    ) -> Result<u64> {
        let mut chunker = FileChunker::new(file_path, chunk_size)?;
        let total_chunks = chunker.total_chunks();
        
        log::info!(
            "Starting file transfer: {} ({} bytes, {} chunks)",
            file_path.display(),
            chunker.file_size(),
            total_chunks
        );

        let mut bytes_sent = 0u64;
        let mut chunk_count = 0u64;

        while let Some(chunk_packet) = chunker.next_chunk()? {
            // Send the chunk packet on the data stream
            // Don't set FIN until the last chunk
            let is_last = chunk_count == total_chunks - 1;
            match connection.stream_send(stream_id, &chunk_packet, is_last) {
                Ok(written) => {
                    bytes_sent += written as u64;
                    chunk_count += 1;
                    
                    log::debug!(
                        "Sent chunk {}/{}: {} bytes (progress: {:.1}%)",
                        chunk_count,
                        total_chunks,
                        written,
                        chunker.progress() * 100.0
                    );
                }
                Err(e) => {
                    log::error!("Failed to send chunk {}: {:?}", chunk_count, e);
                    return Err(e.into());
                }
            }
        }

        self.total_bytes_sent += bytes_sent;
        self.total_chunks_sent += chunk_count;

        log::info!(
            "File transfer complete: {} bytes in {} chunks",
            bytes_sent,
            chunk_count
        );

        Ok(bytes_sent)
    }

    /// Send raw data as a single packet on a stream
    /// 
    /// # Arguments
    /// * `connection` - The server connection to send data on
    /// * `stream_id` - The stream ID to send data on
    /// * `data` - The data to send
    /// * `fin` - Whether this is the final data on the stream
    /// 
    /// # Returns
    /// The number of bytes successfully sent
    pub fn send_data(
        &mut self,
        connection: &mut ServerConnection,
        stream_id: u64,
        data: &[u8],
        fin: bool,
    ) -> Result<usize> {
        match connection.stream_send(stream_id, data, fin) {
            Ok(written) => {
                self.total_bytes_sent += written as u64;
                log::trace!(
                    "Sent {} bytes on stream {} (total: {})",
                    written,
                    stream_id,
                    self.total_bytes_sent
                );
                Ok(written)
            }
            Err(e) => {
                log::error!("Error sending on stream {}: {:?}", stream_id, e);
                Err(e.into())
            }
        }
    }

    /// Get the total number of bytes sent
    pub fn total_bytes_sent(&self) -> u64 {
        self.total_bytes_sent
    }

    /// Get the total number of chunks sent
    pub fn total_chunks_sent(&self) -> u64 {
        self.total_chunks_sent
    }

    /// Reset the counters
    pub fn reset_counters(&mut self) {
        self.total_bytes_sent = 0;
        self.total_chunks_sent = 0;
    }
}

impl Default for DataSender {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_data_sender_creation() {
        let sender = DataSender::new();
        assert_eq!(sender.total_bytes_sent(), 0);
    }

    #[test]
    fn test_counters_reset() {
        let mut sender = DataSender::new();
        sender.total_bytes_sent = 100;
        sender.total_chunks_sent = 10;
        sender.reset_counters();
        assert_eq!(sender.total_bytes_sent(), 0);
        assert_eq!(sender.total_chunks_sent(), 0);
    }
}
