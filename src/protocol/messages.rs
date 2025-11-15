// Protocol message types and enums

use prost::Message;

/// Session initialization message
#[derive(Clone, PartialEq, Message)]
pub struct SessionStart {
    /// Unique session identifier
    #[prost(string, tag = "1")]
    pub session_id: String,
    
    /// File path on sender
    #[prost(string, tag = "2")]
    pub file_path: String,
    
    /// Total file size in bytes
    #[prost(uint64, tag = "3")]
    pub file_size: u64,
    
    /// Chunk size to use for transfer
    #[prost(uint32, tag = "4")]
    pub chunk_size: u32,
    
    /// Total number of chunks
    #[prost(uint64, tag = "5")]
    pub total_chunks: u64,
    
    /// Compression algorithm used
    #[prost(string, tag = "6")]
    pub compression: String,
    
    /// Optional file metadata (JSON-encoded)
    #[prost(string, optional, tag = "7")]
    pub metadata: Option<String>,
}

/// File manifest with chunk information
#[derive(Clone, PartialEq, Message)]
pub struct Manifest {
    /// Session ID this manifest belongs to
    #[prost(string, tag = "1")]
    pub session_id: String,
    
    /// File name
    #[prost(string, tag = "2")]
    pub file_name: String,
    
    /// File size
    #[prost(uint64, tag = "3")]
    pub file_size: u64,
    
    /// Chunk size
    #[prost(uint32, tag = "4")]
    pub chunk_size: u32,
    
    /// Total chunks
    #[prost(uint64, tag = "5")]
    pub total_chunks: u64,
    
    /// File hash (BLAKE3)
    #[prost(bytes, tag = "6")]
    pub file_hash: Vec<u8>,
    
    /// Chunk hashes (BLAKE3 for each chunk)
    #[prost(bytes, repeated, tag = "7")]
    pub chunk_hashes: Vec<Vec<u8>>,
    
    /// Compression algorithm
    #[prost(string, tag = "8")]
    pub compression: String,
    
    /// Original file size (before compression)
    #[prost(uint64, optional, tag = "9")]
    pub original_size: Option<u64>,
}

/// Individual chunk packet
#[derive(Clone, PartialEq, Message)]
pub struct ChunkPacket {
    /// Session ID
    #[prost(string, tag = "1")]
    pub session_id: String,
    
    /// Chunk number/ID
    #[prost(uint64, tag = "2")]
    pub chunk_id: u64,
    
    /// Offset in file where this chunk starts
    #[prost(uint64, tag = "3")]
    pub offset: u64,
    
    /// Chunk data (possibly compressed)
    #[prost(bytes, tag = "4")]
    pub data: Vec<u8>,
    
    /// Actual data size (uncompressed size if compressed)
    #[prost(uint32, tag = "5")]
    pub size: u32,
    
    /// Compressed size (if different from size)
    #[prost(uint32, optional, tag = "6")]
    pub compressed_size: Option<u32>,
    
    /// Hash of this chunk (BLAKE3)
    #[prost(bytes, tag = "7")]
    pub hash: Vec<u8>,
    
    /// Is this the last chunk (EOF marker)
    #[prost(bool, tag = "8")]
    pub is_last: bool,
    
    /// Sequence number for ordering
    #[prost(uint64, optional, tag = "9")]
    pub sequence: Option<u64>,
}

/// Request to resume a transfer
#[derive(Clone, PartialEq, Message)]
pub struct ResumeRequest {
    /// Session ID to resume
    #[prost(string, tag = "1")]
    pub session_id: String,
    
    /// List of chunk IDs already received
    #[prost(uint64, repeated, tag = "2")]
    pub received_chunks: Vec<u64>,
    
    /// Bitmap of received chunks (for efficiency)
    #[prost(bytes, optional, tag = "3")]
    pub received_bitmap: Option<Vec<u8>>,
    
    /// Last chunk ID successfully received
    #[prost(uint64, optional, tag = "4")]
    pub last_chunk_id: Option<u64>,
}

/// Response to resume request
#[derive(Clone, PartialEq, Message)]
pub struct ResumeResponse {
    /// Session ID
    #[prost(string, tag = "1")]
    pub session_id: String,
    
    /// Whether resume is accepted
    #[prost(bool, tag = "2")]
    pub accepted: bool,
    
    /// List of chunk IDs that need to be resent
    #[prost(uint64, repeated, tag = "3")]
    pub missing_chunks: Vec<u64>,
    
    /// Total chunks remaining
    #[prost(uint64, tag = "4")]
    pub chunks_remaining: u64,
    
    /// Optional error message if not accepted
    #[prost(string, optional, tag = "5")]
    pub error: Option<String>,
}

