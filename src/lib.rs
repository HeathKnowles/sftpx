pub mod common;
pub mod client;
pub mod server;
pub mod protocol;
pub mod proto;
pub mod chunking;
// pub mod transport;  // Uncomment when transport is implemented
// pub mod storage;  // Uncomment when storage is implemented
pub mod logging;
pub mod resumption;
pub mod retransmission;
pub mod validation;

// Re-export commonly used items
pub use common::{Error, Result, ClientConfig};
pub use client::Client;

pub use server::{Server, ServerConfig, ServerConnection, ServerSession};