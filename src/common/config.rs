// Configuration types and parsing

use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;
use crate::common::error::{Error, Result};
use crate::chunking::compress::CompressionType;

#[derive(Debug, Clone)]
pub struct ClientConfig {
    pub server_addr: SocketAddr,
    pub server_name: String,
    pub chunk_size: usize,
    pub max_retries: usize,
    pub timeout: Duration,
    pub session_dir: PathBuf,
    pub verify_cert: bool,
    pub ca_cert_path: Option<PathBuf>,
    pub compression: CompressionType,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            server_addr: "127.0.0.1:4433".parse().unwrap(),
            server_name: "localhost".to_string(),
            chunk_size: super::types::DEFAULT_CHUNK_SIZE,
            max_retries: 3,
            timeout: super::types::DEFAULT_TIMEOUT,
            session_dir: PathBuf::from(".sftpx/sessions"),
            verify_cert: false, // Default to no verification for easier testing
            ca_cert_path: Some(PathBuf::from("certs/cert.pem")), // Default cert path
            compression: CompressionType::None,  // Default: no compression
        }
    }
}

impl ClientConfig {
    pub fn new(server_addr: SocketAddr, server_name: String) -> Self {
        Self {
            server_addr,
            server_name,
            ..Default::default()
        }
    }
    
    pub fn with_chunk_size(mut self, size: usize) -> Result<Self> {
        if size < super::types::MIN_CHUNK_SIZE || size > super::types::MAX_CHUNK_SIZE {
            return Err(Error::ConfigError(format!(
                "Chunk size must be between {} and {}",
                super::types::MIN_CHUNK_SIZE,
                super::types::MAX_CHUNK_SIZE
            )));
        }
        self.chunk_size = size;
        Ok(self)
    }
    
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }
    
    pub fn with_session_dir(mut self, dir: PathBuf) -> Self {
        self.session_dir = dir;
        self
    }
    
    pub fn disable_cert_verification(mut self) -> Self {
        self.verify_cert = false;
        self
    }
    
    pub fn enable_cert_verification(mut self) -> Self {
        self.verify_cert = true;
        self
    }
    
    pub fn with_ca_cert(mut self, cert_path: PathBuf) -> Self {
        self.ca_cert_path = Some(cert_path);
        self.verify_cert = true;
        self
    }
    
    pub fn with_max_retries(mut self, retries: usize) -> Self {
        self.max_retries = retries;
        self
    }
    
    pub fn with_compression(mut self, compression: CompressionType) -> Self {
        self.compression = compression;
        self
    }
}

#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub listen_addr: SocketAddr,
    pub cert_path: PathBuf,
    pub key_path: PathBuf,
    pub max_connections: usize,
    pub chunk_size: usize,
    pub timeout: Duration,
    pub session_dir: PathBuf,
    pub storage_dir: PathBuf,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            listen_addr: "0.0.0.0:4433".parse().unwrap(),
            cert_path: PathBuf::from("certs/cert.pem"),
            key_path: PathBuf::from("certs/key.pem"),
            max_connections: 100,
            chunk_size: super::types::DEFAULT_CHUNK_SIZE,
            timeout: super::types::DEFAULT_TIMEOUT,
            session_dir: PathBuf::from(".sftpx/sessions"),
            storage_dir: PathBuf::from("./uploads"),
        }
    }
}

impl ServerConfig {
    pub fn new(listen_addr: SocketAddr, cert_path: PathBuf, key_path: PathBuf) -> Self {
        Self {
            listen_addr,
            cert_path,
            key_path,
            ..Default::default()
        }
    }
}