/// Status update during transfer
#[derive(Clone, PartialEq, Message)]
pub struct StatusUpdate {
    /// Session ID
    #[prost(string, tag = "1")]
    pub session_id: String,
    
    /// Transfer state
    #[prost(enumeration = "TransferState", tag = "2")]
    pub state: i32,
    
    /// Chunks received/sent so far
    #[prost(uint64, tag = "3")]
    pub chunks_transferred: u64,
    
    /// Total chunks
    #[prost(uint64, tag = "4")]
    pub total_chunks: u64,
    
    /// Bytes transferred
    #[prost(uint64, tag = "5")]
    pub bytes_transferred: u64,
    
    /// Total bytes
    #[prost(uint64, tag = "6")]
    pub total_bytes: u64,
    
    /// Transfer rate (bytes/sec)
    #[prost(uint64, optional, tag = "7")]
    pub transfer_rate: Option<u64>,
    
    /// Estimated time remaining (seconds)
    #[prost(uint64, optional, tag = "8")]
    pub eta_seconds: Option<u64>,
    
    /// Optional status message
    #[prost(string, optional, tag = "9")]
    pub message: Option<String>,
}

/// Transfer completion message
#[derive(Clone, PartialEq, Message)]
pub struct TransferComplete {
    /// Session ID
    #[prost(string, tag = "1")]
    pub session_id: String,
    
    /// Whether transfer was successful
    #[prost(bool, tag = "2")]
    pub success: bool,
    
    /// Total chunks transferred
    #[prost(uint64, tag = "3")]
    pub chunks_transferred: u64,
    
    /// Total bytes transferred
    #[prost(uint64, tag = "4")]
    pub bytes_transferred: u64,
    
    /// Final file hash (BLAKE3)
    #[prost(bytes, tag = "5")]
    pub file_hash: Vec<u8>,
    
    /// Transfer duration in milliseconds
    #[prost(uint64, tag = "6")]
    pub duration_ms: u64,
    
    /// Average transfer rate (bytes/sec)
    #[prost(uint64, tag = "7")]
    pub avg_transfer_rate: u64,
    
    /// Optional error message if not successful
    #[prost(string, optional, tag = "8")]
    pub error: Option<String>,
}

/// Transfer state enumeration
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, prost::Enumeration)]
#[repr(i32)]
pub enum TransferState {
    /// Transfer is initializing
    Initializing = 0,
    /// Handshake in progress
    Handshaking = 1,
    /// Sending manifest
    SendingManifest = 2,
    /// Receiving manifest
    ReceivingManifest = 3,
    /// Actively transferring data
    Transferring = 4,
    /// Resuming a previous transfer
    Resuming = 5,
    /// Completing transfer (verification)
    Completing = 6,
    /// Transfer completed successfully
    Completed = 7,
    /// Transfer failed
    Failed = 8,
    /// Transfer was cancelled
    Cancelled = 9,
}

/// Helper functions for serialization/deserialization
impl SessionStart {
    /// Encode to bytes
    pub fn encode_to_vec(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.reserve(self.encoded_len());
        self.encode(&mut buf).expect("Failed to encode SessionStart");
        buf
    }
    
    /// Decode from bytes
    pub fn decode_from_bytes(bytes: &[u8]) -> Result<Self, prost::DecodeError> {
        Self::decode(bytes)
    }
}

impl Manifest {
    /// Encode to bytes
    pub fn encode_to_vec(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.reserve(self.encoded_len());
        self.encode(&mut buf).expect("Failed to encode Manifest");
        buf
    }
    
    /// Decode from bytes
    pub fn decode_from_bytes(bytes: &[u8]) -> Result<Self, prost::DecodeError> {
        Self::decode(bytes)
    }
}

impl ChunkPacket {
    /// Encode to bytes
    pub fn encode_to_vec(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.reserve(self.encoded_len());
        self.encode(&mut buf).expect("Failed to encode ChunkPacket");
        buf
    }
    
    /// Decode from bytes
    pub fn decode_from_bytes(bytes: &[u8]) -> Result<Self, prost::DecodeError> {
        Self::decode(bytes)
    }
}

impl ResumeRequest {
    /// Encode to bytes
    pub fn encode_to_vec(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.reserve(self.encoded_len());
        self.encode(&mut buf).expect("Failed to encode ResumeRequest");
        buf
    }
    
    /// Decode from bytes
    pub fn decode_from_bytes(bytes: &[u8]) -> Result<Self, prost::DecodeError> {
        Self::decode(bytes)
    }
}

impl ResumeResponse {
    /// Encode to bytes
    pub fn encode_to_vec(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.reserve(self.encoded_len());
        self.encode(&mut buf).expect("Failed to encode ResumeResponse");
        buf
    }
    
