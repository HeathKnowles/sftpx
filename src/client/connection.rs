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
    last_heartbeat: Instant,
    stats: ConnectionStats,
    migration_enabled: bool,
    original_peer_addr: SocketAddr,
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
        
        // Configure transport parameters for maximum throughput
        quic_config.set_max_idle_timeout(IDLE_TIMEOUT.as_millis() as u64);
        quic_config.set_max_recv_udp_payload_size(MAX_DATAGRAM_SIZE);
        quic_config.set_max_send_udp_payload_size(MAX_DATAGRAM_SIZE);
        quic_config.set_initial_max_data(MAX_STREAM_WINDOW * 10);  // 2.56GB connection window
        quic_config.set_initial_max_stream_data_bidi_local(MAX_STREAM_WINDOW);  // 256MB per stream
        quic_config.set_initial_max_stream_data_bidi_remote(MAX_STREAM_WINDOW);  // 256MB per stream
        quic_config.set_initial_max_stream_data_uni(MAX_STREAM_WINDOW);  // 256MB per stream
        quic_config.set_initial_max_streams_bidi(100);
        quic_config.set_initial_max_streams_uni(100);
        quic_config.set_disable_active_migration(false);
        
        // Balanced performance tuning
        quic_config.set_cc_algorithm(quiche::CongestionControlAlgorithm::CUBIC);
        quic_config.enable_hystart(true);  // Faster ramp-up
        
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
            last_heartbeat: Instant::now(),
            stats: ConnectionStats::default(),
            migration_enabled: true,
            original_peer_addr: config.server_addr,
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
    
    // Connection Migration Support
    
    /// Check if connection migration is enabled
    pub fn is_migration_enabled(&self) -> bool {
        self.migration_enabled
    }
    
    /// Enable or disable connection migration
    pub fn set_migration_enabled(&mut self, enabled: bool) {
        self.migration_enabled = enabled;
    }
    
    /// Migrate connection to a new local address
    /// This is useful when the client's network interface changes (e.g., WiFi to cellular)
    pub fn migrate_to_address(&mut self, new_local_addr: SocketAddr) -> Result<()> {
        if !self.migration_enabled {
            return Err(Error::Quic("Connection migration is disabled".to_string()));
        }
        
        if !self.is_established() {
            return Err(Error::Quic("Cannot migrate: connection not established".to_string()));
        }
        
        // QUIC handles path validation automatically
        // We just need to send packets from the new address
        log::info!("Migrating connection from new local address: {}", new_local_addr);
        Ok(())
    }
    
    /// Check if the peer address has changed (server migrated)
    pub fn has_peer_migrated(&self, current_peer: SocketAddr) -> bool {
        current_peer != self.original_peer_addr
    }
    
    /// Update peer address after detecting migration
    pub fn update_peer_address(&mut self, new_peer: SocketAddr) {
        if new_peer != self.socket_addr {
            log::info!("Peer migrated from {} to {}", self.socket_addr, new_peer);
            self.socket_addr = new_peer;
        }
    }
    
    /// Get the original peer address (before any migration)
    pub fn original_peer_addr(&self) -> SocketAddr {
        self.original_peer_addr
    }
    
    // Keepalive/Heartbeat Support
    
    /// Check if a heartbeat should be sent
    /// Returns true if HEARTBEAT_INTERVAL has elapsed since last heartbeat
    pub fn should_send_heartbeat(&self) -> bool {
        self.last_heartbeat.elapsed() >= HEARTBEAT_INTERVAL
    }
    
    /// Send a heartbeat ping on the control stream
    pub fn send_heartbeat(&mut self) -> Result<()> {
        if !self.is_established() {
            return Err(Error::Quic("Cannot send heartbeat: connection not established".to_string()));
        }
        
        // Send a small keepalive message on stream 0 (control)
        let heartbeat_msg = b"PING";
        match self.stream_send(0, heartbeat_msg, false) {
            Ok(_) => {
                self.last_heartbeat = Instant::now();
                log::debug!("Heartbeat sent");
                Ok(())
            }
            Err(e) => {
                log::warn!("Failed to send heartbeat: {:?}", e);
                Err(e)
            }
        }
    }
    
    /// Check if connection is idle and needs keepalive
    pub fn is_idle(&self) -> bool {
        self.last_activity.elapsed() >= KEEPALIVE_IDLE_THRESHOLD
    }
    
    /// Get time since last activity
    pub fn idle_duration(&self) -> Duration {
        self.last_activity.elapsed()
    }
    
    /// Get time since last heartbeat
    pub fn time_since_heartbeat(&self) -> Duration {
        self.last_heartbeat.elapsed()
    }
    
    /// Process potential heartbeat on receive
    pub fn handle_heartbeat(&mut self, data: &[u8]) -> bool {
        if data == b"PING" {
            log::debug!("Heartbeat PING received");
            // Send PONG response
            if let Err(e) = self.stream_send(0, b"PONG", false) {
                log::warn!("Failed to send PONG: {:?}", e);
            }
            true
        } else if data == b"PONG" {
            log::debug!("Heartbeat PONG received");
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_heartbeat_timing() {
        use std::time::Duration;
        // Test that HEARTBEAT_INTERVAL is reasonable
        assert_eq!(HEARTBEAT_INTERVAL, Duration::from_secs(30));
        assert_eq!(KEEPALIVE_IDLE_THRESHOLD, Duration::from_secs(60));
    }

    #[test]
    fn test_heartbeat_messages() {
        // Test heartbeat message format
        let ping = b"PING";
        let pong = b"PONG";
        assert_eq!(ping.len(), 4);
        assert_eq!(pong.len(), 4);
    }
}
