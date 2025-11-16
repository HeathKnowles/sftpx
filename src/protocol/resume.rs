// Resume request and response protocol handlers

use crate::protocol::messages::{ResumeRequest, ResumeResponse};
use crate::common::error::{Error, Result};

/// Handles sending ResumeRequest messages
pub struct ResumeRequestSender;

impl ResumeRequestSender {
    pub fn new() -> Self {
        Self
    }
    
    /// Send a resume request with bitmap
    pub fn send_request<F>(
        &self,
        session_id: String,
        received_chunks: Vec<u64>,
        received_bitmap: Option<Vec<u8>>,
        last_chunk_id: Option<u64>,
        mut write_fn: F,
    ) -> Result<usize>
    where
        F: FnMut(&[u8], bool) -> std::result::Result<usize, quiche::Error>,
    {
        let request = ResumeRequest {
            session_id,
            received_chunks,
            received_bitmap,
            last_chunk_id,
        };
        
        let data = request.encode_to_vec();
        
        // Send with length prefix
        let len = data.len() as u32;
        let mut buffer = Vec::with_capacity(4 + data.len());
        buffer.extend_from_slice(&len.to_be_bytes());
        buffer.extend_from_slice(&data);
        
        // Send all data with fin=true
        write_fn(&buffer, true)
            .map_err(|e| Error::Protocol(format!("Failed to send resume request: {:?}", e)))
    }
}

/// Handles receiving ResumeRequest messages
pub struct ResumeRequestReceiver {
    buffer: Vec<u8>,
    expected_length: Option<usize>,
}

impl ResumeRequestReceiver {
    pub fn new() -> Self {
        Self {
            buffer: Vec::new(),
            expected_length: None,
        }
    }
    
    /// Receive chunk of resume request data
    pub fn receive_chunk(&mut self, data: &[u8], fin: bool) -> Result<Option<ResumeRequest>> {
        self.buffer.extend_from_slice(data);
        
        // Parse length prefix if we haven't yet
        if self.expected_length.is_none() && self.buffer.len() >= 4 {
            let len_bytes: [u8; 4] = self.buffer[0..4].try_into().unwrap();
            self.expected_length = Some(u32::from_be_bytes(len_bytes) as usize);
        }
        
        // Check if we have complete message
        if let Some(expected) = self.expected_length {
            if self.buffer.len() >= expected + 4 {
                let message_data = &self.buffer[4..4 + expected];
                let request = ResumeRequest::decode_from_bytes(message_data)
                    .map_err(|e| Error::Protocol(format!("Failed to decode resume request: {:?}", e)))?;
                return Ok(Some(request));
            }
        }
        
        // Need more data
        if fin && self.expected_length.is_some() {
            return Err(Error::Protocol("Incomplete resume request".to_string()));
        }
        
        Ok(None)
    }
}

/// Handles sending ResumeResponse messages
pub struct ResumeResponseSender;

impl ResumeResponseSender {
    pub fn new() -> Self {
        Self
    }
    
    /// Send a resume response with missing chunks list
    pub fn send_response<F>(
        &self,
        session_id: String,
        accepted: bool,
        missing_chunks: Vec<u64>,
        chunks_remaining: u64,
        error: Option<String>,
        mut write_fn: F,
    ) -> Result<usize>
    where
        F: FnMut(&[u8], bool) -> std::result::Result<usize, quiche::Error>,
    {
        let response = ResumeResponse {
            session_id,
            accepted,
            missing_chunks,
            chunks_remaining,
            error,
        };
        
        let data = response.encode_to_vec();
        
        // Send with length prefix
        let len = data.len() as u32;
        let mut buffer = Vec::with_capacity(4 + data.len());
        buffer.extend_from_slice(&len.to_be_bytes());
        buffer.extend_from_slice(&data);
        
        // Send all data with fin=true
        write_fn(&buffer, true)
            .map_err(|e| Error::Protocol(format!("Failed to send resume response: {:?}", e)))
    }
}

/// Handles receiving ResumeResponse messages
pub struct ResumeResponseReceiver {
    buffer: Vec<u8>,
    expected_length: Option<usize>,
}

impl ResumeResponseReceiver {
    pub fn new() -> Self {
        Self {
            buffer: Vec::new(),
            expected_length: None,
        }
    }
    
    /// Receive chunk of resume response data
    pub fn receive_chunk(&mut self, data: &[u8], fin: bool) -> Result<Option<ResumeResponse>> {
        self.buffer.extend_from_slice(data);
        
        // Parse length prefix if we haven't yet
        if self.expected_length.is_none() && self.buffer.len() >= 4 {
            let len_bytes: [u8; 4] = self.buffer[0..4].try_into().unwrap();
            self.expected_length = Some(u32::from_be_bytes(len_bytes) as usize);
        }
        
        // Check if we have complete message
        if let Some(expected) = self.expected_length {
            if self.buffer.len() >= expected + 4 {
                let message_data = &self.buffer[4..4 + expected];
                let response = ResumeResponse::decode_from_bytes(message_data)
                    .map_err(|e| Error::Protocol(format!("Failed to decode resume response: {:?}", e)))?;
                return Ok(Some(response));
            }
        }
        
        // Need more data
        if fin && self.expected_length.is_some() {
            return Err(Error::Protocol("Incomplete resume response".to_string()));
        }
        
        Ok(None)
    }
}