    /// Decode from bytes
    pub fn decode_from_bytes(bytes: &[u8]) -> Result<Self, prost::DecodeError> {
        Self::decode(bytes)
    }
}

impl StatusUpdate {
    /// Encode to bytes
    pub fn encode_to_vec(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.reserve(self.encoded_len());
        self.encode(&mut buf).expect("Failed to encode StatusUpdate");
        buf
    }
    
    /// Decode from bytes
    pub fn decode_from_bytes(bytes: &[u8]) -> Result<Self, prost::DecodeError> {
        Self::decode(bytes)
    }
}

impl TransferComplete {
    /// Encode to bytes
    pub fn encode_to_vec(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.reserve(self.encoded_len());
        self.encode(&mut buf).expect("Failed to encode TransferComplete");
        buf
    }
    
    /// Decode from bytes
    pub fn decode_from_bytes(bytes: &[u8]) -> Result<Self, prost::DecodeError> {
        Self::decode(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_start_encode_decode() {
        let msg = SessionStart {
            session_id: "test-session-123".to_string(),
            file_path: "/path/to/file.txt".to_string(),
            file_size: 1024,
            chunk_size: 256,
            total_chunks: 4,
            compression: "zstd".to_string(),
            metadata: Some(r#"{"key": "value"}"#.to_string()),
        };
        
        let encoded = msg.encode_to_vec();
        let decoded = SessionStart::decode_from_bytes(&encoded).unwrap();
        
        assert_eq!(msg, decoded);
    }

    #[test]
    fn test_manifest_encode_decode() {
        let msg = Manifest {
            session_id: "test-session".to_string(),
            file_name: "test.dat".to_string(),
            file_size: 2048,
            chunk_size: 512,
            total_chunks: 4,
            file_hash: vec![1, 2, 3, 4],
            chunk_hashes: vec![vec![5, 6], vec![7, 8]],
            compression: "lz4hc".to_string(),
            original_size: Some(2048),
        };
        
        let encoded = msg.encode_to_vec();
        let decoded = Manifest::decode_from_bytes(&encoded).unwrap();
        
        assert_eq!(msg, decoded);
    }

    #[test]
    fn test_chunk_packet_encode_decode() {
        let msg = ChunkPacket {
            session_id: "session-1".to_string(),
            chunk_id: 42,
            offset: 1024,
            data: vec![0xDE, 0xAD, 0xBE, 0xEF],
            size: 4,
            compressed_size: Some(3),
            hash: vec![0xFF; 32],
            is_last: false,
            sequence: Some(100),
        };
        
        let encoded = msg.encode_to_vec();
        let decoded = ChunkPacket::decode_from_bytes(&encoded).unwrap();
        
        assert_eq!(msg, decoded);
    }

    #[test]
    fn test_resume_request_encode_decode() {
        let msg = ResumeRequest {
            session_id: "resume-session".to_string(),
            received_chunks: vec![0, 1, 2, 5, 6],
            received_bitmap: Some(vec![0b11100111]),
            last_chunk_id: Some(6),
        };
        
        let encoded = msg.encode_to_vec();
        let decoded = ResumeRequest::decode_from_bytes(&encoded).unwrap();
        
        assert_eq!(msg, decoded);
    }

    #[test]
    fn test_status_update_encode_decode() {
        let msg = StatusUpdate {
            session_id: "status-session".to_string(),
            state: TransferState::Transferring as i32,
            chunks_transferred: 50,
            total_chunks: 100,
            bytes_transferred: 51200,
            total_bytes: 102400,
            transfer_rate: Some(10240),
            eta_seconds: Some(5),
            message: Some("50% complete".to_string()),
        };
        
        let encoded = msg.encode_to_vec();
        let decoded = StatusUpdate::decode_from_bytes(&encoded).unwrap();
        
        assert_eq!(msg, decoded);
    }

    #[test]
    fn test_transfer_complete_encode_decode() {
        let msg = TransferComplete {
            session_id: "complete-session".to_string(),
            success: true,
            chunks_transferred: 100,
            bytes_transferred: 102400,
            file_hash: vec![0xAB; 32],
            duration_ms: 5000,
            avg_transfer_rate: 20480,
            error: None,
        };
        
        let encoded = msg.encode_to_vec();
        let decoded = TransferComplete::decode_from_bytes(&encoded).unwrap();
        
        assert_eq!(msg, decoded);
    }

    #[test]
    fn test_transfer_state_enum() {
        use std::convert::TryFrom;
        
        assert_eq!(TransferState::try_from(0), Ok(TransferState::Initializing));
        assert_eq!(TransferState::try_from(4), Ok(TransferState::Transferring));
        assert_eq!(TransferState::try_from(7), Ok(TransferState::Completed));
        assert!(TransferState::try_from(99).is_err());
    }
}
