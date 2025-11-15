// Server-side stream handlers

use super::connection::ServerConnection;

const NUM_STREAMS: usize = 4;

/// Types of streams in the server
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamType {
    Control,
    Manifest,
    Data,
    Status,
}

impl StreamType {
    /// Get the stream ID for this type
    pub fn stream_id(&self) -> u64 {
        match self {
            StreamType::Control => 0,
            StreamType::Manifest => 4,
            StreamType::Data => 8,
            StreamType::Status => 12,
        }
    }

    /// Get all stream types
    pub fn all() -> [StreamType; NUM_STREAMS] {
        [
            StreamType::Control,
            StreamType::Manifest,
            StreamType::Data,
            StreamType::Status,
        ]
    }
}

/// Manages multiple streams for a connection
pub struct StreamManager {
    streams: Vec<StreamInfo>,
}

/// Information about a stream
#[derive(Debug, Clone)]
pub struct StreamInfo {
    pub stream_type: StreamType,
    pub stream_id: u64,
    pub bytes_sent: usize,
    pub bytes_received: usize,
    pub is_active: bool,
}

impl StreamManager {
    /// Create a new stream manager
    pub fn new() -> Self {
        Self {
            streams: Vec::new(),
        }
    }

    /// Initialize all 4 streams for a connection
    pub fn initialize_streams(
        &mut self,
        _connection: &mut ServerConnection,
    ) -> Result<(), Box<dyn std::error::Error>> {
        for stream_type in StreamType::all() {
            let stream_id = stream_type.stream_id();
            
            let info = StreamInfo {
                stream_type,
                stream_id,
                bytes_sent: 0,
                bytes_received: 0,
                is_active: true,
            };
            
            self.streams.push(info);
            println!("Initialized stream: {:?} with ID {}", stream_type, stream_id);
        }

        Ok(())
    }

    /// Get stream info by ID
    pub fn get_stream(&self, stream_id: u64) -> Option<&StreamInfo> {
        self.streams.iter().find(|s| s.stream_id == stream_id)
    }

    /// Get mutable stream info by ID
    pub fn get_stream_mut(&mut self, stream_id: u64) -> Option<&mut StreamInfo> {
        self.streams.iter_mut().find(|s| s.stream_id == stream_id)
    }

    /// Update bytes sent for a stream
    pub fn update_bytes_sent(&mut self, stream_id: u64, bytes: usize) {
        if let Some(stream) = self.get_stream_mut(stream_id) {
            stream.bytes_sent += bytes;
        }
    }

    /// Update bytes received for a stream
    pub fn update_bytes_received(&mut self, stream_id: u64, bytes: usize) {
        if let Some(stream) = self.get_stream_mut(stream_id) {
            stream.bytes_received += bytes;
        }
    }

    /// Get all active streams
    pub fn active_streams(&self) -> Vec<&StreamInfo> {
        self.streams.iter().filter(|s| s.is_active).collect()
    }

    /// Mark a stream as inactive
    pub fn deactivate_stream(&mut self, stream_id: u64) {
        if let Some(stream) = self.get_stream_mut(stream_id) {
            stream.is_active = false;
        }
    }

    /// Get total number of streams
    pub fn stream_count(&self) -> usize {
        self.streams.len()
    }

    /// Get statistics for all streams
    pub fn get_statistics(&self) -> StreamStatistics {
        let total_sent: usize = self.streams.iter().map(|s| s.bytes_sent).sum();
        let total_received: usize = self.streams.iter().map(|s| s.bytes_received).sum();
        let active_count = self.active_streams().len();

        StreamStatistics {
            total_streams: self.streams.len(),
            active_streams: active_count,
            total_bytes_sent: total_sent,
            total_bytes_received: total_received,
        }
    }
}

impl Default for StreamManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Statistics for all streams
#[derive(Debug, Clone)]
pub struct StreamStatistics {
    pub total_streams: usize,
    pub active_streams: usize,
    pub total_bytes_sent: usize,
    pub total_bytes_received: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stream_ids() {
        assert_eq!(StreamType::Control.stream_id(), 0);
        assert_eq!(StreamType::Manifest.stream_id(), 4);
        assert_eq!(StreamType::Data.stream_id(), 8);
        assert_eq!(StreamType::Status.stream_id(), 12);
    }

    #[test]
    fn test_stream_manager_creation() {
        let manager = StreamManager::new();
        assert_eq!(manager.stream_count(), 0);
    }
}
