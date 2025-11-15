// Server-side transfer logic

use super::connection::ServerConnection;
use super::sender::DataSender;
use crate::protocol::manifest::ManifestBuilder;
use crate::transport::manifest_stream::ManifestSender;
use crate::protocol::hash_check::{HashCheckRequestReceiver, HashCheckResponseSender};
use crate::protocol::resume::{ResumeRequestReceiver, ResumeResponseSender};
use crate::chunking::ChunkBitmap;
use std::path::{Path, PathBuf};

const DEFAULT_CHUNK_SIZE: usize = 8192;
const STREAM_HASH_CHECK: u64 = 16;  // Client-initiated bidirectional stream for hash checks (changed from 1)
const STREAM_RESUME: u64 = 20;      // Client-initiated bidirectional stream for resume protocol

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
                    continue;
                }
                Err(e) => {
                    return Err(format!("Stream receive error: {:?}", e).into());
                }
            }
        };
        
        log::info!("Manifest received: {} chunks, {} bytes", 
            manifest.total_chunks, manifest.file_size);
        
        // --- RESUME PROTOCOL PHASE ---
        // Check if client wants to resume a partial transfer
        log::info!("Server: checking for resume request on stream {}...", STREAM_RESUME);
        
        let mut chunk_bitmap = ChunkBitmap::with_exact_size(manifest.total_chunks as u32);
        let bitmap_path = output_dir.join(format!(".{}.bitmap", manifest.session_id));
        let mut resume_mode = false;
        let mut skip_chunks: std::collections::HashSet<u64> = std::collections::HashSet::new();
        
        // Wait briefly for resume request
        let mut resume_request_receiver = ResumeRequestReceiver::new();
        let mut resume_iterations = 0;
        const MAX_RESUME_WAIT: usize = 50;  // 50 * 10ms = 500ms
        
        while resume_iterations < MAX_RESUME_WAIT {
            // Process network I/O
            socket.set_read_timeout(Some(Duration::from_millis(10)))?;
            if let Ok((len, from)) = socket.recv_from(&mut buf) {
                let _ = connection.process_packet(&mut buf[..len], from, socket.local_addr()?);
            }
            let _ = connection.send_packets(socket, &mut out);
            
            // Check if resume stream is readable
            let readable_streams: Vec<u64> = connection.readable().collect();
            if readable_streams.contains(&STREAM_RESUME) {
                match connection.stream_recv(STREAM_RESUME, &mut buf) {
                    Ok((read, fin)) => {
                        if read > 0 {
                            if let Ok(Some(request)) = resume_request_receiver.receive_chunk(&buf[..read], fin) {
                                log::info!("Server: received resume request with {} received chunks", request.received_chunks.len());
                                resume_mode = true;
                                
                                // Reconstruct bitmap from received chunks
                                for &chunk_idx in &request.received_chunks {
                                    if chunk_idx < manifest.total_chunks {
                                        chunk_bitmap.mark_received(chunk_idx as u32, chunk_idx == manifest.total_chunks - 1);
                                        skip_chunks.insert(chunk_idx);
                                    }
                                }
                                
                                // Find missing chunks
                                let missing = chunk_bitmap.find_missing();
                                let missing_u64: Vec<u64> = missing.iter().map(|&x| x as u64).collect();
                                
                                log::info!("Server: {} of {} chunks already received, {} missing", 
                                    request.received_chunks.len(), manifest.total_chunks, missing_u64.len());
                                
                                // Send resume response
                                let resume_response_sender = ResumeResponseSender::new();
                                resume_response_sender.send_response(
                                    request.session_id.clone(),
                                    true,
                                    missing_u64.clone(),
                                    missing_u64.len() as u64,
                                    None,
                                    |data, fin| connection.stream_send(STREAM_RESUME, data, fin)
                                )?;
                                
                                // Flush response
                                for _ in 0..10 {
                                    let _ = connection.send_packets(socket, &mut out);
                                    std::thread::sleep(Duration::from_millis(5));
                                }
                                
                                log::info!("Server: resume response sent");
                                break;
                            }
                        }
                    }
                    Err(quiche::Error::Done) => {}
                    Err(e) => log::debug!("Server: resume stream error: {:?}", e),
                }
            }
            
            resume_iterations += 1;
            std::thread::sleep(Duration::from_millis(10));
        }
        
        if !resume_mode {
            log::info!("Server: no resume request received, starting fresh transfer");
        }
        
        // --- HASH CHECK PHASE (Deduplication) ---
        log::info!("Server: waiting for hash check request on stream {} (client-initiated)...", STREAM_HASH_CHECK);
        
        // Create chunk index
        use crate::chunking::ChunkHashIndex;
        let index_dir = output_dir.join(".sftpx");
        std::fs::create_dir_all(&index_dir)?;
        let mut chunk_index = ChunkHashIndex::new(&index_dir).unwrap_or_else(|e| {
            log::warn!("Server: failed to create/load chunk index: {:?}", e);
            ChunkHashIndex::new(&std::env::temp_dir()).expect("Failed to create temp index")
        });
        
        // Receive hash check request on client-initiated stream STREAM_HASH_CHECK
        let mut hash_request_receiver = HashCheckRequestReceiver::new();
        let mut hash_request_received = false;
        let mut chunk_hashes_to_check = vec![];
        let mut idle_iterations = 0;
        let mut total_bytes_received = 0;
        const MAX_IDLE: usize = 500;  // Increased timeout: 500 * 10ms = 5 seconds
        
        socket.set_read_timeout(Some(Duration::from_millis(10)))?;
        
        while !hash_request_received && idle_iterations < MAX_IDLE {
            let mut made_progress = false;
            
            // Process network I/O to receive hash check request
            match socket.recv_from(&mut buf) {
                Ok((len, from)) => {
                    log::debug!("Server: received {} bytes from {}", len, from);
                    let _ = connection.process_packet(&mut buf[..len], from, socket.local_addr()?);
                    made_progress = true;
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock || e.kind() == std::io::ErrorKind::TimedOut => {
                    // No data available
                }
                Err(e) => {
                    log::warn!("Server: socket recv error during hash check: {:?}", e);
                }
            }
            
            // Send any response packets
            let _ = connection.send_packets(socket, &mut out);
            
            // Log readable streams for debugging
            if idle_iterations % 100 == 0 {
                let readable_streams: Vec<u64> = connection.readable().collect();
                if !readable_streams.is_empty() {
                    log::debug!("Server: readable streams: {:?}", readable_streams);
                }
            }
            
            // Only try to read from stream 16 if it's actually readable
            let readable_streams: Vec<u64> = connection.readable().collect();
            if readable_streams.contains(&STREAM_HASH_CHECK) {
                // Check for hash check request on STREAM_HASH_CHECK
                loop {
                    match connection.stream_recv(STREAM_HASH_CHECK, &mut buf[..]) {
                        Ok((read, fin)) => {
                            if read > 0 {
                                total_bytes_received += read;
                                log::debug!("Server: read {} bytes from hash check stream (total: {}, fin: {})", read, total_bytes_received, fin);
                                made_progress = true;
                                
                                match hash_request_receiver.receive_chunk(&buf[..read], fin) {
                                    Ok(Some(request)) => {
                                        chunk_hashes_to_check = request.chunk_hashes;
                                        hash_request_received = true;
                                        log::info!("Server: received hash check request with {} hashes ({} bytes)", chunk_hashes_to_check.len(), total_bytes_received);
                                        break;
                                    }
                                    Ok(None) => {
                                        // Need more data, continue reading
                                        log::debug!("Server: hash check incomplete, need more data");
                                        continue;
                                    }
                                    Err(e) => {
                                        log::error!("Server: hash check parse error: {:?}", e);
                                        break;
                                    }
                                }
                            }
                            if fin {
                                log::debug!("Server: hash check stream closed (fin received)");
                                break;
                            }
                            if read == 0 {
                                break;
                            }
                        }
                        Err(quiche::Error::Done) => break,
                        Err(e) => {
                            log::debug!("Server: stream_recv error on stream {}: {:?}", STREAM_HASH_CHECK, e);
                            break;
                        }
                    }
                }
            }
            
            if hash_request_received {
                break;
            }
            
            // Only increment idle counter if we didn't make any progress
            if !made_progress {
                idle_iterations += 1;
            } else {
                idle_iterations = 0;  // Reset on progress
            }
            
            if connection.is_closed() {
                log::warn!("Server: connection closed during hash check");
                break;
            }
        }
        
        if !hash_request_received {
            log::warn!("Server: hash check request not received after {} iterations, proceeding without dedup", idle_iterations);
        }
        
        // Check which hashes exist in the index
        let mut existing_hashes = vec![];
        for hash in &chunk_hashes_to_check {
            if chunk_index.has_chunk(hash) {
                existing_hashes.push(hash.clone());
            }
        }
        
        log::info!("Server: {} out of {} chunks already exist", 
            existing_hashes.len(), chunk_hashes_to_check.len());
        
        // Send hash check response back on same stream
        if hash_request_received {
            let hash_response_sender = HashCheckResponseSender::new();
            
            log::info!("Server: sending hash check response with {} existing hashes", existing_hashes.len());
            
            hash_response_sender.send_response(
                manifest.session_id.clone(),
                existing_hashes.clone(),
                |data, fin| {
                    log::debug!("Server: writing {} bytes to hash check stream (fin={})", data.len(), fin);
                    connection.stream_send(STREAM_HASH_CHECK, data, fin)
                        .map_err(|e| crate::common::error::Error::Protocol(format!("Failed to send hash check response: {:?}", e)))
                }
            )?;
            
            // Flush the response
            for _ in 0..20 {
                if let Err(e) = connection.send_packets(socket, &mut out) {
                    log::warn!("Server: error flushing hash check response: {:?}", e);
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            
            log::info!("Server: hash check response sent and flushed");
        }
        
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
                                        
                                        // Update bitmap with received chunk
                                        let is_last = chunks_received == manifest.total_chunks;
                                        chunk_bitmap.mark_received((chunks_received - 1) as u32, is_last);
                                        
                                        // Periodically save bitmap for resume
                                        if chunks_received % 10 == 0 || is_last {
                                            if let Err(e) = chunk_bitmap.save_to_disk(&bitmap_path) {
                                                log::warn!("Server: failed to save bitmap: {:?}", e);
                                            }
                                        }
                                        
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
        
        // Update chunk index with received chunks
        log::info!("Server: updating chunk index with {} chunks...", manifest.chunk_hashes.len());
        use crate::chunking::ChunkLocation;
        
        for (chunk_idx, chunk_hash) in manifest.chunk_hashes.iter().enumerate() {
            let chunk_offset = chunk_idx as u64 * manifest.chunk_size as u64;
            let chunk_size = if chunk_idx == manifest.chunk_hashes.len() - 1 {
                // Last chunk might be smaller
                let remaining = manifest.file_size - chunk_offset;
                std::cmp::min(remaining, manifest.chunk_size as u64) as u32
            } else {
                manifest.chunk_size
            };
            
            let location = ChunkLocation {
                file_path: final_path.clone(),
                byte_offset: chunk_offset,
                chunk_size,
            };
            
            chunk_index.add_chunk(chunk_hash.clone(), location);
        }
        
        // Save updated index
        if let Err(e) = chunk_index.save() {
            log::warn!("Server: failed to save chunk index: {:?}", e);
        } else {
            log::info!("Server: chunk index saved successfully ({} total unique chunks)", 
                chunk_index.total_chunks());
        }
        
        // Delete bitmap file after successful transfer
        if bitmap_path.exists() {
            if let Err(e) = std::fs::remove_file(&bitmap_path) {
                log::warn!("Server: failed to delete bitmap file: {:?}", e);
            } else {
                log::debug!("Server: deleted bitmap file");
            }
        }
        
        log::info!("TransferManager: file receive complete!");
        log::info!("  File saved to: {:?}", final_path);
        log::info!("  Total bytes: {}", bytes_received);
        log::info!("  Resume mode: {}", resume_mode);
        
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
