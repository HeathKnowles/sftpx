// Client-side transfer logic

use std::net::UdpSocket;
use std::time::Duration;
use std::path::{Path, PathBuf};
use log::{info, debug, error};
use crate::common::error::{Error, Result};
use crate::common::config::ClientConfig;
use crate::common::types::*;
use crate::protocol::manifest::ManifestBuilder;
use crate::transport::manifest_stream::ManifestReceiver;
use crate::protocol::control::ControlMessage;
use crate::client::receiver::FileReceiver;
use super::connection::ClientConnection;
use super::streams::{StreamManager, STREAM_CONTROL, STREAM_HASH_CHECK, STREAM_MANIFEST, STREAM_DATA, STREAM_STATUS};
use crate::protocol::hash_check::{HashCheckRequestSender, HashCheckResponseReceiver};
use super::session::ClientSession;

pub struct Transfer {
    config: ClientConfig,
    connection: Option<ClientConnection>,
    stream_manager: StreamManager,
    session: Option<ClientSession>,
    socket: Option<UdpSocket>,
    state: TransferState,
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
        
        // Build manifest
        let manifest = ManifestBuilder::new(session_id.clone())
            .file_path(file_path)
            .chunk_size(self.config.chunk_size as u32)
            .build()?;
        
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
        
        info!("Client: hash check complete, {} chunks already exist", existing_hashes.len());
        
        Ok((total_sent as u64, manifest, existing_hashes))
    }
    
    /// Hash check phase - check which chunks already exist on server
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
        // Send hash check request on server-initiated stream (STREAM_HASH_CHECK)
        let hash_sender = HashCheckRequestSender::new();
        
        let send_result = hash_sender.send_request(
            session_id.to_string(),
            chunk_hashes.clone(),
            |data, fin| {
                // Send on STREAM_HASH_CHECK - this is a server-initiated bidirectional stream
                // The server opens it (ID 1), but client can write to it
                connection.stream_send(STREAM_HASH_CHECK, data, fin)
            },
        )?;
        
        debug!("Hash check request sent: {} bytes", send_result);
        
        // Flush outgoing packets
        while let Ok((write, send_info)) = connection.send(out) {
            socket.send_to(&out[..write], send_info.to)?;
        }
        
        // Receive hash check response on same stream
        let mut response_receiver = HashCheckResponseReceiver::new();
        let mut received_response = false;
        let mut existing_hashes = vec![];
        let mut idle_iterations = 0;
        const MAX_IDLE: usize = 100;
        
        while !received_response && idle_iterations < MAX_IDLE {
            // Flush outgoing packets
            while let Ok((write, send_info)) = connection.send(out) {
                socket.send_to(&out[..write], send_info.to)?;
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
                            if let Some(response) = response_receiver.receive_chunk(&buf[..read], fin)? {
                                existing_hashes = response.existing_hashes;
                                received_response = true;
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
                Err(e) => return Err(Error::from(e)),
            }
            
            if connection.is_closed() {
                return Err(Error::ConnectionClosed);
            }
        }
        
        if !received_response {
            return Err(Error::Protocol("Hash check response not received".to_string()));
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
    ) -> Result<u64> {
        use crate::chunking::FileChunker;
        use std::collections::HashSet;
        
        info!("Client: starting file chunk upload...");
        
        // Convert existing hashes to HashSet for efficient lookup
        let existing_set: HashSet<&[u8]> = existing_hashes.iter()
            .map(|v| v.as_slice())
            .collect();
        
        let mut chunker = FileChunker::with_compression(
            file_path, 
            Some(self.config.chunk_size),
            self.config.compression
        )?;
        let total_chunks = chunker.total_chunks();
        let mut bytes_sent = 0u64;
        let mut chunk_count = 0u64;
        let mut chunks_skipped = 0u64;
        
        info!("Client: uploading {} chunks ({} bytes) with compression: {:?}", 
            total_chunks, chunker.file_size(), self.config.compression);
        
        if !existing_set.is_empty() {
            info!("Client: {} chunks already exist on server (will skip)", existing_set.len());
        }
        
        while let Some(chunk_packet) = chunker.next_chunk()? {
            let is_last = chunk_count == total_chunks - 1;
            
            // Check if this chunk's hash exists on server
            let chunk_hash = &manifest.chunk_hashes[chunk_count as usize];
            let should_skip = existing_set.contains(chunk_hash.as_slice());
            
            if should_skip {
                chunks_skipped += 1;
                chunk_count += 1;
                
                if chunk_count % 10 == 0 {
                    info!("Client: skipped chunk {}/{} (dedup)", chunk_count, total_chunks);
                }
                continue;
            }
            
            // Send length prefix (4 bytes, big-endian)
            let len_bytes = (chunk_packet.len() as u32).to_be_bytes();
            self.send_data_with_flow_control(
                connection, socket, buf, out, local_addr,
                STREAM_DATA, &len_bytes, false
            )?;
            
            // Send chunk packet data
            self.send_data_with_flow_control(
                connection, socket, buf, out, local_addr,
                STREAM_DATA, &chunk_packet, is_last
            )?;
            
            bytes_sent += chunk_packet.len() as u64;
            chunk_count += 1;
            
            if chunk_count % 5 == 0 || is_last {
                info!("Client: sent chunk {}/{} ({:.1}%)", 
                    chunk_count, total_chunks, (chunk_count as f64 / total_chunks as f64) * 100.0);
            }
        }
        
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
                socket.send_to(&out[..len], send_info.to)?;
                sent_any = true;
            }
            
            // Receive ACKs
            socket.set_read_timeout(Some(Duration::from_millis(10)))?;
            if let Ok((len, from)) = socket.recv_from(buf) {
                let recv_info = quiche::RecvInfo { from, to: local_addr };
                let _ = connection.recv(&mut buf[..len], recv_info);
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
        let max_retries = 100;
        let mut retry_count = 0;
        
        while written < data.len() {
            // Check if stream is writable
            let is_writable = connection.writable().any(|id| id == stream_id);
            
            if is_writable {
                match connection.stream_send(stream_id, &data[written..], fin && written >= data.len()) {
                    Ok(w) => {
                        written += w;
                        retry_count = 0; // Reset retry count on success
                    }
                    Err(ref e) if format!("{:?}", e).contains("Done") => {
                        // Stream not ready, continue to flush/receive below
                    }
                    Err(e) => return Err(e),
                }
            }
            
            // Flush packets
            while let Ok((len, send_info)) = connection.send(out) {
                socket.send_to(&out[..len], send_info.to)?;
            }
            
            // Receive ACKs
            socket.set_read_timeout(Some(Duration::from_millis(10)))?;
            if let Ok((len, from)) = socket.recv_from(buf) {
                let recv_info = quiche::RecvInfo { from, to: local_addr };
                let _ = connection.recv(&mut buf[..len], recv_info);
            }
            
            if written < data.len() {
                retry_count += 1;
                if retry_count >= max_retries {
                    return Err(Error::Protocol(format!(
                        "Flow control stall: failed to send after {} retries", max_retries
                    )));
                }
                std::thread::sleep(Duration::from_millis(10));
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
