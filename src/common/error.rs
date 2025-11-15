// Error types and error handling

use std::io;
use std::fmt;

#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    Quic(String),
    Protocol(String),
    InvalidManifest(String),
    HashMismatch { expected: Vec<u8>, actual: Vec<u8> },
    ChunkNotFound(u64),
    SessionNotFound(String),
    InvalidChunkSize,
    InvalidOffset,
    TransferTimeout,
    ConnectionClosed,
    StreamError(u64),
    SerializationError(String),
    DeserializationError(String),
    FileNotFound(String),
    PermissionDenied(String),
    DiskFull,
    ConfigError(String),
    TlsError(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(e) => write!(f, "IO error: {}", e),
            Error::Quic(e) => write!(f, "QUIC error: {}", e),
            Error::Protocol(e) => write!(f, "Protocol error: {}", e),
            Error::InvalidManifest(e) => write!(f, "Invalid manifest: {}", e),
            Error::HashMismatch { expected, actual } => {
                write!(f, "Hash mismatch: expected {:?}, got {:?}", expected, actual)
            },
            Error::ChunkNotFound(id) => write!(f, "Chunk {} not found", id),
            Error::SessionNotFound(id) => write!(f, "Session {} not found", id),
            Error::InvalidChunkSize => write!(f, "Invalid chunk size"),
            Error::InvalidOffset => write!(f, "Invalid file offset"),
            Error::TransferTimeout => write!(f, "Transfer timeout"),
            Error::ConnectionClosed => write!(f, "Connection closed"),
            Error::StreamError(id) => write!(f, "Stream {} error", id),
            Error::SerializationError(e) => write!(f, "Serialization error: {}", e),
            Error::DeserializationError(e) => write!(f, "Deserialization error: {}", e),
            Error::FileNotFound(path) => write!(f, "File not found: {}", path),
            Error::PermissionDenied(path) => write!(f, "Permission denied: {}", path),
            Error::DiskFull => write!(f, "Disk full"),
            Error::ConfigError(e) => write!(f, "Configuration error: {}", e),
            Error::TlsError(e) => write!(f, "TLS error: {}", e),
        }
    }
}

impl std::error::Error for Error {}

impl From<io::Error> for Error {
    fn from(err: io::Error) -> Self {
        Error::Io(err)
    }
}

impl From<quiche::Error> for Error {
    fn from(err: quiche::Error) -> Self {
        Error::Quic(format!("{:?}", err))
    }
}

pub type Result<T> = std::result::Result<T, Error>;
