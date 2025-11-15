// Client-side file receiving logic

use std::path::{Path, PathBuf};
use std::fs::{File, OpenOptions};
use std::io::{Write, Seek, SeekFrom};
use crate::common::error::{Error, Result};
use crate::common::types::*;
use super::session::ClientSession;

pub struct FileReceiver {
    session: ClientSession,
    part_file: File,
    part_file_path: PathBuf,
    final_file_path: PathBuf,
    receive_buffer: Vec<u8>,
}

impl FileReceiver {
    pub fn new(session: ClientSession, output_dir: &Path) -> Result<Self> {
        let filename = session.file_path
            .file_name()
            .ok_or_else(|| Error::Protocol("Invalid file path".to_string()))?;
        
        let final_file_path = output_dir.join(filename);
        let part_file_path = output_dir.join(format!("{}.part", filename.to_str().unwrap()));
        
        // Create or open .part file
        let part_file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(&part_file_path)?;
        
        // Pre-allocate space
        part_file.set_len(session.file_size)?;
        
        Ok(Self {
            session,
            part_file,
            part_file_path,
            final_file_path,
            receive_buffer: vec![0u8; DEFAULT_CHUNK_SIZE],
        })
    }
    
    /// Receive and process a chunk packet
    pub fn receive_chunk(&mut self, chunk_data: &[u8]) -> Result<ChunkId> {
        // Parse chunk packet (simplified - should use protobuf)
        if chunk_data.len() < 20 {
            return Err(Error::Protocol("Chunk packet too small".to_string()));
        }
        
        let chunk_id = u64::from_le_bytes(chunk_data[0..8].try_into().unwrap());
        let offset = u64::from_le_bytes(chunk_data[8..16].try_into().unwrap());
        let size = u32::from_le_bytes(chunk_data[16..20].try_into().unwrap()) as usize;
        let data = &chunk_data[20..];
        
        if data.len() != size {
            return Err(Error::Protocol("Chunk size mismatch".to_string()));
        }
        
        // Write chunk to file at correct offset
        self.part_file.seek(SeekFrom::Start(offset))?;
        self.part_file.write_all(data)?;
        self.part_file.flush()?;
        
        // Mark chunk as received
        self.session.mark_chunk_acknowledged(chunk_id)?;
        
        Ok(chunk_id)
    }
    
    /// Finalize the transfer
    pub fn finalize(&mut self) -> Result<()> {
        if !self.session.is_complete() {
            return Err(Error::Protocol("Transfer not complete".to_string()));
        }
        
        // Flush and close
        self.part_file.flush()?;
        drop(std::mem::replace(&mut self.part_file, File::open("/dev/null")?));
        
        // Atomic rename
        std::fs::rename(&self.part_file_path, &self.final_file_path)?;
        
        Ok(())
    }
    
    pub fn session(&self) -> &ClientSession {
        &self.session
    }
    
    pub fn progress(&self) -> f64 {
        self.session.progress()
    }
}
