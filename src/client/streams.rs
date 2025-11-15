// Client-side stream handlers

use std::collections::HashMap;
use crate::common::error::Result;
use super::connection::ClientConnection;

pub use crate::common::types::StreamType;

/// Stream IDs for the 4 application streams
pub const STREAM_CONTROL: u64 = 0;  // Bidirectional stream 0
pub const STREAM_DATA1: u64 = 4;    // Bidirectional stream 1
pub const STREAM_DATA2: u64 = 8;    // Bidirectional stream 2
pub const STREAM_DATA3: u64 = 12;   // Bidirectional stream 3

pub struct StreamManager {
    streams: HashMap<u64, StreamInfo>,
}

#[derive(Debug, Clone)]
struct StreamInfo {
    stream_id: u64,
    name: String,
    is_finished: bool,
    bytes_sent: u64,
    bytes_received: u64,
}

impl StreamManager {
    pub fn new() -> Self {
        let mut streams = HashMap::new();
        
        // Pre-register the 4 streams
        streams.insert(STREAM_CONTROL, StreamInfo {
            stream_id: STREAM_CONTROL,
            name: "control".to_string(),
            is_finished: false,
            bytes_sent: 0,
            bytes_received: 0,
        });
        
        streams.insert(STREAM_DATA1, StreamInfo {
            stream_id: STREAM_DATA1,
            name: "data1".to_string(),
            is_finished: false,
            bytes_sent: 0,
            bytes_received: 0,
        });
        
        streams.insert(STREAM_DATA2, StreamInfo {
            stream_id: STREAM_DATA2,
            name: "data2".to_string(),
            is_finished: false,
            bytes_sent: 0,
            bytes_received: 0,
        });
        
        streams.insert(STREAM_DATA3, StreamInfo {
            stream_id: STREAM_DATA3,
            name: "data3".to_string(),
            is_finished: false,
            bytes_sent: 0,
            bytes_received: 0,
        });
        
        Self { streams }
    }
    
    /// Set stream priority based on stream type
    pub fn set_stream_priority(&self, conn: &mut ClientConnection, stream_id: u64) -> Result<()> {
        let (urgency, incremental) = match stream_id {
            STREAM_CONTROL => (0, false),  // Highest priority, non-incremental
            STREAM_DATA1 => (3, true),     // Lower priority, incremental
            STREAM_DATA2 => (3, true),     // Lower priority, incremental
            STREAM_DATA3 => (3, true),     // Lower priority, incremental
            _ => (7, true),                // Lowest priority for unknown streams
        };
        
        conn.stream_priority(stream_id, urgency, incremental)?;
        Ok(())
    }
    
    /// Send data on a specific stream
    pub fn send_on_stream(
        &mut self,
        conn: &mut ClientConnection,
        stream_id: u64,
        data: &[u8],
        fin: bool,
    ) -> Result<usize> {
        let written = conn.stream_send(stream_id, data, fin)?;
        
        if let Some(info) = self.streams.get_mut(&stream_id) {
            info.bytes_sent += written as u64;
            if fin {
                info.is_finished = true;
            }
        }
        
        Ok(written)
    }
    
    /// Receive data from a specific stream
    pub fn recv_from_stream(
        &mut self,
        conn: &mut ClientConnection,
        stream_id: u64,
        buf: &mut [u8],
    ) -> Result<(usize, bool)> {
        let (read, fin) = conn.stream_recv(stream_id, buf)?;
        
        if let Some(info) = self.streams.get_mut(&stream_id) {
            info.bytes_received += read as u64;
            if fin {
                info.is_finished = true;
            }
        }
        
        Ok((read, fin))
    }
    
    /// Get stream name
    pub fn get_stream_name(&self, stream_id: u64) -> Option<&str> {
        self.streams.get(&stream_id).map(|info| info.name.as_str())
    }
    
    /// Check if stream is finished
    pub fn is_stream_finished(&self, stream_id: u64) -> bool {
        self.streams
            .get(&stream_id)
            .map(|info| info.is_finished)
            .unwrap_or(false)
    }
    
    /// Get statistics for a stream
    pub fn stream_stats(&self, stream_id: u64) -> Option<(u64, u64)> {
        self.streams
            .get(&stream_id)
            .map(|info| (info.bytes_sent, info.bytes_received))
    }
    
    /// Initialize all streams with proper priorities
    pub fn initialize_streams(&self, conn: &mut ClientConnection) -> Result<()> {
        for stream_id in [STREAM_CONTROL, STREAM_DATA1, STREAM_DATA2, STREAM_DATA3] {
            self.set_stream_priority(conn, stream_id)?;
        }
        Ok(())
    }
    
    /// Get all stream IDs
    pub fn get_all_stream_ids(&self) -> Vec<u64> {
        vec![STREAM_CONTROL, STREAM_DATA1, STREAM_DATA2, STREAM_DATA3]
    }
}
