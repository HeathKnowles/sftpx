// Control stream implementation

use crate::common::error::{Error, Result};
use crate::protocol::control::{ControlMessage, ControlMessageType};
use std::collections::VecDeque;

/// Maximum control message size (1 MB)
const MAX_CONTROL_MESSAGE_SIZE: usize = 1024 * 1024;

/// Control stream handler for processing control messages
pub struct ControlStreamHandler {
    /// Buffer for incoming messages
    recv_buffer: Vec<u8>,
    /// Queue of received control messages
    message_queue: VecDeque<ControlMessage>,
    /// Total bytes received
    bytes_received: usize,
}

impl ControlStreamHandler {
    /// Create a new control stream handler
    pub fn new() -> Self {
        Self {
            recv_buffer: Vec::new(),
            message_queue: VecDeque::new(),
            bytes_received: 0,
        }
    }

    /// Process incoming data from the control stream
    /// 
    /// # Arguments
    /// * `data` - Raw bytes received from the stream
    /// 
    /// # Returns
    /// Number of messages parsed
    pub fn process_data(&mut self, data: &[u8]) -> Result<usize> {
        if data.is_empty() {
            return Ok(0);
        }

        // Append to buffer
        self.recv_buffer.extend_from_slice(data);
        self.bytes_received += data.len();

        // Check size limit
        if self.recv_buffer.len() > MAX_CONTROL_MESSAGE_SIZE {
            return Err(Error::Protocol(format!(
                "Control message too large: {} bytes",
                self.recv_buffer.len()
            )));
        }

        // Try to decode message
        let mut parsed_count = 0;
        match ControlMessage::decode_from_bytes(&self.recv_buffer) {
            Ok(msg) => {
                self.message_queue.push_back(msg);
                self.recv_buffer.clear();
                parsed_count = 1;
            }
            Err(_) => {
                // Incomplete message, keep buffering
            }
        }

        Ok(parsed_count)
    }

    /// Get the next control message from the queue
    pub fn next_message(&mut self) -> Option<ControlMessage> {
        self.message_queue.pop_front()
    }

    /// Check if there are pending messages
    pub fn has_messages(&self) -> bool {
        !self.message_queue.is_empty()
    }

    /// Get number of pending messages
    pub fn message_count(&self) -> usize {
        self.message_queue.len()
    }

    /// Clear all pending messages and buffers
    pub fn clear(&mut self) {
        self.recv_buffer.clear();
        self.message_queue.clear();
    }

    /// Get total bytes received
    pub fn bytes_received(&self) -> usize {
        self.bytes_received
    }
}

impl Default for ControlStreamHandler {
    fn default() -> Self {
        Self::new()
    }
}

/// Control message sender for sending control messages over QUIC
pub struct ControlMessageSender;

impl ControlMessageSender {
    /// Create a new control message sender
    pub fn new() -> Self {
        Self
    }

    /// Send a control message over a stream
    /// 
    /// # Arguments
    /// * `message` - The control message to send
    /// * `send_fn` - Function to send data (data, fin) -> Result<bytes_written>
    /// 
    /// # Returns
    /// Number of bytes written
    pub fn send_message<F>(
        &self,
        message: &ControlMessage,
        mut send_fn: F,
    ) -> Result<usize>
    where
        F: FnMut(&[u8], bool) -> Result<usize>,
    {
        let encoded = message.encode_to_vec();

        if encoded.len() > MAX_CONTROL_MESSAGE_SIZE {
            return Err(Error::Protocol(format!(
                "Control message too large: {} bytes",
                encoded.len()
            )));
        }

        // Send with FIN=false to keep stream open for more messages
        let bytes_written = send_fn(&encoded, false)?;

        if bytes_written != encoded.len() {
            return Err(Error::Io(std::io::Error::new(
                std::io::ErrorKind::WriteZero,
                format!("Partial write: {}/{} bytes", bytes_written, encoded.len()),
            )));
        }

        Ok(bytes_written)
    }

    /// Send multiple control messages in a batch
    pub fn send_batch<F>(
        &self,
        messages: &[ControlMessage],
        mut send_fn: F,
    ) -> Result<usize>
    where
        F: FnMut(&[u8], bool) -> Result<usize>,
    {
        let mut total_written = 0;

        for message in messages {
            let encoded = message.encode_to_vec();
            let bytes_written = send_fn(&encoded, false)?;
            total_written += bytes_written;

            if bytes_written != encoded.len() {
                return Err(Error::Io(std::io::Error::new(
                    std::io::ErrorKind::WriteZero,
                    "Partial write in batch",
                )));
            }
        }

        Ok(total_written)
    }
}

