// Hash checking protocol for deduplication

use crate::common::error::{Error, Result};
use crate::protocol::messages::{HashCheckRequest, HashCheckResponse};
use prost::Message;
use std::collections::HashSet;

/// Sender for hash check requests
pub struct HashCheckRequestSender;

impl HashCheckRequestSender {
    pub fn new() -> Self {
        Self
    }
    
    /// Send a hash check request
    /// 
    /// # Arguments
    /// * `session_id` - Session identifier
    /// * `chunk_hashes` - List of chunk hashes to check
    /// * `send_fn` - Callback to send data over the network
    pub fn send_request<F>(
        &self,
        session_id: String,
        chunk_hashes: Vec<Vec<u8>>,
        mut send_fn: F,
    ) -> Result<usize>
    where
        F: FnMut(&[u8], bool) -> Result<usize>,
    {
        let request = HashCheckRequest {
            session_id,
            chunk_hashes,
        };
        
        let mut buf = Vec::new();
        request.encode(&mut buf)
            .map_err(|e| Error::Protocol(format!("Failed to encode hash check request: {}", e)))?;
        
        let len_bytes = (buf.len() as u32).to_be_bytes();
        send_fn(&len_bytes, false)?;
        send_fn(&buf, true)?;
        
        Ok(buf.len())
    }
}

/// Receiver for hash check requests
pub struct HashCheckRequestReceiver {
    buffer: Vec<u8>,
    expected_length: Option<usize>,
}

impl HashCheckRequestReceiver {
    pub fn new() -> Self {
        Self {
            buffer: Vec::new(),
            expected_length: None,
        }
    }
    
    /// Receive and parse a hash check request
    /// 
    /// # Arguments
    /// * `data` - Incoming data chunk
    /// * `fin` - Whether this is the final chunk
    /// 
    /// # Returns
    /// * `Ok(Some(request))` - Successfully received complete request
    /// * `Ok(None)` - Need more data
    /// * `Err(error)` - Parse error
    pub fn receive_chunk(&mut self, data: &[u8], fin: bool) -> Result<Option<HashCheckRequest>> {
        self.buffer.extend_from_slice(data);
        
        // Try to read length prefix if we don't have it yet
        if self.expected_length.is_none() && self.buffer.len() >= 4 {
            let len_bytes: [u8; 4] = self.buffer[0..4].try_into().unwrap();
            let length = u32::from_be_bytes(len_bytes) as usize;
            self.expected_length = Some(length);
            self.buffer.drain(0..4);
        }
        
        // Check if we have the complete message
        if let Some(expected) = self.expected_length {
            if self.buffer.len() >= expected {
                let request = HashCheckRequest::decode(&self.buffer[..expected])
                    .map_err(|e| Error::Protocol(format!("Failed to decode hash check request: {}", e)))?;
                
                self.buffer.clear();
                self.expected_length = None;
                
                return Ok(Some(request));
            }
        }
        
        if fin && self.expected_length.is_none() {
            return Err(Error::Protocol("Hash check request incomplete".to_string()));
        }
        
        Ok(None)
    }
}

/// Sender for hash check responses
pub struct HashCheckResponseSender;

impl HashCheckResponseSender {
    pub fn new() -> Self {
        Self
    }
    
    /// Send a hash check response
    /// 
    /// # Arguments
    /// * `session_id` - Session identifier
    /// * `existing_hashes` - List of hashes that already exist
    /// * `send_fn` - Callback to send data over the network
    pub fn send_response<F>(
        &self,
        session_id: String,
        existing_hashes: Vec<Vec<u8>>,
        mut send_fn: F,
    ) -> Result<usize>
    where
        F: FnMut(&[u8], bool) -> Result<usize>,
    {
        let response = HashCheckResponse {
            session_id,
            existing_hashes,
            existing_bitmap: None, // TODO: Implement bitmap for efficiency
        };
        
        let mut buf = Vec::new();
        response.encode(&mut buf)
            .map_err(|e| Error::Protocol(format!("Failed to encode hash check response: {}", e)))?;
        
        let len_bytes = (buf.len() as u32).to_be_bytes();
        send_fn(&len_bytes, false)?;
        send_fn(&buf, true)?;
        
        Ok(buf.len())
    }
}

/// Receiver for hash check responses
pub struct HashCheckResponseReceiver {
    buffer: Vec<u8>,
    expected_length: Option<usize>,
}

impl HashCheckResponseReceiver {
    pub fn new() -> Self {
        Self {
            buffer: Vec::new(),
            expected_length: None,
        }
    }
    
    /// Receive and parse a hash check response
    /// 
    /// # Arguments
    /// * `data` - Incoming data chunk
    /// * `fin` - Whether this is the final chunk
    /// 
    /// # Returns
    /// * `Ok(Some(response))` - Successfully received complete response
    /// * `Ok(None)` - Need more data
    /// * `Err(error)` - Parse error
    pub fn receive_chunk(&mut self, data: &[u8], fin: bool) -> Result<Option<HashCheckResponse>> {
        self.buffer.extend_from_slice(data);
        
        // Try to read length prefix if we don't have it yet
        if self.expected_length.is_none() && self.buffer.len() >= 4 {
            let len_bytes: [u8; 4] = self.buffer[0..4].try_into().unwrap();
            let length = u32::from_be_bytes(len_bytes) as usize;
            self.expected_length = Some(length);
            self.buffer.drain(0..4);
        }
        
        // Check if we have the complete message
        if let Some(expected) = self.expected_length {
            if self.buffer.len() >= expected {
                let response = HashCheckResponse::decode(&self.buffer[..expected])
                    .map_err(|e| Error::Protocol(format!("Failed to decode hash check response: {}", e)))?;
                
                self.buffer.clear();
                self.expected_length = None;
                
                return Ok(Some(response));
            }
        }
        
        if fin && self.expected_length.is_none() {
            return Err(Error::Protocol("Hash check response incomplete".to_string()));
        }
        
        Ok(None)
    }
    
    /// Convert response to a HashSet for efficient lookups
    pub fn to_hash_set(response: &HashCheckResponse) -> HashSet<Vec<u8>> {
        response.existing_hashes.iter().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_check_request_roundtrip() {
        let sender = HashCheckRequestSender::new();
        let mut receiver = HashCheckRequestReceiver::new();
        
        let session_id = "test_session".to_string();
        let chunk_hashes = vec![
            vec![1, 2, 3, 4],
            vec![5, 6, 7, 8],
        ];
        
        let mut sent_data = Vec::new();
        sender.send_request(session_id.clone(), chunk_hashes.clone(), |data, _fin| {
            sent_data.extend_from_slice(data);
            Ok(data.len())
        }).unwrap();
        
        let request = receiver.receive_chunk(&sent_data, true).unwrap().unwrap();
        assert_eq!(request.session_id, session_id);
        assert_eq!(request.chunk_hashes, chunk_hashes);
    }

    #[test]
    fn test_hash_check_response_roundtrip() {
        let sender = HashCheckResponseSender::new();
        let mut receiver = HashCheckResponseReceiver::new();
        
        let session_id = "test_session".to_string();
        let existing_hashes = vec![
            vec![1, 2, 3, 4],
        ];
        
        let mut sent_data = Vec::new();
        sender.send_response(session_id.clone(), existing_hashes.clone(), |data, _fin| {
            sent_data.extend_from_slice(data);
            Ok(data.len())
        }).unwrap();
        
        let response = receiver.receive_chunk(&sent_data, true).unwrap().unwrap();
        assert_eq!(response.session_id, session_id);
        assert_eq!(response.existing_hashes, existing_hashes);
    }
}
