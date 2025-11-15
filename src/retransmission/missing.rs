// Track missing chunks and manage retransmission requests

use std::collections::{HashSet, VecDeque};
use std::time::{Duration, Instant};

/// Tracks missing chunks and manages retransmission requests
#[derive(Debug)]
pub struct MissingChunkTracker {
    /// Total number of chunks expected
    total_chunks: u64,
    
    /// Set of chunk IDs that have been received
    received_chunks: HashSet<u64>,
    
    /// Chunks that need retransmission
    pending_retransmit: HashSet<u64>,
    
    /// Chunks currently being retransmitted (with timestamp)
    in_flight: Vec<(u64, Instant)>,
    
    /// Maximum number of retries per chunk
    max_retries: u32,
    
    /// Retry count per chunk
    retry_counts: Vec<u32>,
    
    /// Timeout for retransmission requests
    timeout: Duration,
}

impl MissingChunkTracker {
    /// Create a new tracker for the given number of chunks
    pub fn new(total_chunks: u64) -> Self {
        Self {
            total_chunks,
            received_chunks: HashSet::new(),
            pending_retransmit: HashSet::new(),
            in_flight: Vec::new(),
            max_retries: 5,
            retry_counts: vec![0; total_chunks as usize],
            timeout: Duration::from_secs(5),
        }
    }
    
    /// Create with custom settings
    pub fn with_config(total_chunks: u64, max_retries: u32, timeout: Duration) -> Self {
        Self {
            total_chunks,
            received_chunks: HashSet::new(),
            pending_retransmit: HashSet::new(),
            in_flight: Vec::new(),
            max_retries,
            retry_counts: vec![0; total_chunks as usize],
            timeout,
        }
    }
    
    /// Mark a chunk as successfully received
    pub fn mark_received(&mut self, chunk_id: u64) {
        self.received_chunks.insert(chunk_id);
        self.pending_retransmit.remove(&chunk_id);
        self.in_flight.retain(|(id, _)| *id != chunk_id);
    }
    
    /// Mark a chunk as corrupted and needing retransmission
    pub fn mark_corrupted(&mut self, chunk_id: u64) {
        if chunk_id < self.total_chunks {
            self.received_chunks.remove(&chunk_id);
            self.pending_retransmit.insert(chunk_id);
            // Remove from in-flight if it was there
            self.in_flight.retain(|(id, _)| *id != chunk_id);
        }
    }
    
    /// Get list of all missing chunks
    pub fn get_missing(&self) -> Vec<u64> {
        (0..self.total_chunks)
            .filter(|id| !self.received_chunks.contains(id))
            .collect()
    }
    
    /// Get chunks that need retransmission (not in flight)
    pub fn get_pending_retransmit(&self) -> Vec<u64> {
        let in_flight_ids: HashSet<_> = self.in_flight.iter().map(|(id, _)| *id).collect();
        
        self.pending_retransmit
            .iter()
            .filter(|id| !in_flight_ids.contains(id))
            .filter(|id| self.retry_counts[**id as usize] < self.max_retries)
            .copied()
            .collect()
    }
    
    /// Get next batch of chunks to request retransmission for
    pub fn get_next_batch(&mut self, batch_size: usize) -> Vec<u64> {
        let mut batch: Vec<u64> = self.get_pending_retransmit()
            .into_iter()
            .take(batch_size)
            .collect();
        
        let now = Instant::now();
        
        // Check for timed-out in-flight requests
        let timed_out: Vec<u64> = self.in_flight
            .iter()
            .filter(|(_, timestamp)| now.duration_since(*timestamp) > self.timeout)
            .map(|(id, _)| *id)
            .collect();
        
        // Move timed-out chunks back to pending
        for chunk_id in timed_out {
            self.in_flight.retain(|(id, _)| *id != chunk_id);
            if self.retry_counts[chunk_id as usize] < self.max_retries {
                self.pending_retransmit.insert(chunk_id);
                batch.push(chunk_id);
            }
        }
        
        // Mark requested chunks as in-flight
        for &chunk_id in &batch {
            self.in_flight.push((chunk_id, now));
            self.retry_counts[chunk_id as usize] += 1;
            self.pending_retransmit.remove(&chunk_id);
        }
        
        batch
    }
    
    /// Check if all chunks are received
    pub fn is_complete(&self) -> bool {
        self.received_chunks.len() == self.total_chunks as usize
    }
    
    /// Get completion percentage
    pub fn completion_percentage(&self) -> f64 {
        if self.total_chunks == 0 {
            return 100.0;
        }
        (self.received_chunks.len() as f64 / self.total_chunks as f64) * 100.0
    }
    
    /// Get number of chunks received
    pub fn received_count(&self) -> usize {
        self.received_chunks.len()
    }
    
