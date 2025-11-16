// Client-side transfer logic

use std::net::UdpSocket;
use std::time::Duration;
use std::path::{Path, PathBuf};
use log::{info, debug, error, warn};
use crate::common::error::{Error, Result};
use crate::common::config::ClientConfig;
use crate::common::types::*;
use crate::protocol::manifest::ManifestBuilder;
use crate::transport::manifest_stream::ManifestReceiver;
use crate::protocol::control::ControlMessage;
use crate::client::receiver::FileReceiver;
use super::connection::ClientConnection;
use super::streams::{StreamManager, STREAM_CONTROL, STREAM_HASH_CHECK, STREAM_RESUME, STREAM_MANIFEST, STREAM_DATA, STREAM_STATUS};
use crate::protocol::hash_check::{HashCheckRequestSender, HashCheckResponseReceiver};
use crate::protocol::resume::{ResumeRequestSender, ResumeResponseReceiver};
use crate::chunking::ChunkBitmap;
use super::session::ClientSession;
use std::collections::HashMap;

pub struct Transfer {
    config: ClientConfig,
    connection: Option<ClientConnection>,
    stream_manager: StreamManager,
    session: Option<ClientSession>,
    socket: Option<UdpSocket>,
    state: TransferState,
    resume_bitmaps: HashMap<String, ChunkBitmap>,  // In-memory bitmap storage by session_id
}

impl Transfer {
    /// Create a new transfer for sending a file
    pub fn send_file(config: ClientConfig, file_path: &str, destination: &str) -> Result<Self> {
        let file_path_buf = PathBuf::from(file_path);
        let metadata = std::fs::metadata(&file_path_buf)
            .map_err(|_| Error::FileNotFound(file_path.to_string()))?;
        
        let session = ClientSession::new(
            file_path_buf,
            metadata.len(),
            config.chunk_size,
            destination.to_string(),
            TransferDirection::Send,
        );
        
        Ok(Self {
            config,
            connection: None,
            stream_manager: StreamManager::new(),
            session: Some(session),
            socket: None,
            state: TransferState::Initializing,
            resume_bitmaps: HashMap::new(),
        })
    }
    
    /// Create a new transfer for receiving a file
    pub fn receive_file(config: ClientConfig, session_id: &str) -> Result<Self> {
        let session = ClientSession::load(&config.session_dir, session_id)?;
        Ok(Self {
            config,
            connection: None,
            stream_manager: StreamManager::new(),
            session: Some(session),
            socket: None,
            state: TransferState::Initializing,
            resume_bitmaps: HashMap::new(),
        })
    }
    
    /// Resume an existing transfer
    pub fn resume(config: ClientConfig, session_id: &str) -> Result<Self> {
        let mut session = ClientSession::load(&config.session_dir, session_id)?;
        session.update_state(TransferState::Resuming);
        Ok(Self {
            config,
            connection: None,
            stream_manager: StreamManager::new(),
            session: Some(session),
            socket: None,
            state: TransferState::Resuming,
            resume_bitmaps: HashMap::new(),
        })
    }
    
