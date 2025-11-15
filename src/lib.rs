// Library root - exports public API

pub mod common;
pub mod client;
pub mod server;  // Uncomment when server is implemented
// pub mod protocol;  // Uncomment when protocol is implemented
// pub mod transport;  // Uncomment when transport is implemented
// pub mod chunking;  // Uncomment when chunking is implemented
// pub mod storage;  // Uncomment when storage is implemented

// Re-export commonly used items
pub use common::{Error, Result, ClientConfig};
pub use client::Client;

pub use server::{Server, ServerConfig, ServerConnection, ServerSession};