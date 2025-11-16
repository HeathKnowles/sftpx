// Common type definitions and constants

use std::time::Duration;
use serde::{Serialize, Deserialize};

pub type SessionId = String;
pub type ChunkId = u64;
pub type StreamId = u64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StreamType {
    Control,
    Manifest,
    Data,
    Status,
}

impl StreamType {
    pub fn to_stream_id(&self) -> StreamId {
        match self {
            StreamType::Control => 0,
            StreamType::Manifest => 4,
            StreamType::Data => 8,
            StreamType::Status => 12,
        }
    }
    
    pub fn from_stream_id(id: StreamId) -> Option<Self> {
        match id {
            0 => Some(StreamType::Control),
            4 => Some(StreamType::Manifest),
            8 => Some(StreamType::Data),
            12 => Some(StreamType::Status),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TransferDirection {
    Send,
    Receive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TransferState {
    Initializing,
    Handshaking,
    SendingManifest,
    ReceivingManifest,
    Transferring,
    Resuming,
    Completing,
    Completed,
    Failed,
    Cancelled,
}

// Constants
pub const DEFAULT_CHUNK_SIZE: usize = 1024 * 1024; // 1MB
pub const MAX_CHUNK_SIZE: usize = 10 * 1024 * 1024; // 10MB
pub const MIN_CHUNK_SIZE: usize = 64 * 1024; // 64KB
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);
pub const IDLE_TIMEOUT: Duration = Duration::from_secs(300);
pub const MAX_DATAGRAM_SIZE: usize = 1350;
pub const PROTOCOL_VERSION: &str = "sftpx/0.1";
pub const MAX_STREAM_WINDOW: u64 = 100 * 1024 * 1024; // 100MB - increased for faster transfers

// Keepalive/Heartbeat constants
pub const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(30); // Send heartbeat every 30s
pub const KEEPALIVE_IDLE_THRESHOLD: Duration = Duration::from_secs(60); // Consider idle after 60s
