// Server module - QUIC server implementation

mod connection;
mod session;
mod streams;
mod sender;
mod transfer;

pub use connection::ServerConnection;
pub use session::ServerSession;
pub use streams::{StreamManager, StreamType};
pub use sender::DataSender;
pub use transfer::TransferManager;

use quiche::Config;
use std::net::UdpSocket;

const MAX_DATAGRAM_SIZE: usize = 1350;
const NUM_STREAMS_PER_CONNECTION: usize = 4;

/// Server configuration
pub struct ServerConfig {
    pub bind_addr: String,
    pub cert_path: String,
    pub key_path: String,
    pub max_idle_timeout: u64,
    pub max_data: u64,
    pub max_stream_data: u64,
    pub max_streams: u64,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            bind_addr: "127.0.0.1:4443".to_string(),
            cert_path: "certs/cert.pem".to_string(),
            key_path: "certs/key.pem".to_string(),
            max_idle_timeout: 5000,
            max_data: 2_560_000_000,  // 2.56GB connection window for parallel processing
            max_stream_data: 268_435_456,  // 256MB per stream for parallel processing
            max_streams: 1000,  // Increased for parallel chunk transfers
        }
    }
}

/// Main QUIC server
pub struct Server {
    config: ServerConfig,
    socket: UdpSocket,
    quic_config: Config,
}

impl Server {
    /// Create a new server instance
    pub fn new(config: ServerConfig) -> Result<Self, Box<dyn std::error::Error>> {
        let socket = UdpSocket::bind(&config.bind_addr)?;
        println!("Server listening on {}", config.bind_addr);

        let mut quic_config = Config::new(quiche::PROTOCOL_VERSION)?;
        quic_config.set_application_protos(&[b"sftpx/0.1"])?;
        quic_config.verify_peer(false);
        quic_config.set_max_idle_timeout(config.max_idle_timeout);
        quic_config.set_max_recv_udp_payload_size(MAX_DATAGRAM_SIZE);
        quic_config.set_max_send_udp_payload_size(MAX_DATAGRAM_SIZE);
        quic_config.set_initial_max_data(config.max_data);
        quic_config.set_initial_max_stream_data_bidi_local(config.max_stream_data);
        quic_config.set_initial_max_stream_data_bidi_remote(config.max_stream_data);
        quic_config.set_initial_max_streams_bidi(config.max_streams);

        // Load server certificate and private key
        quic_config.load_cert_chain_from_pem_file(&config.cert_path)?;
        quic_config.load_priv_key_from_pem_file(&config.key_path)?;

        Ok(Self {
            config,
            socket,
            quic_config,
        })
    }

    /// Run the server and accept connections
    pub fn run(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let mut buf = [0u8; MAX_DATAGRAM_SIZE];
        let mut out = [0u8; MAX_DATAGRAM_SIZE];

        loop {
            println!("Server: waiting for initial packet...");
            let (len, from) = self.socket.recv_from(&mut buf)?;
            println!("Server: received initial packet ({} bytes) from {}", len, from);

            let mut hdr_buf = &mut buf[..len];
            let hdr = match quiche::Header::from_slice(&mut hdr_buf, quiche::MAX_CONN_ID_LEN) {
                Ok(h) => h,
                Err(e) => {
                    eprintln!("Failed to parse header: {:?}", e);
                    continue;
                }
            };

            // Create server connection
            let scid = quiche::ConnectionId::from_ref(&hdr.dcid);
            let mut server_conn = ServerConnection::accept(
                &scid,
                self.socket.local_addr()?,
                from,
                &mut self.quic_config,
            )?;
            println!("Server: connection accepted");

            // Process initial packet
            server_conn.process_packet(&mut buf[..len], from, self.socket.local_addr()?)?;
            println!("Server: initial packet processed");

            // Send handshake response packets
            server_conn.send_packets(&self.socket, &mut out)?;
            println!("Server: sent handshake response");

            // Handle the connection session (this will complete handshake and handle data)
            match self.handle_session(&mut server_conn, &mut buf, &mut out) {
                Ok(_) => println!("Server: session completed successfully"),
                Err(e) => eprintln!("Server: session error: {:?}", e),
            }

            println!("Server: connection closed, ready for next connection\n");
            // Continue loop to accept next connection
        }
    }

    /// Handle a complete session with a client
    fn handle_session(
        &self,
        conn: &mut ServerConnection,
        buf: &mut [u8],
        out: &mut [u8],
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut session = ServerSession::new(conn);
        session.run(&self.socket, buf, out)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_server_config_default() {
        let config = ServerConfig::default();
        assert_eq!(config.bind_addr, "127.0.0.1:4443");
        assert_eq!(config.cert_path, "certs/cert.pem");
    }
}
