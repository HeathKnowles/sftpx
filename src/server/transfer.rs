// Server-side transfer logic

use super::connection::ServerConnection;
use super::sender::DataSender;
use crate::protocol::manifest::ManifestBuilder;
use crate::transport::manifest_stream::ManifestSender;
use std::path::{Path, PathBuf};

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
    
    /// Integrated file send with manifest and chunks
    /// This orchestrates: Manifest build -> Manifest send -> Chunk send
    /// 
    /// # Arguments
    /// * `connection` - The server connection
    /// * `file_path` - Path to the file to transfer
    /// * `session_id` - Session ID for this transfer
    /// * `manifest_stream` - Stream ID for manifest (typically STREAM_MANIFEST = 1)
    /// * `data_stream` - Stream ID for data chunks (typically STREAM_DATA = 2)
    /// 
    /// # Returns
    /// Total bytes sent (manifest + chunks)
    pub fn send_file_integrated(
        &mut self,
        connection: &mut ServerConnection,
        file_path: &Path,
        session_id: String,
        manifest_stream: u64,
        data_stream: u64,
    ) -> Result<u64, Box<dyn std::error::Error>> {
        log::info!("TransferManager: starting integrated file send for {:?}", file_path);
        
        // Build manifest
        log::info!("Building manifest...");
        let manifest = ManifestBuilder::new(session_id)
            .file_path(file_path)
            .chunk_size(self.chunk_size as u32)
            .build()?;
        
        log::info!("Manifest built: {} chunks, {} bytes total", 
            manifest.total_chunks, manifest.file_size);
        
        // Send manifest
        log::info!("Sending manifest on stream {}...", manifest_stream);
        let manifest_sender = ManifestSender::new();
        let manifest_bytes = manifest_sender.send_manifest(&manifest, |data, fin| {
            Ok(connection.stream_send(manifest_stream, data, fin)?)
        })?;
        
        log::info!("Manifest sent: {} bytes", manifest_bytes);
        
        // Send file chunks
        log::info!("Sending file chunks on stream {}...", data_stream);
        let chunks_bytes = self.sender.send_file(
            connection,
            data_stream,
            file_path,
            Some(self.chunk_size),
        )?;
        
        log::info!("File chunks sent: {} bytes", chunks_bytes);
        
        let total_bytes = manifest_bytes as u64 + chunks_bytes;
        log::info!("TransferManager: integrated send complete ({} bytes total)", total_bytes);
        
        Ok(total_bytes)
    }
    
    /// Integrated file receive with manifest and chunks
    /// This orchestrates: Receive manifest -> Receive chunks -> Assemble file
    /// 
    /// # Arguments
    /// * `connection` - The server connection
    /// * `socket` - The UDP socket for receiving packets
    /// * `output_dir` - Directory where received file will be saved
    /// * `manifest_stream` - Stream ID for manifest (typically STREAM_MANIFEST = 4)
    /// * `data_stream` - Stream ID for data chunks (typically STREAM_DATA = 8)
    /// 
    /// # Returns
    /// Path to the assembled file and total bytes received
    pub fn receive_file_integrated(
        &mut self,
        connection: &mut ServerConnection,
        socket: &std::net::UdpSocket,
        output_dir: &Path,
        manifest_stream: u64,
        data_stream: u64,
    ) -> Result<(PathBuf, u64), Box<dyn std::error::Error>> {
        use crate::transport::manifest_stream::ManifestReceiver;
        use crate::client::receiver::FileReceiver;
        use std::time::{Duration, Instant};
        
        log::info!("TransferManager: starting integrated file receive");
        
        let mut buf = vec![0u8; 65535];
        let mut out = vec![0u8; 65535];
        
        // --- MANIFEST RECEIVE PHASE ---
        log::info!("Receiving manifest on stream {}...", manifest_stream);
        let mut manifest_receiver = ManifestReceiver::new();
        let mut manifest_buffer = vec![0u8; 65535];
        let manifest_timeout = Duration::from_secs(10);
        let manifest_start = Instant::now();
        
        let manifest = loop {
            // First, receive packets from network
            socket.set_read_timeout(Some(Duration::from_millis(10)))?;
            if let Ok((len, from)) = socket.recv_from(&mut buf) {
                let _ = connection.process_packet(&mut buf[..len], from, socket.local_addr()?);
            }
            
            // Send any response packets
            let _ = connection.send_packets(socket, &mut out);
            
            // Now try to read from stream
            match connection.stream_recv(manifest_stream, &mut manifest_buffer) {
                Ok((read, fin)) => {
                    if read > 0 {
                        match manifest_receiver.receive_chunk(&manifest_buffer[..read], fin) {
                            Ok(Some(m)) => {
                                log::info!("Server: received manifest for file: {}", m.file_name);
                                break m;
                            }
                            Ok(None) => {
                                // Need more data
                                continue;
                            }
                            Err(e) => {
                                return Err(format!("Manifest receive error: {:?}", e).into());
                            }
                        }
                    }
                }
                Err(quiche::Error::Done) => {
                    if manifest_start.elapsed() > manifest_timeout {
                        return Err("Manifest receive timeout".into());
                    }
                    continue;
                }
                Err(e) => {
                    return Err(format!("Stream receive error: {:?}", e).into());
                }
            }
        };
        
        log::info!("Manifest received: {} chunks, {} bytes", 
            manifest.total_chunks, manifest.file_size);
        
        // --- FILE RECEIVE PHASE ---
        log::info!("Receiving file chunks on stream {}...", data_stream);
        
        // Create file receiver - it will handle .part file internally
        let mut receiver = FileReceiver::new(
            output_dir,
            &manifest.file_name,
            manifest.file_size,
        )?;
        
        let mut stream_buffer = Vec::new(); // Accumulate stream data
        let mut chunk_buffer = vec![0u8; 65535];
        let receive_timeout = Duration::from_secs(30);
        let receive_start = Instant::now();
        let mut last_progress = 0.0;
        let mut chunks_received = 0u64;
        let mut expecting_length = true;
        let mut expected_chunk_size = 0usize;
        
        loop {
            // Receive network packets first
            socket.set_read_timeout(Some(Duration::from_millis(10)))?;
            if let Ok((len, from)) = socket.recv_from(&mut buf) {
                let to = socket.local_addr()?;
                let _ = connection.process_packet(&mut buf[..len], from, to);
                let _ = connection.send_packets(socket, &mut out);
            }
            
            // Read from data stream and accumulate
            match connection.stream_recv(data_stream, &mut chunk_buffer) {
                Ok((read, fin)) => {
                    if read > 0 {
                        stream_buffer.extend_from_slice(&chunk_buffer[..read]);
                        
                        // Process complete messages with length framing
                        loop {
                            if expecting_length {
                                // Need 4 bytes for length prefix
                                if stream_buffer.len() < 4 {
                                    break;
                                }
                                let len_bytes: [u8; 4] = stream_buffer[0..4].try_into().unwrap();
                                expected_chunk_size = u32::from_be_bytes(len_bytes) as usize;
                                stream_buffer.drain(0..4);
                                expecting_length = false;
                            } else {
                                // Need complete chunk packet
                                if stream_buffer.len() < expected_chunk_size {
                                    break;
                                }
                                
                                // Extract and process chunk
                                let chunk_data: Vec<u8> = stream_buffer.drain(0..expected_chunk_size).collect();
                                match receiver.receive_chunk(&chunk_data) {
                                    Ok(_chunk) => {
                                        chunks_received += 1;
                                        let progress = receiver.progress();
                                        if chunks_received % 5 == 0 || progress - last_progress > 0.1 {
                                            log::info!("Server: received chunk {}/{} ({:.1}%)", 
                                                chunks_received, manifest.total_chunks, progress * 100.0);
                                            last_progress = progress;
                                        }
                                    }
                                    Err(e) => {
                                        log::error!("Server: chunk decode error: {:?}", e);
                                    }
                                }
                                expecting_length = true;
                            }
                        }
                    }
                    
                    if fin {
                        log::info!("Server: received FIN on data stream");
                        break;
                    }
                }
                Err(quiche::Error::Done) => {
                    // No data available, check if complete
                    if receiver.is_complete() {
                        log::info!("Server: all chunks received!");
                        break;
                    }
                    
                    if receive_start.elapsed() > receive_timeout {
                        return Err("File receive timeout".into());
                    }
                    
                    std::thread::sleep(Duration::from_millis(10));
                    continue;
                }
                Err(quiche::Error::InvalidStreamState(_)) => {
                    // Stream not ready yet, wait for data
                    if receive_start.elapsed() > receive_timeout {
                        return Err("File receive timeout waiting for stream".into());
                    }
                    std::thread::sleep(Duration::from_millis(10));
                    continue;
                }
                Err(e) => {
                    return Err(format!("Stream receive error: {:?}", e).into());
                }
            }
            
            // Check if complete
            if receiver.is_complete() {
                log::info!("Server: all chunks received!");
                break;
            }
            
            if receive_start.elapsed() > receive_timeout {
                return Err("File receive timeout".into());
            }
        }
        
        // Finalize file
        let final_path = receiver.finalize()?;
        let bytes_received = manifest.file_size;
        
        log::info!("TransferManager: file receive complete!");
        log::info!("  File saved to: {:?}", final_path);
        log::info!("  Total bytes: {}", bytes_received);
        
        Ok((final_path, bytes_received))
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
