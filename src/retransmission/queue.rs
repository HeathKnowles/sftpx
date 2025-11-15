// Retransmission queue management

use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// Entry in the retransmission queue
#[derive(Debug, Clone)]
pub struct RetransmitEntry {
    /// Chunk ID to retransmit
    pub chunk_id: u64,
    
    /// Timestamp when request was made
    pub requested_at: Instant,
    
    /// Number of times this chunk has been requested
    pub retry_count: u32,
    
    /// Session ID
    pub session_id: String,
}

/// Manages retransmission queue and rate limiting
#[derive(Debug)]
pub struct RetransmissionQueue {
    /// Queue of pending retransmit requests
    queue: VecDeque<RetransmitEntry>,
    
    /// Maximum queue size
    max_queue_size: usize,
    
    /// Minimum time between retransmit requests for same chunk
    min_retry_interval: Duration,
    
    /// Maximum number of concurrent in-flight retransmit requests
    max_in_flight: usize,
    
    /// Currently in-flight requests
    in_flight: Vec<RetransmitEntry>,
}

impl RetransmissionQueue {
    /// Create a new retransmission queue with default settings
    pub fn new() -> Self {
        Self {
            queue: VecDeque::new(),
            max_queue_size: 1000,
            min_retry_interval: Duration::from_secs(1),
            max_in_flight: 50,
            in_flight: Vec::new(),
        }
    }
    
    /// Create with custom configuration
    pub fn with_config(
        max_queue_size: usize,
        min_retry_interval: Duration,
        max_in_flight: usize,
    ) -> Self {
        Self {
            queue: VecDeque::new(),
            max_queue_size,
            min_retry_interval,
            max_in_flight,
            in_flight: Vec::new(),
        }
    }
    
    /// Add a chunk to the retransmission queue
    pub fn enqueue(&mut self, chunk_id: u64, session_id: String) -> bool {
        // Check if already in queue or in-flight
        if self.queue.iter().any(|e| e.chunk_id == chunk_id) {
            return false;
        }
        
        if self.in_flight.iter().any(|e| e.chunk_id == chunk_id) {
            return false;
        }
        
        // Check queue capacity
        if self.queue.len() >= self.max_queue_size {
            return false;
        }
        
        let entry = RetransmitEntry {
            chunk_id,
            requested_at: Instant::now(),
            retry_count: 0,
            session_id,
        };
        
        self.queue.push_back(entry);
        true
    }
    
    /// Add multiple chunks to the queue
    pub fn enqueue_batch(&mut self, chunk_ids: Vec<u64>, session_id: String) -> usize {
        let mut count = 0;
        for chunk_id in chunk_ids {
            if self.enqueue(chunk_id, session_id.clone()) {
                count += 1;
            }
        }
        count
    }
    
    /// Get next batch of chunks to request (respecting rate limits)
    pub fn dequeue_batch(&mut self, batch_size: usize) -> Vec<RetransmitEntry> {
        let mut batch = Vec::new();
        let now = Instant::now();
        
        // Clean up completed in-flight requests (shouldn't happen normally)
        self.in_flight.retain(|e| {
            now.duration_since(e.requested_at) < Duration::from_secs(30)
        });
        
        // Check how many more we can send
        let available_slots = self.max_in_flight.saturating_sub(self.in_flight.len());
        let batch_limit = batch_size.min(available_slots);
        
        // Take from queue
        while batch.len() < batch_limit {
            if let Some(mut entry) = self.queue.pop_front() {
                // Check if we need to wait before retrying
                if entry.retry_count > 0 {
                    let elapsed = now.duration_since(entry.requested_at);
                    if elapsed < self.min_retry_interval {
                        // Put it back and try next
                        self.queue.push_back(entry);
                        continue;
                    }
                }
                
                entry.requested_at = now;
                entry.retry_count += 1;
                
                self.in_flight.push(entry.clone());
                batch.push(entry);
            } else {
                break;
            }
        }
        
        batch
    }
    
    /// Mark a chunk as successfully received (remove from in-flight)
    pub fn mark_received(&mut self, chunk_id: u64) {
        self.in_flight.retain(|e| e.chunk_id != chunk_id);
        self.queue.retain(|e| e.chunk_id != chunk_id);
    }
    
    /// Mark a chunk as failed again (move from in-flight back to queue)
    pub fn mark_failed(&mut self, chunk_id: u64) {
        if let Some(pos) = self.in_flight.iter().position(|e| e.chunk_id == chunk_id) {
            let entry = self.in_flight.remove(pos);
            // Re-queue at front for priority retry
            self.queue.push_front(entry);
        }
    }
    
