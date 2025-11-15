// Server-side transfer logic

use super::connection::ServerConnection;
use super::sender::DataSender;
use super::streams::StreamManager;
use std::path::Path;
use std::fs::File;
use std::io::Read;

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

    /// Transfer a file to a client
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
    ) -> Result<usize, Box<dyn std::error::Error>> {
        let mut file = File::open(file_path)?;
        let mut buffer = vec![0u8; self.chunk_size];
        let mut total_sent = 0;

        println!("TransferManager: starting file transfer from {:?}", file_path);

        loop {
            match file.read(&mut buffer) {
                Ok(0) => {
                    // End of file - send with FIN flag
                    println!("TransferManager: file transfer complete ({} bytes)", total_sent);
                    break;
                }
                Ok(n) => {
                    let sent = self.sender.send_data(
                        connection,
                        stream_id,
                        &buffer[..n],
                        false,
                    )?;
                    total_sent += sent;
                }
                Err(e) => {
                    eprintln!("TransferManager: error reading file: {:?}", e);
                    return Err(Box::new(e));
                }
            }
        }

        // Send FIN
        self.sender.send_data(connection, stream_id, &[], true)?;

        Ok(total_sent)
    }

    /// Transfer data to a client using multiple streams
    /// 
    /// # Arguments
    /// * `connection` - The server connection
    /// * `stream_manager` - The stream manager
    /// * `data` - The data to transfer
    /// 
    /// # Returns
    /// The total number of bytes transferred
    pub fn transfer_data_multistream(
        &mut self,
        connection: &mut ServerConnection,
        stream_manager: &StreamManager,
        data: &[u8],
    ) -> Result<usize, Box<dyn std::error::Error>> {
        let stream_ids: Vec<u64> = stream_manager
            .active_streams()
            .iter()
            .map(|s| s.stream_id)
            .collect();

        if stream_ids.is_empty() {
            return Err("No active streams available".into());
        }

        println!(
            "TransferManager: transferring {} bytes across {} streams",
            data.len(),
            stream_ids.len()
        );

        self.sender.send_distributed(
            connection,
            &stream_ids,
            data,
            self.chunk_size,
        )
    }

    /// Transfer a file using multiple streams for parallel transfer
    /// 
    /// # Arguments
    /// * `connection` - The server connection
    /// * `stream_manager` - The stream manager
    /// * `file_path` - Path to the file to transfer
    /// 
    /// # Returns
    /// The total number of bytes transferred
    pub fn transfer_file_multistream(
        &mut self,
        connection: &mut ServerConnection,
        stream_manager: &StreamManager,
        file_path: &Path,
    ) -> Result<usize, Box<dyn std::error::Error>> {
        let mut file = File::open(file_path)?;
        let mut data = Vec::new();
        file.read_to_end(&mut data)?;

        println!(
            "TransferManager: loaded {} bytes from {:?}",
            data.len(),
            file_path
        );

        self.transfer_data_multistream(connection, stream_manager, &data)
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
    pub fn total_bytes_sent(&self) -> usize {
        self.sender.total_bytes_sent()
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