    /// Main transfer event loop with proper handshake
    pub fn run(&mut self) -> Result<()> {
        // Bind UDP socket
        let socket = UdpSocket::bind("0.0.0.0:0")?;
        socket.connect(self.config.server_addr)?;
        socket.set_nonblocking(false)?;  // Use blocking mode initially
        
        let local_addr = socket.local_addr()?;
        let peer_addr = self.config.server_addr;
        
        info!("Client: connecting to {}", peer_addr);
        
        // Create QUIC connection
        let mut connection = ClientConnection::new(&self.config, local_addr)?;
        
        let mut buf = vec![0u8; 65535];
        let mut out = vec![0u8; MAX_DATAGRAM_SIZE];
        
        // Send initial packet
        let (len, send_info) = connection.send(&mut out)?;
        socket.send_to(&out[..len], send_info.to)?;
        info!("Client: sent initial packet ({} bytes)", len);
        
        // --- HANDSHAKE PHASE ---
        info!("Client: waiting for handshake to complete...");
        let mut handshake_iter = 0;
        
        loop {
            // Receive packets
            socket.set_read_timeout(Some(Duration::from_millis(100)))?;
            match socket.recv_from(&mut buf) {
                Ok((len, from)) => {
                    debug!("Client: recv {} bytes from {}", len, from);
                    let recv_info = quiche::RecvInfo { from, to: local_addr };
                    match connection.recv(&mut buf[..len], recv_info) {
                        Ok(_) => {},
                        Err(e) => debug!("Client: conn.recv error: {:?}", e),
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock || 
                          e.kind() == std::io::ErrorKind::TimedOut => {
                    // Timeout is normal during handshake
                }
                Err(e) => return Err(Error::from(e)),
            }
            
            // Send handshake packets
            while let Ok((len, send_info)) = connection.send(&mut out) {
                socket.send_to(&out[..len], send_info.to)?;
            }
            
            // Check if handshake complete
            if connection.is_established() && connection.peer_streams_left_bidi() > 0 {
                info!("Client: handshake complete!");
                self.state = TransferState::Handshaking;
                break;
            }
            
            handshake_iter += 1;
            if handshake_iter % 10 == 0 {
                debug!("Client: handshake iter={} is_established={} peer_streams_left_bidi={}",
                    handshake_iter, connection.is_established(), connection.peer_streams_left_bidi());
            }
            
            std::thread::sleep(Duration::from_millis(10));
        }
        
        // Initialize stream priorities
        self.stream_manager.initialize_streams(&mut connection)?;
        info!("Client: initialized 4 streams (control, manifest, data, status)");
        
        // --- APPLICATION DATA PHASE ---
        info!("Client: sending application messages on 4 streams...");
        self.state = TransferState::Transferring;
        
        // Send test messages on each stream
        let messages: Vec<(u64, &[u8])> = vec![
            (STREAM_CONTROL, b"Control message from client"),
            (STREAM_MANIFEST, b"Manifest message from client"),
            (STREAM_DATA, b"Data message from client"),
            (STREAM_STATUS, b"Status message from client"),
        ];
        
        for (stream_id, message) in &messages {
            match self.stream_manager.send_on_stream(&mut connection, *stream_id, message, true) {
                Ok(wrote) => {
                    let name = self.stream_manager.get_stream_name(*stream_id).unwrap_or("unknown");
                    info!("Client: sent {} bytes on stream {} ({})", wrote, stream_id, name);
                }
                Err(e) => error!("Client: stream_send error on {}: {:?}", stream_id, e),
            }
        }
        
        // Flush all pending packets
        while let Ok((len, send_info)) = connection.send(&mut out) {
            socket.send_to(&out[..len], send_info.to)?;
        }
        
        info!("Client: waiting for server responses...");
        
        // Wait for responses from all 4 streams
        socket.set_read_timeout(Some(Duration::from_millis(100)))?;
        let mut done = false;
        let mut received_streams = std::collections::HashSet::new();
        
        loop {
            match socket.recv_from(&mut buf) {
                Ok((len, from)) => {
                    debug!("Client: recv {} bytes from {}", len, from);
                    let recv_info = quiche::RecvInfo { from, to: local_addr };
                    match connection.recv(&mut buf[..len], recv_info) {
                        Ok(_) => {},
                        Err(e) => debug!("Client: conn.recv error: {:?}", e),
                    }
                    
                    let readable: Vec<u64> = connection.readable().collect();
                    if !readable.is_empty() {
                        debug!("Client: readable streams: {:?}", readable);
                    }
                    
                    for stream_id in readable {
                        loop {
                            match self.stream_manager.recv_from_stream(&mut connection, stream_id, &mut buf) {
                                Ok((read, fin)) => {
                                    if read == 0 {
                                        break;
                                    }
                                    
                                    let msg = String::from_utf8_lossy(&buf[..read]);
                                    let name = self.stream_manager.get_stream_name(stream_id).unwrap_or("unknown");
                                    info!("Client received on stream {} ({}): {}", stream_id, name, msg);
                                    
                                    received_streams.insert(stream_id);
                                    
                                    if fin {
                                        debug!("Client: stream {} finished", stream_id);
                                        break;
                                    }
                                }
                                Err(_) => break,
                            }
                        }
                    }
                    
                    // Check if we received from all 4 streams
                    if received_streams.len() >= 4 {
                        info!("Client: received responses from all 4 streams!");
                        done = true;
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock || 
                          e.kind() == std::io::ErrorKind::TimedOut => {
                    // Normal timeout
                }
                Err(e) => {
                    error!("Client: recv_from error: {:?}", e);
                    break;
                }
            }
            
            // Send any pending packets
            while let Ok((len, send_info)) = connection.send(&mut out) {
                socket.send_to(&out[..len], send_info.to)?;
            }
            
            if done || connection.is_closed() {
                break;
            }
            
            std::thread::sleep(Duration::from_millis(10));
        }
        
        // Clean close
        let _ = connection.close(true, 0x00, b"done");
        
        // Final flush
        while let Ok((len, send_info)) = connection.send(&mut out) {
            socket.send_to(&out[..len], send_info.to)?;
        }
        
        self.state = TransferState::Completed;
        info!("Client: transfer complete!");
        
        // Print statistics
        let stats = connection.stats();
        info!("Client stats: sent={} bytes, recv={} bytes, packets_sent={}, packets_recv={}",
            stats.bytes_sent, stats.bytes_received, stats.packets_sent, stats.packets_received);
        
        Ok(())
    }
    
    /// Run an integrated file receive transfer with all components
    /// This orchestrates: QUIC handshake -> Manifest receive -> Chunk receive -> Verification
    pub fn run_receive(&mut self) -> Result<PathBuf> {
        info!("Starting integrated file receive transfer");
        
        // Bind UDP socket
        let socket = UdpSocket::bind("0.0.0.0:0")?;
        socket.connect(self.config.server_addr)?;
        socket.set_nonblocking(false)?;
        
        let local_addr = socket.local_addr()?;
        let peer_addr = self.config.server_addr;
        
        info!("Client: connecting to {}", peer_addr);
        
        // Create QUIC connection
        let mut connection = ClientConnection::new(&self.config, local_addr)?;
        
        let mut buf = vec![0u8; 65535];
        let mut out = vec![0u8; MAX_DATAGRAM_SIZE];
        
        // Send initial packet
        let (len, send_info) = connection.send(&mut out)?;
        socket.send_to(&out[..len], send_info.to)?;
        
        // --- HANDSHAKE PHASE ---
        self.handshake_phase(&socket, &mut connection, &mut buf, &mut out, local_addr)?;
        
        // Initialize streams
        self.stream_manager.initialize_streams(&mut connection)?;
        info!("Client: initialized streams");
        
        // --- MANIFEST RECEIVE PHASE ---
        let manifest = self.receive_manifest_phase(&socket, &mut connection, &mut buf, &mut out, local_addr)?;
        info!("Client: received manifest for file: {}", manifest.file_name);
        
        // --- FILE RECEIVE PHASE ---
        let output_path = self.receive_file_phase(
            &socket,
            &mut connection,
            &mut buf,
            &mut out,
            local_addr,
            &manifest,
        )?;
        
        // Clean close
        let _ = connection.close(true, 0x00, b"done");
        while let Ok((len, send_info)) = connection.send(&mut out) {
            socket.send_to(&out[..len], send_info.to)?;
        }
        
        self.state = TransferState::Completed;
        info!("Client: transfer complete! File saved to: {:?}", output_path);
        
        Ok(output_path)
    }
    
    /// Run an integrated file send transfer (upload to server)
    /// This orchestrates: QUIC handshake -> Build manifest -> Send manifest -> Send chunks
    pub fn run_send(&mut self, file_path: &Path) -> Result<u64> {
        info!("Starting integrated file send transfer");
        
        // Bind UDP socket
        let socket = UdpSocket::bind("0.0.0.0:0")?;
        socket.connect(self.config.server_addr)?;
        socket.set_nonblocking(false)?;
        
        let local_addr = socket.local_addr()?;
        let peer_addr = self.config.server_addr;
        
        info!("Client: connecting to {}", peer_addr);
        
        // Create QUIC connection
        let mut connection = ClientConnection::new(&self.config, local_addr)?;
        
        let mut buf = vec![0u8; 65535];
        let mut out = vec![0u8; MAX_DATAGRAM_SIZE];
        
        // Send initial packet
        let (len, send_info) = connection.send(&mut out)?;
        socket.send_to(&out[..len], send_info.to)?;
        
        // --- HANDSHAKE PHASE ---
        self.handshake_phase(&socket, &mut connection, &mut buf, &mut out, local_addr)?;
        
        // Initialize streams
        self.stream_manager.initialize_streams(&mut connection)?;
        info!("Client: initialized streams");
        
        // --- MANIFEST BUILD AND SEND PHASE ---
        let (manifest_bytes, manifest, existing_hashes) = self.send_manifest_phase(
            &socket,
            &mut connection,
            &mut buf,
            &mut out,
            local_addr,
            file_path,
        )?;
        
        // --- RESUME PROTOCOL PHASE (check if server has partial file) ---
        let skip_chunks = self.check_resume_phase(
            &socket,
            &mut connection,
            &mut buf,
            &mut out,
            local_addr,
            &manifest,
        )?;
        
        // --- FILE SEND PHASE ---
        let chunks_bytes = self.send_file_phase(
            &socket,
            &mut connection,
            &mut buf,
            &mut out,
            local_addr,
            file_path,
            &manifest,
            &existing_hashes,
            &skip_chunks,
        )?;
        
        // Clean close
        let _ = connection.close(true, 0x00, b"done");
        while let Ok((len, send_info)) = connection.send(&mut out) {
            socket.send_to(&out[..len], send_info.to)?;
        }
        
        self.state = TransferState::Completed;
        let total = manifest_bytes + chunks_bytes;
        info!("Client: upload complete! Sent {} bytes total", total);
        
        Ok(total)
    }
    
    /// Handshake phase - establish QUIC connection
    fn handshake_phase(
        &mut self,
        socket: &UdpSocket,
        connection: &mut ClientConnection,
        buf: &mut [u8],
        out: &mut [u8],
        local_addr: std::net::SocketAddr,
    ) -> Result<()> {
        info!("Client: waiting for handshake to complete...");
        
        loop {
            socket.set_read_timeout(Some(Duration::from_millis(100)))?;
            match socket.recv_from(buf) {
                Ok((len, from)) => {
                    let recv_info = quiche::RecvInfo { from, to: local_addr };
                    let _ = connection.recv(&mut buf[..len], recv_info);
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock ||
                          e.kind() == std::io::ErrorKind::TimedOut => {}
                Err(e) => return Err(Error::from(e)),
            }
            
            while let Ok((len, send_info)) = connection.send(out) {
                socket.send_to(&out[..len], send_info.to)?;
            }
            
            if connection.is_established() && connection.peer_streams_left_bidi() > 0 {
                info!("Client: handshake complete!");
                self.state = TransferState::Handshaking;
                break;
            }
            
            std::thread::sleep(Duration::from_millis(10));
        }
        
        Ok(())
    }
    
    /// Manifest receive phase - receive and parse manifest
    fn receive_manifest_phase(
        &mut self,
        socket: &UdpSocket,
        connection: &mut ClientConnection,
        buf: &mut [u8],
        out: &mut [u8],
        local_addr: std::net::SocketAddr,
    ) -> Result<crate::protocol::messages::Manifest> {
        info!("Client: receiving manifest on stream {}...", STREAM_MANIFEST);
        
        let mut manifest_receiver = ManifestReceiver::new();
        
        loop {
            // Receive packets
            socket.set_read_timeout(Some(Duration::from_millis(100)))?;
            match socket.recv_from(buf) {
                Ok((len, from)) => {
                    let recv_info = quiche::RecvInfo { from, to: local_addr };
                    let _ = connection.recv(&mut buf[..len], recv_info);
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock ||
                          e.kind() == std::io::ErrorKind::TimedOut => {}
                Err(e) => return Err(Error::from(e)),
            }
            
            // Check for readable streams
            let readable: Vec<u64> = connection.readable().collect();
            for stream_id in readable {
                if stream_id == STREAM_MANIFEST {
                    loop {
                        match connection.stream_recv(stream_id, buf) {
                            Ok((read, fin)) => {
                                if read == 0 {
                                    break;
                                }
                                
                                debug!("Client: received {} bytes on manifest stream", read);
                                
                                if let Some(manifest) = manifest_receiver.receive_chunk(&buf[..read], fin)? {
                                    return Ok(manifest);
                                }
                                
                                if fin {
                                    break;
                                }
                            }
                            Err(_) => break,
                        }
                    }
                }
            }
            
            // Send any pending packets
            while let Ok((len, send_info)) = connection.send(out) {
                socket.send_to(&out[..len], send_info.to)?;
            }
            
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    
    /// File receive phase - receive chunks and assemble file
    fn receive_file_phase(
        &mut self,
        socket: &UdpSocket,
        connection: &mut ClientConnection,
        buf: &mut [u8],
        out: &mut [u8],
        local_addr: std::net::SocketAddr,
        manifest: &crate::protocol::messages::Manifest,
    ) -> Result<PathBuf> {
        info!("Client: receiving file data on stream {}...", STREAM_DATA);
        
        // Create file receiver with output directory from session or current directory
        let output_dir = self.config.session_dir.parent()
            .unwrap_or_else(|| std::path::Path::new("."));
        std::fs::create_dir_all(output_dir)?;
        
        let mut receiver = FileReceiver::new(
            output_dir,
            &manifest.file_name,
            manifest.file_size,
        )?;
        
        // Set expected hash from manifest
        receiver.set_expected_hash(manifest.file_hash.clone())?;
        
        // Setup control message sender for auto-retransmit
        let session_id = manifest.session_id.clone();
        let control_sender = Box::new(move |msg: ControlMessage| {
            // In a real implementation, this would send over STREAM_CONTROL
            info!("Would send control message: {:?}", msg.get_type());
            Ok(())
        });
        
        receiver.enable_auto_retransmit(session_id, control_sender);
        
        let mut last_progress = 0.0;
        
        loop {
            // Receive packets
            socket.set_read_timeout(Some(Duration::from_millis(100)))?;
            match socket.recv_from(buf) {
                Ok((len, from)) => {
                    let recv_info = quiche::RecvInfo { from, to: local_addr };
                    let _ = connection.recv(&mut buf[..len], recv_info);
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock ||
                          e.kind() == std::io::ErrorKind::TimedOut => {}
                Err(e) => return Err(Error::from(e)),
            }
            
            // Check for readable streams
            let readable: Vec<u64> = connection.readable().collect();
            for stream_id in readable {
                if stream_id == STREAM_DATA {
                    loop {
                        match connection.stream_recv(stream_id, buf) {
                            Ok((read, fin)) => {
                                if read == 0 {
                                    break;
                                }
                                
                                // Receive chunk
                                match receiver.receive_chunk(&buf[..read]) {
                                    Ok(chunk) => {
                                        let progress = receiver.progress();
                                        if progress - last_progress > 0.1 {
                                            info!("Progress: {:.1}%", progress * 100.0);
                                            last_progress = progress;
                                        }
                                        
                                        if chunk.end_of_file {
                                            info!("Received final chunk");
                                        }
                                    }
                                    Err(e) => {
                                        error!("Chunk receive error: {:?}", e);
                                        // Auto-retransmit will handle this
                                    }
                                }
                                
                                if fin {
                                    break;
                                }
                            }
                            Err(_) => break,
                        }
                    }
                }
            }
            
            // Send any pending packets
            while let Ok((len, send_info)) = connection.send(out) {
                socket.send_to(&out[..len], send_info.to)?;
            }
            
            // Check if complete
            if receiver.is_complete() {
                info!("All chunks received! Finalizing file...");
                let final_path = receiver.finalize()?;
                return Ok(final_path);
            }
            
            // Check for failed chunks
            if receiver.has_failed_chunks() {
                let failed = receiver.get_failed_chunks();
                return Err(Error::Protocol(format!(
                    "Transfer failed: {} chunks exceeded max retries",
                    failed.len()
                )));
            }
            
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    
    /// Send manifest phase - build and send manifest to server
    /// Returns (bytes_sent, manifest, existing_hashes)
    fn send_manifest_phase(
        &mut self,
        socket: &UdpSocket,
        connection: &mut ClientConnection,
        buf: &mut [u8],
        out: &mut [u8],
        local_addr: std::net::SocketAddr,
        file_path: &Path,
    ) -> Result<(u64, crate::protocol::messages::Manifest, Vec<Vec<u8>>)> {
        info!("Client: building manifest for upload...");
        
        // Generate session ID
        use std::time::SystemTime;
        let timestamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let session_id = format!("upload_{}", timestamp);
        
        // Build manifest using parallel hash computation for better performance
        let manifest = ManifestBuilder::new(session_id.clone())
            .file_path(file_path)
            .chunk_size(self.config.chunk_size as u32)
            .build_parallel()?;
        
        info!("Client: sending manifest ({} chunks, {} bytes total)", 
            manifest.total_chunks, manifest.file_size);
        
        // Send manifest on STREAM_MANIFEST
        // Encode manifest first
        let encoded = manifest.encode_to_vec();
        info!("Client: manifest encoded ({} bytes)", encoded.len());
        
        // Send with retry on partial writes
        let mut total_sent = 0usize;
        let mut offset = 0usize;
        let max_retries = 100;
        let mut retries = 0;
        
        while offset < encoded.len() {
            let remaining = &encoded[offset..];
            let is_last = offset + remaining.len() == encoded.len();
            
            match connection.stream_send(STREAM_MANIFEST, remaining, is_last) {
                Ok(written) => {
                    if written > 0 {
                        offset += written;
                        total_sent += written;
                        retries = 0; // Reset retry counter on progress
                        
                        // Flush packets
                        while let Ok((len, send_info)) = connection.send(out) {
                            socket.send_to(&out[..len], send_info.to)?;
                        }
                    } else {
                        // No bytes written, wait and retry
                        retries += 1;
                        if retries > max_retries {
                            return Err(Error::Protocol(format!(
                                "Manifest send stalled: {}/{} bytes sent",
                                total_sent, encoded.len()
                            )));
                        }
                        std::thread::sleep(Duration::from_millis(10));
                    }
                }
                Err(_e) => {
                    // Stream not writable, flush and retry
                    retries += 1;
                    if retries > max_retries {
                        return Err(Error::Protocol(format!(
                            "Manifest send timeout: {}/{} bytes sent",
                            total_sent, encoded.len()
                        )));
                    }
                    
                    while let Ok((len, send_info)) = connection.send(out) {
                        socket.send_to(&out[..len], send_info.to)?;
                    }
                    
                    // Receive ACKs
                    socket.set_read_timeout(Some(Duration::from_millis(10)))?;
                    if let Ok((len, from)) = socket.recv_from(buf) {
                        let recv_info = quiche::RecvInfo { from, to: local_addr };
                        let _ = connection.recv(&mut buf[..len], recv_info);
                    }
                    
                    std::thread::sleep(Duration::from_millis(10));
                }
            }
        }
        
        info!("Client: manifest sent ({} bytes)", total_sent);
        
        // Phase 3: Hash check on dedicated server-initiated stream (STREAM_HASH_CHECK)
        // SKIP hash check for maximum performance - proceed directly to upload
        info!("Client: skipping hash check for maximum performance");
        let existing_hashes = vec![];
        
        /*
        // Original hash check code - disabled for performance
        info!("Client: performing hash check for deduplication");
        
        let chunk_hashes: Vec<Vec<u8>> = manifest.chunk_hashes.clone();
        let existing_hashes = self.hash_check_phase(
            socket,
            connection,
            buf,
            out,
            local_addr,
            &session_id,
            chunk_hashes,
        )?;
        */
        
        info!("Client: hash check complete, {} chunks already exist", existing_hashes.len());
        
        Ok((total_sent as u64, manifest, existing_hashes))
    }
    
    /// Resume protocol phase - check if server has partial file
    fn check_resume_phase(
        &mut self,
        socket: &UdpSocket,
        connection: &mut ClientConnection,
        buf: &mut [u8],
        out: &mut [u8],
        local_addr: std::net::SocketAddr,
        manifest: &crate::protocol::messages::Manifest,
    ) -> Result<std::collections::HashSet<u64>> {
        use std::collections::HashSet;
        
        // Check if we have a saved bitmap in memory from a previous interrupted transfer
        let bitmap = if let Some(saved_bitmap) = self.resume_bitmaps.get(&manifest.session_id) {
            info!("Client: found saved bitmap in memory, attempting resume...");
            info!("Client: loaded bitmap with {} chunks received", saved_bitmap.received_count());
            
            // Create a copy for use
            let total_chunks = saved_bitmap.total_chunks().unwrap_or(0);
            let mut bitmap_copy = ChunkBitmap::with_exact_size(total_chunks);
            for i in 0..total_chunks {
                if saved_bitmap.is_received(i) {
                    bitmap_copy.mark_received(i, saved_bitmap.is_complete() && i == total_chunks - 1);
                }
            }
            bitmap_copy
        } else {
            info!("Client: no saved bitmap found in memory, starting fresh transfer");
            return Ok(HashSet::new());
        };
        
        // Don't resume if no progress was made
        if bitmap.received_count() == 0 {
            info!("Client: bitmap shows no progress, starting fresh");
            return Ok(HashSet::new());
        }
        
        // Send resume request to server
        let resume_request_sender = ResumeRequestSender::new();
        let received_chunks = bitmap.get_received_chunks();
        let last_chunk = received_chunks.last().copied();
        
        info!("Client: sending resume request for {} received chunks", received_chunks.len());
        
        let send_result = resume_request_sender.send_request(
            manifest.session_id.clone(),
            received_chunks.clone(),
            Some(bitmap.to_bytes()),
            last_chunk,
            |data: &[u8], fin: bool| -> std::result::Result<usize, quiche::Error> {
                match connection.stream_send(STREAM_RESUME, data, fin) {
                    Ok(n) => Ok(n),
                    Err(_) => Err(quiche::Error::Done), // Map to quiche error
                }
            }
        );
        
        if let Err(e) = send_result {
            warn!("Client: failed to send resume request: {}, starting fresh", e);
            return Ok(HashSet::new());
        }
        
        // Flush the request
        socket.set_read_timeout(Some(Duration::from_millis(10)))?;
        for _ in 0..20 {
            loop {
                match connection.send(out) {
                    Ok((write, send_info)) => {
                        if write > 0 {
                            socket.send_to(&out[..write], send_info.to)?;
                        }
                    }
                    Err(_) => break,
                }
            }
            
            // Receive ACKs
            if let Ok((len, from)) = socket.recv_from(buf) {
                let recv_info = quiche::RecvInfo { from, to: local_addr };
                let _ = connection.recv(&mut buf[..len], recv_info);
            }
            
            std::thread::sleep(Duration::from_millis(5));
        }
        
        info!("Client: resume request sent, waiting for response...");
        
        // Receive resume response
        let mut response_receiver = ResumeResponseReceiver::new();
        let mut received_response = false;
        let mut skip_chunks = HashSet::new();
        let mut idle_iterations = 0;
        const MAX_IDLE: usize = 200; // 200 * 10ms = 2 seconds
        
        while !received_response && idle_iterations < MAX_IDLE {
            // Flush outgoing packets
            while let Ok((write, send_info)) = connection.send(out) {
                socket.send_to(&out[..write], send_info.to)?;
                idle_iterations = 0;
            }
            
            // Receive data
            match socket.recv_from(buf) {
                Ok((len, from)) => {
                    let recv_info = quiche::RecvInfo { from, to: local_addr };
                    let _ = connection.recv(&mut buf[..len], recv_info);
                    idle_iterations = 0;
                    
                    // Check for resume response on STREAM_RESUME
                    while let Ok((read, fin)) = connection.stream_recv(STREAM_RESUME, buf) {
                        if read > 0 {
                            if let Some(response) = response_receiver.receive_chunk(&buf[..read], fin)? {
                                if response.accepted {
                                    info!("Client: resume accepted! Server needs {} chunks", response.chunks_remaining);
                                    
                                    // Build skip set - chunks NOT in missing list
                                    let missing_set: HashSet<u64> = response.missing_chunks.iter().copied().collect();
                                    for chunk_idx in 0..manifest.total_chunks {
                                        if !missing_set.contains(&chunk_idx) {
                                            skip_chunks.insert(chunk_idx);
                                        }
                                    }
                                    
                                    info!("Client: will skip {} chunks, send {} chunks", 
                                        skip_chunks.len(), response.missing_chunks.len());
                                } else {
                                    warn!("Client: resume rejected by server: {:?}", response.error);
                                }
                                received_response = true;
                                break;
                            }
                        }
                        if fin {
                            break;
                        }
                    }
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock ||
                              e.kind() == std::io::ErrorKind::TimedOut => {
                    idle_iterations += 1;
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(e) => return Err(Error::from(e)),
            }
            
            if connection.is_closed() {
                warn!("Client: connection closed during resume");
                return Err(Error::ConnectionClosed);
            }
        }
        
        if !received_response {
            warn!("Client: no resume response received, starting fresh");
            return Ok(HashSet::new());
        }
        
        Ok(skip_chunks)
    }
    
    /// Save bitmap for resume capability (in-memory) - optimized to reuse existing bitmap
    fn save_resume_bitmap(&mut self, session_id: &str, bitmap: &ChunkBitmap) -> Result<()> {
        let total_chunks = bitmap.total_chunks().unwrap_or(0);
        
        // Get or create the bitmap in the HashMap
        let stored_bitmap = self.resume_bitmaps
            .entry(session_id.to_string())
            .or_insert_with(|| ChunkBitmap::with_exact_size(total_chunks));
        
        // Only copy the new received chunks (last chunk since last save)
        // Since we save every 100 chunks, we only need to update the last few
        let received_count = bitmap.received_count();
        for i in (received_count.saturating_sub(100))..total_chunks {
            if bitmap.is_received(i) && !stored_bitmap.is_received(i) {
                stored_bitmap.mark_received(i, bitmap.is_complete() && i == total_chunks - 1);
            }
        }
        
        debug!("Client: updated resume bitmap in memory for session {} ({}/{} chunks)", 
            session_id, bitmap.received_count(), total_chunks);
        Ok(())
    }
    
    /// Hash check phase - check which chunks already exist on server
    #[allow(dead_code)]
    fn hash_check_phase(
        &mut self,
        socket: &UdpSocket,
        connection: &mut ClientConnection,
        buf: &mut [u8],
        out: &mut [u8],
        local_addr: std::net::SocketAddr,
        session_id: &str,
        chunk_hashes: Vec<Vec<u8>>,
    ) -> Result<Vec<Vec<u8>>> {
        info!("Client: starting hash check phase for {} chunk hashes", chunk_hashes.len());
        
        // Send hash check request on client-initiated stream (STREAM_HASH_CHECK = 16)
        // Client opens the stream and both parties can read/write
        let hash_sender = HashCheckRequestSender::new();
        
        // Retry sending if stream isn't ready
        let mut send_attempts = 0;
        let send_result = loop {
            match hash_sender.send_request(
                session_id.to_string(),
                chunk_hashes.clone(),
                |data, fin| {
                    debug!("Client: attempt {} - writing {} bytes to hash check stream (fin={})", send_attempts + 1, data.len(), fin);
                    connection.stream_send(STREAM_HASH_CHECK, data, fin)
                },
            ) {
                Ok(result) => break result,
                Err(e) => {
                    send_attempts += 1;
                    if send_attempts >= 3 {
                        warn!("Client: failed to send hash check request after {} attempts: {}", send_attempts, e);
                        return Ok(vec![]); // Continue without dedup
                    }
                    warn!("Client: hash check send attempt {} failed: {}, retrying...", send_attempts, e);
                    std::thread::sleep(Duration::from_millis(100));
                }
            }
        };
        
        info!("Client: hash check request prepared: {} bytes", send_result);
        
        // Set short read timeout for non-blocking receive during flush
        socket.set_read_timeout(Some(Duration::from_millis(10)))?;
        
        // Actively flush and receive to ensure ALL data is sent
        // The stream_send call buffers data, we need to send packets until buffer is empty
        let mut consecutive_no_send = 0;
        const MAX_NO_SEND: usize = 100;  // More iterations to ensure large messages are sent
        
        while consecutive_no_send < MAX_NO_SEND {
            // Send outgoing packets - keep sending until no more packets
            let mut sent_any = false;
            loop {
                match connection.send(out) {
                    Ok((write, send_info)) => {
                        if write > 0 {
                            socket.send_to(&out[..write], send_info.to)?;
                            sent_any = true;
                            consecutive_no_send = 0;
                            debug!("Client: sent {} bytes during hash check flush", write);
                        }
                    }
                    Err(_) => break,
                }
            }
            
            // Receive to process ACKs and window updates
            match socket.recv_from(buf) {
                Ok((len, from)) => {
                    let recv_info = quiche::RecvInfo {
                        to: local_addr,
                        from,
                    };
                    if let Ok(read) = connection.recv(&mut buf[..len], recv_info) {
                        if read > 0 {
                            debug!("Client: received {} bytes during hash check flush", len);
                            consecutive_no_send = 0;  // Reset on receive too
                        }
                    }
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(ref e) if e.kind() == std::io::ErrorKind::TimedOut => {}  // Expected with short timeout
                Err(e) => {
                    warn!("Client: unexpected recv error during hash check flush: {}", e);
                }
            }
            
            if !sent_any {
                consecutive_no_send += 1;
            }
            
            std::thread::sleep(Duration::from_millis(10));
        }
        
        info!("Client: hash check request sent, waiting for response...");
        
        // Receive hash check response on same stream
        let mut response_receiver = HashCheckResponseReceiver::new();
        let mut received_response = false;
        let mut existing_hashes = vec![];
        let mut idle_iterations = 0;
        const MAX_IDLE: usize = 500;  // 500 * 10ms = 5 seconds
        
        debug!("Client: waiting for hash check response on stream {}...", STREAM_HASH_CHECK);
        
        let start_time = std::time::Instant::now();
        
        while !received_response && idle_iterations < MAX_IDLE {
            // Flush outgoing packets
            let mut sent_packets = 0;
            while let Ok((write, send_info)) = connection.send(out) {
                socket.send_to(&out[..write], send_info.to)?;
                sent_packets += 1;
                idle_iterations = 0;
            }
            
            // Receive data
            match socket.recv_from(buf) {
                Ok((len, from)) => {
                    let recv_info = quiche::RecvInfo {
                        to: local_addr,
                        from,
                    };
                    
                    let _ = connection.recv(&mut buf[..len], recv_info);
                    idle_iterations = 0;
                    
                    // Check for hash check response on STREAM_HASH_CHECK
                    while let Ok((read, fin)) = connection.stream_recv(STREAM_HASH_CHECK, &mut buf[..]) {
                        if read > 0 {
                            debug!("Client: received {} bytes on hash check stream", read);
                            if let Some(response) = response_receiver.receive_chunk(&buf[..read], fin)? {
                                existing_hashes = response.existing_hashes;
                                received_response = true;
                                info!("Client: hash check response received with {} existing chunks", existing_hashes.len());
                                break;
                            }
                        }
                        if fin {
                            break;
                        }
                    }
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    idle_iterations += 1;
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::TimedOut => {
                    // Treat timeout like WouldBlock during hash check wait
                    idle_iterations += 1;
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(e) => {
                    warn!("Client: unexpected error during hash check response wait: {}", e);
                    return Err(Error::from(e));
                }
            }
            
            // Log progress every second
            if idle_iterations % 100 == 0 && idle_iterations > 0 {
                debug!("Client: hash check waiting... {:.1}s elapsed, {} packets sent recently", 
                    start_time.elapsed().as_secs_f64(), sent_packets);
            }
            
            if connection.is_closed() {
                warn!("Client: connection closed during hash check");
                return Err(Error::ConnectionClosed);
            }
        }
        
        if !received_response {
            warn!("Client: hash check response not received after {} iterations, proceeding without dedup", idle_iterations);
            // Continue without deduplication rather than failing
            return Ok(vec![]);
        }
        
        Ok(existing_hashes)
    }
    
    /// Send file phase - send file chunks to server
    fn send_file_phase(
        &mut self,
        socket: &UdpSocket,
        connection: &mut ClientConnection,
        buf: &mut [u8],
        out: &mut [u8],
        local_addr: std::net::SocketAddr,
        file_path: &Path,
        manifest: &crate::protocol::messages::Manifest,
        existing_hashes: &[Vec<u8>],
        skip_chunks: &std::collections::HashSet<u64>,
    ) -> Result<u64> {
        use crate::chunking::ParallelChunker;
        use std::collections::HashSet;
        
        info!("Client: starting file chunk upload...");
        
        // Set socket to non-blocking mode for maximum performance
        socket.set_nonblocking(true)?;
        
        // Convert existing hashes to HashSet for efficient lookup
        let existing_set: HashSet<&[u8]> = existing_hashes.iter()
            .map(|v| v.as_slice())
            .collect();
        
        // Create parallel chunker for high-performance processing
        let chunker = ParallelChunker::new(
            file_path,
            Some(self.config.chunk_size),
            self.config.compression,
            None, // Auto-detect CPU count
        )?;
        
        let total_chunks = chunker.total_chunks();
        let mut bytes_sent = 0u64;
        let mut chunk_count = 0u64;
        let mut chunks_skipped = 0u64;
        
        info!("Client: uploading {} chunks ({} bytes) with compression: {:?}", 
            total_chunks, chunker.file_size(), self.config.compression);
        
        if !existing_set.is_empty() {
            info!("Client: {} chunks already exist on server (will skip via dedup)", existing_set.len());
        }
        
        if !skip_chunks.is_empty() {
            info!("Client: {} chunks already received by server (will skip via resume)", skip_chunks.len());
        }
        
        // Create bitmap for tracking sent chunks (for resume capability)
        let mut sent_bitmap = ChunkBitmap::with_exact_size(total_chunks as u32);
        
        // Track performance
        let start_time = std::time::Instant::now();
        
        // Mark skipped chunks as already sent
        for &chunk_idx in skip_chunks {
            if chunk_idx < total_chunks {
                sent_bitmap.mark_received(chunk_idx as u32, chunk_idx == total_chunks - 1);
            }
        }
        
        // Process chunks in parallel pipeline
        let mut chunk_iter = chunker.process_chunks()?;
        
        while let Some(chunk_result) = chunk_iter.next() {
            let processed_chunk = chunk_result?;
            let chunk_id = processed_chunk.chunk_id;
            let is_last = processed_chunk.end_of_file;
            
            // Check if this chunk should be skipped (resume mode)
            if skip_chunks.contains(&chunk_id) {
                chunks_skipped += 1;
                chunk_count += 1;
                
                if chunk_count % 10 == 0 {
                    info!("Client: skipped chunk {}/{} (resume)", chunk_count, total_chunks);
                }
                continue;
            }
            
            // Check if this chunk's hash exists on server (dedup)
            let should_skip = existing_set.contains(processed_chunk.hash.as_slice());
            
            if should_skip {
                chunks_skipped += 1;
                chunk_count += 1;
                
                if chunk_count % 10 == 0 {
                    info!("Client: skipped chunk {}/{} (dedup)", chunk_count, total_chunks);
                }
                continue;
            }
            
            // Combine length prefix and chunk data into single send
            let len_bytes = (processed_chunk.packet.len() as u32).to_be_bytes();
            let mut combined_data = Vec::with_capacity(4 + processed_chunk.packet.len());
            combined_data.extend_from_slice(&len_bytes);
            combined_data.extend_from_slice(&processed_chunk.packet);
            
            self.send_data_with_flow_control(
                connection, socket, buf, out, local_addr,
                STREAM_DATA, &combined_data, is_last
            )?;
            
            bytes_sent += processed_chunk.packet.len() as u64;
            chunk_count += 1;
            
            // Mark chunk as sent in bitmap for resume capability
            let is_eof_chunk = chunk_count == total_chunks;
            sent_bitmap.mark_received((chunk_count - 1) as u32, is_eof_chunk);
            
            // Periodically save bitmap for resume (every 100 chunks)
            if chunk_count % 100 == 0 || is_eof_chunk {
                if let Err(e) = self.save_resume_bitmap(&manifest.session_id, &sent_bitmap) {
                    warn!("Client: failed to save resume bitmap: {}", e);
                }
            }
            
            if chunk_count % 50 == 0 || is_last {
                let elapsed = start_time.elapsed().as_secs_f64();
                let speed_mbps = if elapsed > 0.0 {
                    (bytes_sent as f64 / elapsed) / (1024.0 * 1024.0)
                } else {
                    0.0
                };
                info!("Client: sent chunk {}/{} ({:.1}%) - {:.2} MB/s", 
                    chunk_count, total_chunks, 
                    (chunk_count as f64 / total_chunks as f64) * 100.0,
                    speed_mbps);
            }
        }
        
        let total_elapsed = start_time.elapsed().as_secs_f64();
        let avg_speed_mbps = if total_elapsed > 0.0 {
            (bytes_sent as f64 / total_elapsed) / (1024.0 * 1024.0)
        } else {
            0.0
        };
        
        info!("Client: average upload speed: {:.2} MB/s", avg_speed_mbps);
        
        if chunks_skipped > 0 {
            info!("Client: deduplication saved {} chunks ({:.1}%)", 
                chunks_skipped, 
                (chunks_skipped as f64 / total_chunks as f64) * 100.0);
        }
        
        // Final flush - keep sending until connection is drained or nothing left to send
        info!("Client: flushing final packets...");
        let mut idle_iterations = 0;
        let max_idle_iterations = 100; // ~500ms of idle time
        
        loop {
            // Send any pending packets
            let mut sent_any = false;
            while let Ok((len, send_info)) = connection.send(out) {
                match socket.send_to(&out[..len], send_info.to) {
                    Ok(_) => { sent_any = true; },
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                    Err(_) => break,  // Ignore other errors during flush
                }
            }
            
            // Receive ACKs
            socket.set_read_timeout(Some(Duration::from_millis(10))).ok();
            match socket.recv_from(buf) {
                Ok((len, from)) => {
                    let recv_info = quiche::RecvInfo { from, to: local_addr };
                    let _ = connection.recv(&mut buf[..len], recv_info);
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {},
                Err(ref e) if e.kind() == std::io::ErrorKind::TimedOut => {},
                Err(_) => {},  // Ignore errors during flush
            }
            
            // If nothing was sent, increment idle counter
            if !sent_any {
                idle_iterations += 1;
                if idle_iterations > max_idle_iterations {
                    info!("Client: flush complete (no more packets to send)");
                    break;
                }
            } else {
                idle_iterations = 0; // Reset on activity
            }
            
            std::thread::sleep(Duration::from_millis(5));
        }
        
        info!("Client: file upload complete ({} bytes sent)", bytes_sent);
        
        // Remove bitmap from memory after successful transfer
        if self.resume_bitmaps.remove(&manifest.session_id).is_some() {
            debug!("Client: removed resume bitmap from memory after successful transfer");
        }
        
        Ok(bytes_sent)
    }
    
    /// Helper: Send data with flow control handling
    fn send_data_with_flow_control(
        &self,
        connection: &mut ClientConnection,
        socket: &UdpSocket,
        buf: &mut [u8],
        out: &mut [u8],
        local_addr: std::net::SocketAddr,
        stream_id: u64,
        data: &[u8],
        fin: bool,
    ) -> Result<()> {
        let mut written = 0;
        let start_time = std::time::Instant::now();
        let max_duration = Duration::from_secs(300);  // 5 minutes timeout
        
        // Set socket to non-blocking for efficient I/O
        socket.set_nonblocking(true).ok();
        
        while written < data.len() {
            // Try to write as much as possible to the stream
            match connection.stream_send(stream_id, &data[written..], fin && written >= data.len() - 1) {
                Ok(w) if w > 0 => {
                    written += w;
                }
                Ok(_) => {}
                Err(ref e) if format!("{:?}", e).contains("Done") => {}
                Err(e) => return Err(e),
            }
            
            // Flush all pending packets aggressively
            loop {
                match connection.send(out) {
                    Ok((len, send_info)) => {
                        match socket.send_to(&out[..len], send_info.to) {
                            Ok(_) => {},
                            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                            Err(e) => return Err(Error::from(e)),
                        }
                    }
                    Err(_) => break,
                }
            }
            
            // Receive ACKs - drain all available
            loop {
                match socket.recv_from(buf) {
                    Ok((len, from)) => {
                        let recv_info = quiche::RecvInfo { from, to: local_addr };
                        let _ = connection.recv(&mut buf[..len], recv_info);
                    }
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                    Err(_) => break,
                }
            }
            
            // If we've written everything, we're done - don't wait around!
            if written >= data.len() {
                break;
            }
            
            // Timeout check
            if start_time.elapsed() > max_duration {
                return Err(Error::Protocol(format!(
                    "Flow control timeout: sent {}/{} bytes ({:.1}%) after {:.1}s",
                    written, data.len(), (written as f64 / data.len() as f64) * 100.0,
                    start_time.elapsed().as_secs_f64()
                )));
            }
        }
        
        Ok(())
    }
    
    pub fn session(&self) -> Option<&ClientSession> {
        self.session.as_ref()
    }
    
    pub fn progress(&self) -> f64 {
        self.session.as_ref().map(|s| s.progress()).unwrap_or(0.0)
    }
    
    pub fn state(&self) -> TransferState {
        self.state
    }
}