    /// Check for timed-out in-flight requests and re-queue them
    pub fn check_timeouts(&mut self, timeout: Duration) -> usize {
        let now = Instant::now();
        let mut timed_out = Vec::new();
        
        self.in_flight.retain(|entry| {
            let elapsed = now.duration_since(entry.requested_at);
            if elapsed > timeout {
                timed_out.push(entry.clone());
                false
            } else {
                true
            }
        });
        
        let count = timed_out.len();
        
        // Re-queue timed-out requests
        for entry in timed_out {
            self.queue.push_front(entry);
        }
        
        count
    }
    
    /// Get number of pending requests
    pub fn pending_count(&self) -> usize {
        self.queue.len()
    }
    
    /// Get number of in-flight requests
    pub fn in_flight_count(&self) -> usize {
        self.in_flight.len()
    }
    
    /// Check if queue is empty
    pub fn is_empty(&self) -> bool {
        self.queue.is_empty() && self.in_flight.is_empty()
    }
    
    /// Clear all pending and in-flight requests
    pub fn clear(&mut self) {
        self.queue.clear();
        self.in_flight.clear();
    }
}

impl Default for RetransmissionQueue {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_queue() {
        let queue = RetransmissionQueue::new();
        assert!(queue.is_empty());
        assert_eq!(queue.pending_count(), 0);
        assert_eq!(queue.in_flight_count(), 0);
    }

    #[test]
    fn test_enqueue() {
        let mut queue = RetransmissionQueue::new();
        
        assert!(queue.enqueue(1, "session-1".to_string()));
        assert_eq!(queue.pending_count(), 1);
        
        // Duplicate should be rejected
        assert!(!queue.enqueue(1, "session-1".to_string()));
        assert_eq!(queue.pending_count(), 1);
    }

    #[test]
    fn test_enqueue_batch() {
        let mut queue = RetransmissionQueue::new();
        
        let count = queue.enqueue_batch(vec![1, 2, 3, 4, 5], "session-1".to_string());
        assert_eq!(count, 5);
        assert_eq!(queue.pending_count(), 5);
    }

    #[test]
    fn test_dequeue_batch() {
        let mut queue = RetransmissionQueue::new();
        
        queue.enqueue_batch(vec![1, 2, 3, 4, 5], "session-1".to_string());
        
        let batch = queue.dequeue_batch(3);
        assert_eq!(batch.len(), 3);
        assert_eq!(queue.pending_count(), 2);
        assert_eq!(queue.in_flight_count(), 3);
    }

    #[test]
    fn test_mark_received() {
        let mut queue = RetransmissionQueue::new();
        
        queue.enqueue_batch(vec![1, 2, 3], "session-1".to_string());
        let batch = queue.dequeue_batch(3);
        
        assert_eq!(queue.in_flight_count(), 3);
        
        queue.mark_received(2);
        assert_eq!(queue.in_flight_count(), 2);
    }

    #[test]
    fn test_mark_failed() {
        let mut queue = RetransmissionQueue::new();
        
        queue.enqueue(1, "session-1".to_string());
        let batch = queue.dequeue_batch(1);
        
        assert_eq!(queue.in_flight_count(), 1);
        assert_eq!(queue.pending_count(), 0);
        
        queue.mark_failed(1);
        
        assert_eq!(queue.in_flight_count(), 0);
        assert_eq!(queue.pending_count(), 1);
    }

    #[test]
    fn test_timeout_checking() {
        let mut queue = RetransmissionQueue::new();
        
        queue.enqueue(1, "session-1".to_string());
        let batch = queue.dequeue_batch(1);
        
        // Wait a bit
        std::thread::sleep(Duration::from_millis(10));
        
        let timed_out = queue.check_timeouts(Duration::from_millis(5));
        assert_eq!(timed_out, 1);
        assert_eq!(queue.pending_count(), 1);
        assert_eq!(queue.in_flight_count(), 0);
    }

    #[test]
    fn test_max_queue_size() {
        let mut queue = RetransmissionQueue::with_config(
            5,
            Duration::from_secs(1),
            10,
        );
        
        // Fill to max
        for i in 0..5 {
            assert!(queue.enqueue(i, "session-1".to_string()));
        }
        
        // Next should fail
        assert!(!queue.enqueue(6, "session-1".to_string()));
    }

    #[test]
    fn test_max_in_flight() {
        let mut queue = RetransmissionQueue::with_config(
            100,
            Duration::from_secs(1),
            3, // Max 3 in flight
        );
        
        queue.enqueue_batch(vec![1, 2, 3, 4, 5], "session-1".to_string());
        
        let batch = queue.dequeue_batch(10); // Request 10 but should get only 3
        assert_eq!(batch.len(), 3);
        assert_eq!(queue.in_flight_count(), 3);
    }

    #[test]
    fn test_clear() {
        let mut queue = RetransmissionQueue::new();
        
        queue.enqueue_batch(vec![1, 2, 3], "session-1".to_string());
        queue.dequeue_batch(2);
        
        assert!(!queue.is_empty());
        
        queue.clear();
        assert!(queue.is_empty());
    }
}