impl Default for ControlMessageSender {
    fn default() -> Self {
        Self::new()
    }
}

/// Handler for processing specific control message types
pub trait ControlMessageHandler {
    /// Handle an ACK message
    fn handle_ack(&mut self, message: &ControlMessage) -> Result<()>;

    /// Handle a NACK message
    fn handle_nack(&mut self, message: &ControlMessage) -> Result<()>;

    /// Handle a retransmit request
    fn handle_retransmit_request(&mut self, message: &ControlMessage) -> Result<()>;

    /// Handle a cancel retransmit message
    fn handle_cancel_retransmit(&mut self, message: &ControlMessage) -> Result<()>;

    /// Handle a pause message
    fn handle_pause(&mut self, message: &ControlMessage) -> Result<()>;

    /// Handle a resume message
    fn handle_resume(&mut self, message: &ControlMessage) -> Result<()>;
}

/// Dispatcher for routing control messages to appropriate handlers
pub struct ControlMessageDispatcher<H: ControlMessageHandler> {
    handler: H,
    messages_processed: u64,
}

impl<H: ControlMessageHandler> ControlMessageDispatcher<H> {
    /// Create a new dispatcher with a message handler
    pub fn new(handler: H) -> Self {
        Self {
            handler,
            messages_processed: 0,
        }
    }

    /// Dispatch a control message to the appropriate handler
    pub fn dispatch(&mut self, message: &ControlMessage) -> Result<()> {
        let msg_type = message.get_type();

        let result = match msg_type {
            ControlMessageType::Ack => self.handler.handle_ack(message),
            ControlMessageType::Nack => self.handler.handle_nack(message),
            ControlMessageType::RetransmitRequest => {
                self.handler.handle_retransmit_request(message)
            }
            ControlMessageType::CancelRetransmit => {
                self.handler.handle_cancel_retransmit(message)
            }
            ControlMessageType::Pause => self.handler.handle_pause(message),
            ControlMessageType::Resume => self.handler.handle_resume(message),
        };

        if result.is_ok() {
            self.messages_processed += 1;
        }

        result
    }

    /// Get number of messages processed
    pub fn messages_processed(&self) -> u64 {
        self.messages_processed
    }

    /// Get mutable reference to the handler
    pub fn handler_mut(&mut self) -> &mut H {
        &mut self.handler
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_control_stream_handler() {
        let mut handler = ControlStreamHandler::new();

        // Create a test message
        let msg = ControlMessage::ack("session-1".to_string(), vec![1, 2, 3]);
        let encoded = msg.encode_to_vec();

        // Process the data
        let count = handler.process_data(&encoded).unwrap();
        assert_eq!(count, 1);
        assert!(handler.has_messages());

        // Get the message
        let received = handler.next_message().unwrap();
        assert_eq!(received.session_id, "session-1");
        assert_eq!(received.chunk_ids, vec![1, 2, 3]);
    }

    #[test]
    fn test_control_message_sender() {
        let sender = ControlMessageSender::new();
        let msg = ControlMessage::nack(
            "session-2".to_string(),
            vec![5],
            Some("Test error".to_string()),
        );

        let mut sent_data = Vec::new();
        let result = sender.send_message(&msg, |data, _fin| {
            sent_data.extend_from_slice(data);
            Ok(data.len())
        });

        assert!(result.is_ok());
        assert!(!sent_data.is_empty());

        // Verify we can decode what was sent
        let decoded = ControlMessage::decode_from_bytes(&sent_data).unwrap();
        assert_eq!(decoded.session_id, "session-2");
        assert_eq!(decoded.chunk_ids, vec![5]);
    }

    #[test]
    fn test_partial_message_buffering() {
        let mut handler = ControlStreamHandler::new();
        let msg = ControlMessage::retransmit_request("session-3".to_string(), vec![10, 11]);
        let encoded = msg.encode_to_vec();

        // Send in two parts
        let mid = encoded.len() / 2;
        let count1 = handler.process_data(&encoded[..mid]).unwrap();
        assert_eq!(count1, 0); // Incomplete

        let count2 = handler.process_data(&encoded[mid..]).unwrap();
        assert_eq!(count2, 1); // Complete

        let received = handler.next_message().unwrap();
        assert_eq!(received.chunk_ids, vec![10, 11]);
    }
}
