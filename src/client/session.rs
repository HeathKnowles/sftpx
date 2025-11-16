// Client session management and resumption

use std::path::{Path, PathBuf};
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};
use serde::{Serialize, Deserialize};
use ring::rand::SecureRandom;
use crate::common::error::{Error, Result};
use crate::common::types::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientSession {
    pub session_id: SessionId,
    pub file_path: PathBuf,
    pub file_size: u64,
    pub chunk_size: usize,
    pub total_chunks: u64,
    pub destination: String,
    pub direction: TransferDirection,
    pub state: TransferState,
    pub chunks_sent: Vec<bool>,
    pub chunks_acknowledged: Vec<bool>,
    pub created_at: u64,
    pub updated_at: u64,
}

impl ClientSession {
    pub fn new(
        file_path: PathBuf,
        file_size: u64,
        chunk_size: usize,
        destination: String,
        direction: TransferDirection,
    ) -> Self {
        let total_chunks = (file_size + chunk_size as u64 - 1) / chunk_size as u64;
        let session_id = Self::generate_session_id();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        
        Self {
            session_id,
            file_path,
            file_size,
            chunk_size,
            total_chunks,
            destination,
            direction,
            state: TransferState::Initializing,
            chunks_sent: vec![false; total_chunks as usize],
            chunks_acknowledged: vec![false; total_chunks as usize],
            created_at: now,
            updated_at: now,
        }
    }
    
    fn generate_session_id() -> SessionId {
        use std::fmt::Write;
        let mut id = String::with_capacity(32);
        let random_bytes: [u8; 16] = {
            let mut bytes = [0u8; 16];
            ring::rand::SystemRandom::new()
                .fill(&mut bytes)
                .unwrap();
            bytes
        };
        
        for byte in &random_bytes {
            write!(&mut id, "{:02x}", byte).unwrap();
        }
        id
    }
    
    pub fn mark_chunk_sent(&mut self, chunk_id: ChunkId) -> Result<()> {
        if chunk_id >= self.total_chunks {
            return Err(Error::ChunkNotFound(chunk_id));
        }
        self.chunks_sent[chunk_id as usize] = true;
        self.update_timestamp();
        Ok(())
    }
    
    pub fn mark_chunk_acknowledged(&mut self, chunk_id: ChunkId) -> Result<()> {
        if chunk_id >= self.total_chunks {
            return Err(Error::ChunkNotFound(chunk_id));
        }
        self.chunks_acknowledged[chunk_id as usize] = true;
        self.update_timestamp();
        Ok(())
    }
    
    pub fn get_missing_chunks(&self) -> Vec<ChunkId> {
        self.chunks_acknowledged
            .iter()
            .enumerate()
            .filter_map(|(id, &acked)| {
                if !acked {
                    Some(id as ChunkId)
                } else {
                    None
                }
            })
            .collect()
    }
    
    pub fn progress(&self) -> f64 {
        let acknowledged = self.chunks_acknowledged.iter().filter(|&&x| x).count();
        (acknowledged as f64 / self.total_chunks as f64) * 100.0
    }
    
    pub fn is_complete(&self) -> bool {
        self.chunks_acknowledged.iter().all(|&x| x)
    }
    
    pub fn update_state(&mut self, state: TransferState) {
        self.state = state;
        self.update_timestamp();
    }
    
    fn update_timestamp(&mut self) {
        self.updated_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
    }
    
    /// Save session to disk
    pub fn save(&self, session_dir: &Path) -> Result<()> {
        fs::create_dir_all(session_dir)?;
        let session_file = session_dir.join(format!("{}.json", self.session_id));
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| Error::SerializationError(e.to_string()))?;
        fs::write(session_file, json)?;
        Ok(())
    }
    
    /// Load session from disk
    pub fn load(session_dir: &Path, session_id: &str) -> Result<Self> {
        let session_file = session_dir.join(format!("{}.json", session_id));
        if !session_file.exists() {
            return Err(Error::SessionNotFound(session_id.to_string()));
        }
        
        let json = fs::read_to_string(session_file)?;
        let session: Self = serde_json::from_str(&json)
            .map_err(|e| Error::DeserializationError(e.to_string()))?;
        Ok(session)
    }
}