    /// Get chunks that have exceeded max retries
    pub fn get_failed_chunks(&self) -> Vec<u64> {
        (0..self.total_chunks)
            .filter(|id| !self.received_chunks.contains(id))
            .filter(|id| self.retry_counts[*id as usize] >= self.max_retries)
            .collect()
    }
    
    /// Check if transfer has failed (chunks exceeded max retries)
    pub fn has_failed(&self) -> bool {
        !self.get_failed_chunks().is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_tracker() {
        let tracker = MissingChunkTracker::new(10);
        assert_eq!(tracker.total_chunks, 10);
        assert_eq!(tracker.received_count(), 0);
        assert!(!tracker.is_complete());
    }

    #[test]
    fn test_mark_received() {
        let mut tracker = MissingChunkTracker::new(5);
        
        tracker.mark_received(0);
        tracker.mark_received(2);
        tracker.mark_received(4);
        
        assert_eq!(tracker.received_count(), 3);
        assert_eq!(tracker.get_missing(), vec![1, 3]);
    }

    #[test]
    fn test_mark_corrupted() {
        let mut tracker = MissingChunkTracker::new(5);
        
        tracker.mark_received(2);
        assert!(tracker.received_chunks.contains(&2));
        
        tracker.mark_corrupted(2);
        assert!(!tracker.received_chunks.contains(&2));
        assert!(tracker.pending_retransmit.contains(&2));
    }

    #[test]
    fn test_get_pending_retransmit() {
        let mut tracker = MissingChunkTracker::new(10);
        
        tracker.mark_corrupted(3);
        tracker.mark_corrupted(7);
        
        let pending = tracker.get_pending_retransmit();
        assert!(pending.contains(&3));
        assert!(pending.contains(&7));
    }

    #[test]
    fn test_get_next_batch() {
        let mut tracker = MissingChunkTracker::new(10);
        
        tracker.mark_corrupted(1);
        tracker.mark_corrupted(3);
        tracker.mark_corrupted(5);
        
        let batch = tracker.get_next_batch(2);
        assert_eq!(batch.len(), 2);
        
        // These chunks should now be in-flight
        assert_eq!(tracker.in_flight.len(), 2);
    }

    #[test]
    fn test_completion() {
        let mut tracker = MissingChunkTracker::new(3);
        
        assert!(!tracker.is_complete());
        assert_eq!(tracker.completion_percentage(), 0.0);
        
        tracker.mark_received(0);
        let pct = tracker.completion_percentage();
        assert!((pct - (100.0 / 3.0)).abs() < 0.001);
        
        tracker.mark_received(1);
        tracker.mark_received(2);
        
        assert!(tracker.is_complete());
        assert_eq!(tracker.completion_percentage(), 100.0);
    }

    #[test]
    fn test_max_retries() {
        let mut tracker = MissingChunkTracker::with_config(5, 2, Duration::from_secs(1));
        
        tracker.mark_corrupted(2);
        
        // First retry
        let batch1 = tracker.get_next_batch(1);
        assert_eq!(batch1, vec![2]);
        assert_eq!(tracker.retry_counts[2], 1);
        
        // Mark as failed to move from in-flight back to pending
        tracker.mark_corrupted(2);
        
        // Second retry (max reached after this)
        let batch2 = tracker.get_next_batch(1);
        assert_eq!(batch2, vec![2]);
        assert_eq!(tracker.retry_counts[2], 2);
        
        // Now retry_count is 2, which equals max_retries
        // So has_failed should be true and get_failed_chunks should return [2]
        let failed = tracker.get_failed_chunks();
        println!("Failed chunks: {:?}", failed);
        println!("Retry count for chunk 2: {}", tracker.retry_counts[2]);
        println!("Max retries: {}", tracker.max_retries);
        println!("Received chunks: {:?}", tracker.received_chunks);
        
        assert!(tracker.has_failed());
        assert_eq!(failed, vec![2]);
        
        // Should not retry again (exceeded max)
        tracker.mark_corrupted(2);
        let batch3 = tracker.get_next_batch(1);
        assert!(batch3.is_empty());
    }

    #[test]
    fn test_timeout_retry() {
        let mut tracker = MissingChunkTracker::with_config(
            5,
            3,
            Duration::from_millis(10)
        );
        
        tracker.mark_corrupted(1);
        
        // Request chunk
        let batch = tracker.get_next_batch(1);
        assert_eq!(batch, vec![1]);
        assert_eq!(tracker.in_flight.len(), 1);
        
        // Wait for timeout
        std::thread::sleep(Duration::from_millis(20));
        
        // Should retry timed-out chunk
        let batch2 = tracker.get_next_batch(1);
        assert_eq!(batch2, vec![1]);
        assert_eq!(tracker.retry_counts[1], 2);
    }
}

