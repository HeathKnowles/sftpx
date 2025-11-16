// Control stream messages (ACK, NACK, retransmit requests)

use crate::common::error::{Error, Result};
use prost::Message;

/// Control message types for flow control and retransmission
#[derive(Clone, PartialEq, Message)]
pub struct ControlMessage {
    /// Session ID
    #[prost(string, tag = "1")]
    pub session_id: String,
    
    /// Message type
    #[prost(enumeration = "ControlMessageType", tag = "2")]
    pub message_type: i32,
    
    /// Chunk IDs for ACK/NACK
    #[prost(uint64, repeated, tag = "3")]
    pub chunk_ids: Vec<u64>,
    
    /// Optional reason/error message
    #[prost(string, optional, tag = "4")]
    pub reason: Option<String>,
    
    /// Timestamp (milliseconds since epoch)
    #[prost(uint64, tag = "5")]
    pub timestamp: u64,
}

/// Control message types
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, prost::Enumeration)]
#[repr(i32)]
pub enum ControlMessageType {
    /// Acknowledge successful receipt
    Ack = 0,
    /// Negative acknowledgment - request retransmission
    Nack = 1,
    /// Request specific chunks
    RetransmitRequest = 2,
    /// Cancel pending retransmissions
    CancelRetransmit = 3,
    /// Flow control - pause sending
    Pause = 4,
    /// Flow control - resume sending
    Resume = 5,
}

impl ControlMessage {
    /// Create a new ACK message
    pub fn ack(session_id: String, chunk_ids: Vec<u64>) -> Self {
        Self {
            session_id,
            message_type: ControlMessageType::Ack as i32,
            chunk_ids,
            reason: None,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
        }
    }
    
    /// Create a new NACK message for corrupted chunks
    pub fn nack(session_id: String, chunk_ids: Vec<u64>, reason: Option<String>) -> Self {
        Self {
            session_id,
            message_type: ControlMessageType::Nack as i32,
            chunk_ids,
            reason,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
        }
    }
    
    /// Create a retransmit request for missing chunks
    pub fn retransmit_request(session_id: String, chunk_ids: Vec<u64>) -> Self {
        Self {
            session_id,
            message_type: ControlMessageType::RetransmitRequest as i32,
            chunk_ids,
            reason: None,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
        }
    }
    
    /// Create a pause message
    pub fn pause(session_id: String) -> Self {
        Self {
            session_id,
            message_type: ControlMessageType::Pause as i32,
            chunk_ids: Vec::new(),
            reason: None,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
        }
    }
    
    /// Create a resume message
    pub fn resume(session_id: String) -> Self {
        Self {
            session_id,
            message_type: ControlMessageType::Resume as i32,
            chunk_ids: Vec::new(),
            reason: None,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
        }
    }
    
    /// Encode to bytes
    pub fn encode_to_vec(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        self.encode(&mut buf).expect("Failed to encode ControlMessage");
        buf
    }
    
    /// Decode from bytes
    pub fn decode_from_bytes(data: &[u8]) -> Result<Self> {
        Self::decode(data).map_err(|e| Error::DeserializationError(format!("{:?}", e)))
    }
    
    /// Get message type as enum
    pub fn get_type(&self) -> ControlMessageType {
        self.message_type.try_into().unwrap_or(ControlMessageType::Ack)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ack_message() {
        let msg = ControlMessage::ack("session-123".to_string(), vec![1, 2, 3]);
        
        assert_eq!(msg.session_id, "session-123");
        assert_eq!(msg.get_type(), ControlMessageType::Ack);
        assert_eq!(msg.chunk_ids, vec![1, 2, 3]);
        assert!(msg.reason.is_none());
    }

    #[test]
    fn test_nack_message() {
        let msg = ControlMessage::nack(
            "session-456".to_string(),
            vec![5],
            Some("Checksum mismatch".to_string())
        );
        
        assert_eq!(msg.session_id, "session-456");
        assert_eq!(msg.get_type(), ControlMessageType::Nack);
        assert_eq!(msg.chunk_ids, vec![5]);
        assert_eq!(msg.reason, Some("Checksum mismatch".to_string()));
    }

    #[test]
    fn test_retransmit_request() {
        let msg = ControlMessage::retransmit_request(
            "session-789".to_string(),
            vec![10, 11, 12]
        );
        
        assert_eq!(msg.get_type(), ControlMessageType::RetransmitRequest);
        assert_eq!(msg.chunk_ids.len(), 3);
    }

    #[test]
    fn test_encode_decode() {
        let original = ControlMessage::nack(
            "test-session".to_string(),
            vec![1, 2, 3],
            Some("Test reason".to_string())
        );
        
        let encoded = original.encode_to_vec();
        let decoded = ControlMessage::decode_from_bytes(&encoded).unwrap();
        
        assert_eq!(decoded.session_id, original.session_id);
        assert_eq!(decoded.message_type, original.message_type);
        assert_eq!(decoded.chunk_ids, original.chunk_ids);
        assert_eq!(decoded.reason, original.reason);
    }

    #[test]
    fn test_pause_resume() {
        let pause = ControlMessage::pause("session".to_string());
        assert_eq!(pause.get_type(), ControlMessageType::Pause);
        assert!(pause.chunk_ids.is_empty());
        
        let resume = ControlMessage::resume("session".to_string());
        assert_eq!(resume.get_type(), ControlMessageType::Resume);
    }
}

