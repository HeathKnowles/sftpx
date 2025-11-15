// Client-side transfer logic

use std::net::UdpSocket;
use std::time::{Duration, Instant};
use std::path::PathBuf;
use log::{info, debug, error};
use crate::common::error::{Error, Result};
use crate::common::config::ClientConfig;
use crate::common::types::*;
use super::connection::ClientConnection;
use super::streams::{StreamManager, STREAM_CONTROL, STREAM_DATA1, STREAM_DATA2, STREAM_DATA3};
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
        let handshake_start = Instant::now();
        let handshake_timeout = Duration::from_secs(10);
        
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
            
            if handshake_start.elapsed() > handshake_timeout {
                return Err(Error::TransferTimeout);
            }
            
            std::thread::sleep(Duration::from_millis(10));
        }
        
        // Initialize stream priorities
        self.stream_manager.initialize_streams(&mut connection)?;
        info!("Client: initialized 4 streams (control, data1, data2, data3)");
        
        // --- APPLICATION DATA PHASE ---
        info!("Client: sending application messages on 4 streams...");
        self.state = TransferState::Transferring;
        
        // Send test messages on each stream
        let messages: Vec<(u64, &[u8])> = vec![
            (STREAM_CONTROL, b"Control message from client"),
            (STREAM_DATA1, b"Data1 message from client"),
            (STREAM_DATA2, b"Data2 message from client"),
            (STREAM_DATA3, b"Data3 message from client"),
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
        let start = Instant::now();
        let timeout = Duration::from_secs(10);
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
            
            if start.elapsed() > timeout {
                info!("Client: timeout (received from {} streams)", received_streams.len());
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
