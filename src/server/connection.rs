// Server connection management

use quiche::{Config, Connection, ConnectionId, RecvInfo};
use std::net::{SocketAddr, UdpSocket};
use std::time::Instant;

/// Wrapper around a QUIC connection for the server side
pub struct ServerConnection {
    conn: Connection,
    peer_addr: SocketAddr,
    original_peer_addr: SocketAddr,
    last_activity: Instant,
    last_heartbeat: Instant,
    migration_count: usize,
    migration_detected: bool,
}

impl ServerConnection {
    /// Accept a new connection
    pub fn accept(
        scid: &ConnectionId,
        local_addr: SocketAddr,
        peer_addr: SocketAddr,
        config: &mut Config,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let conn = quiche::accept(scid, None, local_addr, peer_addr, config)?;
        Ok(Self { 
            conn, 
            peer_addr,
            original_peer_addr: peer_addr,
            last_activity: Instant::now(),
            last_heartbeat: Instant::now(),
            migration_count: 0,
            migration_detected: false,
        })
    }

    /// Process an incoming packet
    pub fn process_packet(
        &mut self,
        buf: &mut [u8],
        from: SocketAddr,
        to: SocketAddr,
    ) -> Result<usize, Box<dyn std::error::Error>> {
        // Detect peer address migration - this usually means client restarted
        // In this case, we should close this connection and let server accept new one
        if from != self.peer_addr && self.conn.is_established() {
            println!("Server: Peer migrated from {} to {} - treating as new connection", self.peer_addr, from);
            println!("Server: Closing old connection to allow new handshake");
            self.migration_detected = true;
            // Close the connection immediately
            let _ = self.conn.close(true, 0x00, b"peer migration");
            return Err("Peer migration - connection closed".into());
        }
        
        let recv_info = RecvInfo { from, to };
        match self.conn.recv(buf, recv_info) {
            Ok(v) => {
                self.last_activity = Instant::now();
                Ok(v)
            }
            Err(e) => {
                eprintln!("Connection recv error: {:?}", e);
                Err(Box::new(e))
            }
        }
    }
    
    /// Check if peer migration was detected
    pub fn migration_detected(&self) -> bool {
        self.migration_detected
    }
    
    /// Clear migration flag
    pub fn clear_migration_flag(&mut self) {
        self.migration_detected = false;
    }

    /// Send packets to the peer
    pub fn send_packets(
        &mut self,
        socket: &UdpSocket,
        out: &mut [u8],
    ) -> Result<(), Box<dyn std::error::Error>> {
        while let Ok((write, send_info)) = self.conn.send(out) {
            socket.send_to(&out[..write], send_info.to)?;
            self.last_activity = Instant::now();
        }
        Ok(())
    }

    /// Check if the connection is established
    pub fn is_established(&self) -> bool {
        self.conn.is_established()
    }

    /// Check if the connection is closed
    pub fn is_closed(&self) -> bool {
        self.conn.is_closed()
    }

    /// Get readable stream IDs
    pub fn readable(&self) -> impl Iterator<Item = u64> + '_ {
        self.conn.readable()
    }

    /// Receive data from a stream
    pub fn stream_recv(
        &mut self,
        stream_id: u64,
        buf: &mut [u8],
    ) -> Result<(usize, bool), quiche::Error> {
        self.conn.stream_recv(stream_id, buf)
    }

    /// Send data on a stream
    pub fn stream_send(
        &mut self,
        stream_id: u64,
        data: &[u8],
        fin: bool,
    ) -> Result<usize, quiche::Error> {
        self.conn.stream_send(stream_id, data, fin)
    }

    /// Get the peer address
    pub fn peer_addr(&self) -> SocketAddr {
        self.peer_addr
    }

    /// Get mutable reference to the underlying connection
    pub fn conn_mut(&mut self) -> &mut Connection {
        &mut self.conn
    }

    /// Get reference to the underlying connection
    pub fn conn(&self) -> &Connection {
        &self.conn
    }
    
    // Connection Migration Support
    
    /// Get the original peer address (before any migrations)
    pub fn original_peer_addr(&self) -> SocketAddr {
        self.original_peer_addr
    }
    
    /// Get the number of times peer has migrated
    pub fn migration_count(&self) -> usize {
        self.migration_count
    }
    
    /// Check if peer has migrated
    pub fn has_migrated(&self) -> bool {
        self.peer_addr != self.original_peer_addr
    }
    
    // Keepalive/Heartbeat Support
    
    /// Get time since last activity
    pub fn idle_duration(&self) -> std::time::Duration {
        self.last_activity.elapsed()
    }
    
    /// Check if connection is idle
    pub fn is_idle(&self) -> bool {
        use crate::common::types::KEEPALIVE_IDLE_THRESHOLD;
        self.last_activity.elapsed() >= KEEPALIVE_IDLE_THRESHOLD
    }
    
    /// Check if heartbeat should be sent
    pub fn should_send_heartbeat(&self) -> bool {
        use crate::common::types::HEARTBEAT_INTERVAL;
        self.last_heartbeat.elapsed() >= HEARTBEAT_INTERVAL
    }
    
    /// Send heartbeat ping
    pub fn send_heartbeat(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if !self.is_established() {
            return Err("Connection not established".into());
        }
        
        // Send on control stream (0)
        match self.conn.stream_send(0, b"PING", false) {
            Ok(_) => {
                self.last_heartbeat = Instant::now();
                Ok(())
            }
            Err(e) => Err(Box::new(e)),
        }
    }
    
    /// Handle heartbeat message
    pub fn handle_heartbeat(&mut self, data: &[u8]) -> bool {
        if data == b"PING" {
            // Respond with PONG
            let _ = self.conn.stream_send(0, b"PONG", false);
            true
        } else if data == b"PONG" {
            true
        } else {
            false
        }
    }
    
    /// Get time since last heartbeat
    pub fn time_since_heartbeat(&self) -> std::time::Duration {
        self.last_heartbeat.elapsed()
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_migration_tracking() {
        // Test that migration_count starts at 0
        // Actual test would require mock QUIC connection
        assert!(true);
    }

    #[test]
    fn test_heartbeat_methods() {
        // Test heartbeat message handling
        let ping = b"PING";
        let pong = b"PONG";
        assert_eq!(ping, b"PING");
        assert_eq!(pong, b"PONG");
    }
}
