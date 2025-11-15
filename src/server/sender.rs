// Server-side file sending logic

use super::connection::ServerConnection;

/// Handles sending data to connected clients
pub struct DataSender {
    total_bytes_sent: usize,
}

impl DataSender {
    /// Create a new data sender
    pub fn new() -> Self {
        Self {
            total_bytes_sent: 0,
        }
    }

    /// Send data to a client on a specific stream
    /// 
    /// # Arguments
    /// * `connection` - The server connection to send data on
    /// * `stream_id` - The stream ID to send data on
    /// * `data` - The data to send
    /// * `fin` - Whether this is the final data on the stream
    /// 
    /// # Returns
    /// The number of bytes successfully sent
    pub fn send_data(
        &mut self,
        connection: &mut ServerConnection,
        stream_id: u64,
        data: &[u8],
        fin: bool,
    ) -> Result<usize, Box<dyn std::error::Error>> {
        match connection.stream_send(stream_id, data, fin) {
            Ok(written) => {
                self.total_bytes_sent += written;
                println!(
                    "DataSender: sent {} bytes on stream {} (total: {})",
                    written, stream_id, self.total_bytes_sent
                );
                Ok(written)
            }
            Err(e) => {
                eprintln!("DataSender: error sending on stream {}: {:?}", stream_id, e);
                Err(Box::new(e))
            }
        }
    }

    /// Send data in chunks to a client
    /// 
    /// # Arguments
    /// * `connection` - The server connection to send data on
    /// * `stream_id` - The stream ID to send data on
    /// * `data` - The complete data to send
    /// * `chunk_size` - Size of each chunk
    /// * `fin` - Whether to finish the stream after sending all data
    /// 
    /// # Returns
    /// The total number of bytes successfully sent
    pub fn send_chunked(
        &mut self,
        connection: &mut ServerConnection,
        stream_id: u64,
        data: &[u8],
        chunk_size: usize,
        fin: bool,
    ) -> Result<usize, Box<dyn std::error::Error>> {
        let mut total_sent = 0;
        let chunks = data.chunks(chunk_size);
        let total_chunks = chunks.len();

        for (i, chunk) in data.chunks(chunk_size).enumerate() {
            let is_last = i == total_chunks - 1;
            let should_fin = fin && is_last;

            match self.send_data(connection, stream_id, chunk, should_fin) {
                Ok(sent) => {
                    total_sent += sent;
                    println!(
                        "DataSender: chunk {}/{} sent ({} bytes)",
                        i + 1,
                        total_chunks,
                        sent
                    );
                }
                Err(e) => {
                    eprintln!("DataSender: error sending chunk {}: {:?}", i, e);
                    return Err(e);
                }
            }
        }

        Ok(total_sent)
    }

    /// Send data to multiple streams (round-robin distribution)
    /// 
    /// # Arguments
    /// * `connection` - The server connection to send data on
    /// * `stream_ids` - Vector of stream IDs to distribute data across
    /// * `data` - The complete data to send
    /// * `chunk_size` - Size of each chunk
    /// 
    /// # Returns
    /// The total number of bytes successfully sent across all streams
    pub fn send_distributed(
        &mut self,
        connection: &mut ServerConnection,
        stream_ids: &[u64],
        data: &[u8],
        chunk_size: usize,
    ) -> Result<usize, Box<dyn std::error::Error>> {
        if stream_ids.is_empty() {
            return Err("No stream IDs provided".into());
        }

        let mut total_sent = 0;
        let chunks: Vec<&[u8]> = data.chunks(chunk_size).collect();
        let total_chunks = chunks.len();

        for (i, chunk) in chunks.iter().enumerate() {
            let stream_idx = i % stream_ids.len();
            let stream_id = stream_ids[stream_idx];
            let is_last = i == total_chunks - 1;

            match self.send_data(connection, stream_id, chunk, is_last) {
                Ok(sent) => {
                    total_sent += sent;
                    println!(
                        "DataSender: distributed chunk {}/{} to stream {} ({} bytes)",
                        i + 1,
                        total_chunks,
                        stream_id,
                        sent
                    );
                }
                Err(e) => {
                    eprintln!(
                        "DataSender: error distributing chunk {} to stream {}: {:?}",
                        i, stream_id, e
                    );
                    return Err(e);
                }
            }
        }

        Ok(total_sent)
    }

    /// Get the total number of bytes sent
    pub fn total_bytes_sent(&self) -> usize {
        self.total_bytes_sent
    }

    /// Reset the byte counter
    pub fn reset_counter(&mut self) {
        self.total_bytes_sent = 0;
    }
}

impl Default for DataSender {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_data_sender_creation() {
        let sender = DataSender::new();
        assert_eq!(sender.total_bytes_sent(), 0);
    }

    #[test]
    fn test_counter_reset() {
        let mut sender = DataSender::new();
        sender.total_bytes_sent = 100;
        sender.reset_counter();
        assert_eq!(sender.total_bytes_sent(), 0);
    }
}
