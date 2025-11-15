// Client module - QUIC client implementation

mod connection;
mod streams;
mod session;
pub mod receiver;
mod sender;
pub mod transfer;

pub use connection::ClientConnection;
pub use streams::{StreamManager, StreamType};
pub use session::ClientSession;
pub use receiver::FileReceiver;
pub use sender::DataSender;
pub use transfer::Transfer;

use crate::common::error::Result;
use crate::common::config::ClientConfig;

/// Main client interface
pub struct Client {
    config: ClientConfig,
}

impl Client {
    /// Create a new client with the given configuration
    pub fn new(config: ClientConfig) -> Self {
        Self { config }
    }
    
    /// Create a client with default configuration
    /// Uses localhost:4433 and certs/cert.pem for TLS
    pub fn default() -> Self {
        Self {
            config: ClientConfig::default(),
        }
    }
    
    /// Create a client with default settings for the given server address
    /// Uses certs/cert.pem for TLS verification
    pub fn with_defaults(server_addr: &str) -> Result<Self> {
        let addr = server_addr.parse()
            .map_err(|e: std::net::AddrParseError| crate::common::error::Error::ConfigError(format!("Invalid address: {}", e)))?;
        
        let config = ClientConfig::new(addr, "localhost".to_string())
            .with_ca_cert(std::path::PathBuf::from("certs/cert.pem"));
        
        Ok(Self { config })
    }
    
    /// Send a file to the server
    pub fn send_file(&self, file_path: &str, destination: &str) -> Result<Transfer> {
        Transfer::send_file(self.config.clone(), file_path, destination)
    }
    
    /// Receive a file from the server
    pub fn receive_file(&self, session_id: &str) -> Result<Transfer> {
        Transfer::receive_file(self.config.clone(), session_id)
    }
    
    /// Resume a previous transfer
    pub fn resume_transfer(&self, session_id: &str) -> Result<Transfer> {
        Transfer::resume(self.config.clone(), session_id)
    }
    
    /// Get the current client configuration
    pub fn config(&self) -> &ClientConfig {
        &self.config
    }
}
