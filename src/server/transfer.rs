// Server-side transfer logic

use super::connection::ServerConnection;
use super::sender::DataSender;
use std::path::Path;

const DEFAULT_CHUNK_SIZE: usize = 8192;

/// Manages file transfers to clients
pub struct TransferManager {
    sender: DataSender,
    chunk_size: usize,
}

impl TransferManager {
    /// Create a new transfer manager
    pub fn new() -> Self {
        Self {
            sender: DataSender::new(),
            chunk_size: DEFAULT_CHUNK_SIZE,
        }
    }

    /// Create a transfer manager with custom chunk size
    pub fn with_chunk_size(chunk_size: usize) -> Self {
        Self {
            sender: DataSender::new(),
            chunk_size,
        }
    }

    /// Transfer a file to a client using chunked protocol
    /// 
    /// # Arguments
    /// * `connection` - The server connection
    /// * `stream_id` - The stream ID to send the file on
    /// * `file_path` - Path to the file to transfer
    /// 
    /// # Returns
    /// The total number of bytes transferred
    pub fn transfer_file(
        &mut self,
        connection: &mut ServerConnection,
        stream_id: u64,
        file_path: &Path,
    ) -> Result<u64, Box<dyn std::error::Error>> {
        println!("TransferManager: starting file transfer from {:?}", file_path);

        let bytes_sent = self.sender.send_file(
            connection,
            stream_id,
            file_path,
            Some(self.chunk_size),
        )?;

        println!("TransferManager: file transfer complete ({} bytes)", bytes_sent);
        
        Ok(bytes_sent)
    }

    /// Transfer raw data to a client on a stream
    /// 
    /// # Arguments
    /// * `connection` - The server connection
    /// * `stream_id` - The stream ID to send data on
    /// * `data` - The data to transfer
    /// 
    /// # Returns
    /// The total number of bytes transferred
    pub fn transfer_data(
        &mut self,
        connection: &mut ServerConnection,
        stream_id: u64,
        data: &[u8],
    ) -> Result<usize, Box<dyn std::error::Error>> {
        println!(
            "TransferManager: transferring {} bytes on stream {}",
            data.len(),
            stream_id
        );

        let sent = self.sender.send_data(connection, stream_id, data, true)?;
        Ok(sent)
    }

    /// Transfer a file on a specific stream
    /// This is an alias for transfer_file for backwards compatibility
    /// 
    /// # Arguments
    /// * `connection` - The server connection
    /// * `stream_id` - The stream ID to send the file on
    /// * `file_path` - Path to the file to transfer
    /// 
    /// # Returns
    /// The total number of bytes transferred
    pub fn transfer_file_on_stream(
        &mut self,
        connection: &mut ServerConnection,
        stream_id: u64,
        file_path: &Path,
    ) -> Result<u64, Box<dyn std::error::Error>> {
        self.transfer_file(connection, stream_id, file_path)
    }

    /// Get the current chunk size
    pub fn chunk_size(&self) -> usize {
        self.chunk_size
    }

    /// Set a new chunk size
    pub fn set_chunk_size(&mut self, chunk_size: usize) {
        self.chunk_size = chunk_size;
    }

    /// Get total bytes sent
    pub fn total_bytes_sent(&self) -> u64 {
        self.sender.total_bytes_sent()
    }

    /// Get total chunks sent
    pub fn total_chunks_sent(&self) -> u64 {
        self.sender.total_chunks_sent()
    }
}

impl Default for TransferManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transfer_manager_creation() {
        let manager = TransferManager::new();
        assert_eq!(manager.chunk_size(), DEFAULT_CHUNK_SIZE);
    }

    #[test]
    fn test_custom_chunk_size() {
        let manager = TransferManager::with_chunk_size(4096);
        assert_eq!(manager.chunk_size(), 4096);
    }

    #[test]
    fn test_set_chunk_size() {
        let mut manager = TransferManager::new();
        manager.set_chunk_size(16384);
        assert_eq!(manager.chunk_size(), 16384);
    }
}
