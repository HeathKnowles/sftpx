// Client connection management

use quiche;
use std::net::SocketAddr;
use std::time::{Duration, Instant};
use ring::rand::SecureRandom;
use crate::common::error::{Error, Result};
use crate::common::config::ClientConfig;
use crate::common::types::*;

pub struct ClientConnection {
    conn: quiche::Connection,
    socket_addr: SocketAddr,
    server_name: String,
    last_activity: Instant,
    stats: ConnectionStats,
}

#[derive(Debug, Default)]
pub struct ConnectionStats {
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub packets_sent: u64,
    pub packets_received: u64,
}

impl ClientConnection {
    pub fn new(
        config: &ClientConfig,
        local_addr: SocketAddr,
    ) -> Result<Self> {
        // Generate random connection ID
        let mut scid = [0u8; quiche::MAX_CONN_ID_LEN];
        ring::rand::SystemRandom::new()
            .fill(&mut scid)
            .map_err(|_| Error::Quic("Failed to generate connection ID".to_string()))?;
        let scid = quiche::ConnectionId::from_ref(&scid);
        
        // Configure QUIC
        let mut quic_config = quiche::Config::new(quiche::PROTOCOL_VERSION)
            .map_err(|e| Error::Quic(format!("Failed to create config: {:?}", e)))?;
        
        // Set application protocol
        quic_config
            .set_application_protos(&[PROTOCOL_VERSION.as_bytes()])
            .map_err(|e| Error::Quic(format!("Failed to set application protos: {:?}", e)))?;
        
        // Configure transport parameters
        quic_config.set_max_idle_timeout(IDLE_TIMEOUT.as_millis() as u64);
        quic_config.set_max_recv_udp_payload_size(MAX_DATAGRAM_SIZE);
        quic_config.set_max_send_udp_payload_size(MAX_DATAGRAM_SIZE);
        quic_config.set_initial_max_data(MAX_STREAM_WINDOW * 10);
        quic_config.set_initial_max_stream_data_bidi_local(MAX_STREAM_WINDOW);
        quic_config.set_initial_max_stream_data_bidi_remote(MAX_STREAM_WINDOW);
        quic_config.set_initial_max_stream_data_uni(MAX_STREAM_WINDOW);
        quic_config.set_initial_max_streams_bidi(100);
        quic_config.set_initial_max_streams_uni(100);
        quic_config.set_disable_active_migration(false);
        
        // TLS verification
        if !config.verify_cert {
            quic_config.verify_peer(false);
        }
        
        if let Some(ca_path) = &config.ca_cert_path {
            quic_config
                .load_verify_locations_from_file(ca_path.to_str().unwrap())
                .map_err(|e| Error::TlsError(format!("Failed to load CA cert: {:?}", e)))?;
        }
        
        // Create connection
        let conn = quiche::connect(
            Some(&config.server_name),
            &scid,
            local_addr,
            config.server_addr,
            &mut quic_config,
        )
        .map_err(|e| Error::Quic(format!("Failed to create connection: {:?}", e)))?;
        
        Ok(Self {
            conn,
            socket_addr: config.server_addr,
            server_name: config.server_name.clone(),
            last_activity: Instant::now(),
            stats: ConnectionStats::default(),
        })
    }
    
    pub fn send(&mut self, out: &mut [u8]) -> Result<(usize, quiche::SendInfo)> {
        match self.conn.send(out) {
            Ok((written, send_info)) => {
                self.stats.bytes_sent += written as u64;
                self.stats.packets_sent += 1;
                self.last_activity = Instant::now();
                Ok((written, send_info))
            }
            Err(quiche::Error::Done) => Err(Error::Quic("No data to send".to_string())),
            Err(e) => Err(Error::Quic(format!("Send error: {:?}", e))),
        }
    }
    
    pub fn recv(&mut self, buf: &mut [u8], recv_info: quiche::RecvInfo) -> Result<usize> {
        match self.conn.recv(buf, recv_info) {
            Ok(read) => {
                self.stats.bytes_received += read as u64;
                self.stats.packets_received += 1;
                self.last_activity = Instant::now();
                Ok(read)
            }
            Err(quiche::Error::Done) => Ok(0),
            Err(e) => Err(Error::Quic(format!("Receive error: {:?}", e))),
        }
    }
    
    pub fn stream_send(&mut self, stream_id: u64, data: &[u8], fin: bool) -> Result<usize> {
        self.conn
            .stream_send(stream_id, data, fin)
            .map_err(|e| Error::Quic(format!("Stream send error on {}: {:?}", stream_id, e)))
    }
    
    pub fn stream_recv(&mut self, stream_id: u64, out: &mut [u8]) -> Result<(usize, bool)> {
        self.conn
            .stream_recv(stream_id, out)
            .map_err(|e| {
                if matches!(e, quiche::Error::Done) {
                    Error::Quic("Stream done".to_string())
                } else {
                    Error::Quic(format!("Stream recv error on {}: {:?}", stream_id, e))
                }
            })
    }
    
    pub fn stream_priority(&mut self, stream_id: u64, urgency: u8, incremental: bool) -> Result<()> {
        self.conn
            .stream_priority(stream_id, urgency, incremental)
            .map_err(|e| Error::Quic(format!("Stream priority error on {}: {:?}", stream_id, e)))
    }
    
    pub fn readable(&self) -> impl Iterator<Item = u64> + '_ {
        self.conn.readable()
    }
    
    pub fn writable(&self) -> impl Iterator<Item = u64> + '_ {
        self.conn.writable()
    }
    
    pub fn timeout(&self) -> Option<Duration> {
        self.conn.timeout()
    }
    
    pub fn on_timeout(&mut self) {
        self.conn.on_timeout();
    }
    
    pub fn is_established(&self) -> bool {
        self.conn.is_established()
    }
    
    pub fn is_closed(&self) -> bool {
        self.conn.is_closed()
    }
    
    pub fn peer_streams_left_bidi(&self) -> u64 {
        self.conn.peer_streams_left_bidi()
    }
    
    pub fn close(&mut self, app: bool, err: u64, reason: &[u8]) -> Result<()> {
        self.conn
            .close(app, err, reason)
            .map_err(|e| Error::Quic(format!("Close error: {:?}", e)))
    }
    
    pub fn stats(&self) -> &ConnectionStats {
        &self.stats
    }
    
    pub fn quic_stats(&self) -> quiche::Stats {
        self.conn.stats()
    }
    
    pub fn last_activity(&self) -> Instant {
        self.last_activity
    }
    
    pub fn server_addr(&self) -> SocketAddr {
        self.socket_addr
    }
}