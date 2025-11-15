// Server connection management

use quiche::{Config, Connection, ConnectionId, RecvInfo};
use std::net::{SocketAddr, UdpSocket};

/// Wrapper around a QUIC connection for the server side
pub struct ServerConnection {
    conn: Connection,
    peer_addr: SocketAddr,
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
        Ok(Self { conn, peer_addr })
    }

    /// Process an incoming packet
    pub fn process_packet(
        &mut self,
        buf: &mut [u8],
        from: SocketAddr,
        to: SocketAddr,
    ) -> Result<usize, Box<dyn std::error::Error>> {
        let recv_info = RecvInfo { from, to };
        match self.conn.recv(buf, recv_info) {
            Ok(v) => Ok(v),
            Err(e) => {
                eprintln!("Connection recv error: {:?}", e);
                Err(Box::new(e))
            }
        }
    }

    /// Send packets to the peer
    pub fn send_packets(
        &mut self,
        socket: &UdpSocket,
        out: &mut [u8],
    ) -> Result<(), Box<dyn std::error::Error>> {
        while let Ok((write, send_info)) = self.conn.send(out) {
            socket.send_to(&out[..write], send_info.to)?;
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
}
