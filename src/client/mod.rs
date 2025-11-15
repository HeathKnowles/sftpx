// Client module - QUIC client implementation

mod connection;
mod streams;
mod session;
mod receiver;
mod transfer;

pub use connection::ClientConnection;
pub use streams::{StreamManager, StreamType};
pub use session::ClientSession;
pub use receiver::FileReceiver;
pub use transfer::Transfer;

use crate::common::error::Result;
use crate::common::config::ClientConfig;

/// Main client interface
pub struct Client {
    config: ClientConfig,
}

impl Client {
    pub fn new(config: ClientConfig) -> Self {
        Self { config }
    }
    
    pub fn send_file(&self, file_path: &str, destination: &str) -> Result<Transfer> {
        Transfer::send_file(self.config.clone(), file_path, destination)
    }
    
    pub fn receive_file(&self, session_id: &str) -> Result<Transfer> {
        Transfer::receive_file(self.config.clone(), session_id)
    }
    
    pub fn resume_transfer(&self, session_id: &str) -> Result<Transfer> {
        Transfer::resume(self.config.clone(), session_id)
    }
}
