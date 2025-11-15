// Bitmap for tracking received chunks

/// Efficient bitmap for tracking which chunks have been received.
/// Uses bit-level operations for minimal memory overhead.
/// 
/// Memory usage: ~1 bit per chunk (~16KB bitmap for 1GB file with 64KB chunks)
pub struct ChunkBitmap {
    /// The actual bitmap storage (1 bit = 1 chunk)
    bitmap: Vec<u8>,
    /// Total number of chunks expected (known after EOF received)
    total_chunks: Option<u32>,
    /// Number of chunks successfully received and verified
    received_count: u32,
    /// Whether we've seen the chunk with EOF flag
    have_eof: bool,
    /// Current capacity in number of chunks (may exceed total_chunks during dynamic growth)
    capacity: u32,
}

impl ChunkBitmap {
    /// Create a new bitmap with initial capacity
    /// 
    /// # Arguments
    /// * `initial_capacity` - Initial number of chunks to allocate for (or 0 for lazy allocation)
    /// 
    /// 
    /// pub fn new(_total_chunks: u64) -> Self {
        // Self {}
    // }
    /// 
    pub fn new(initial_capacity: u32) -> Self {
        let capacity = if initial_capacity > 0 {
            initial_capacity
        } else {
            1024 // Default: support 1024 chunks initially (~64MB with 64KB chunks)
        };
        
        let bitmap_bytes = Self::capacity_to_bytes(capacity);
        
        Self {
            bitmap: vec![0u8; bitmap_bytes],
            total_chunks: None,
            received_count: 0,
            have_eof: false,
            capacity,
        }
    }
    
    /// Create a bitmap with exact known size
    pub fn with_exact_size(total_chunks: u32) -> Self {
        let bitmap_bytes = Self::capacity_to_bytes(total_chunks);
        
        Self {
            bitmap: vec![0u8; bitmap_bytes],
            total_chunks: Some(total_chunks),
            received_count: 0,
            have_eof: false,
            capacity: total_chunks,
        }
    }
    
    /// Calculate bytes needed for given chunk capacity
    #[inline]
    fn capacity_to_bytes(capacity: u32) -> usize {
        ((capacity + 7) / 8) as usize
    }
    
    /// Check if a chunk has been received
    /// 
    /// # Arguments
    /// * `chunk_number` - The chunk index to check
    /// 
    /// # Returns
    /// `true` if chunk was previously received, `false` otherwise
    #[inline]
    pub fn is_received(&self, chunk_number: u32) -> bool {
        if chunk_number >= self.capacity {
            return false;
        }
        
        let byte_idx = (chunk_number >> 3) as usize; // chunk_number / 8
        let bit_idx = (chunk_number & 7) as u8;      // chunk_number % 8
        
        (self.bitmap[byte_idx] & (1 << bit_idx)) != 0
    }
    
    /// Mark a chunk as received
    /// 
    /// This should only be called AFTER verifying the chunk's checksum.
    /// 
    /// # Arguments
    /// * `chunk_number` - The chunk index to mark
    /// * `is_eof` - Whether this chunk has the end-of-file flag
    /// 
    /// # Returns
    /// `true` if this is a new chunk, `false` if it was a duplicate
    pub fn mark_received(&mut self, chunk_number: u32, is_eof: bool) -> bool {
        // Grow bitmap if needed (only if we haven't seen EOF yet)
        if chunk_number >= self.capacity && !self.have_eof {
            self.grow_to_fit(chunk_number);
        }
        
        // Check if we're beyond known size
        if let Some(total) = self.total_chunks {
            if chunk_number >= total {
                // This shouldn't happen if sender is well-behaved
                return false;
            }
        }
        
        // Check for duplicate
        if self.is_received(chunk_number) {
            return false; // Duplicate - already received
        }
        
        // Set the bit
        let byte_idx = (chunk_number >> 3) as usize;
        let bit_idx = (chunk_number & 7) as u8;
        self.bitmap[byte_idx] |= 1 << bit_idx;
        
        // Update counters
        self.received_count += 1;
        
        // Track EOF
        if is_eof {
            self.have_eof = true;
            self.total_chunks = Some(chunk_number + 1);
        }
        
        true // New chunk
    }
    
    /// Grow the bitmap to accommodate the given chunk number
    /// Uses power-of-2 growth strategy for amortized O(1) insertion
    fn grow_to_fit(&mut self, chunk_number: u32) {
        let required_capacity = chunk_number + 1;
        
        // Find next power of 2 that fits
        let new_capacity = required_capacity.next_power_of_two().max(self.capacity * 2);
        
        let new_bytes = Self::capacity_to_bytes(new_capacity);
        
        // Resize and zero-fill new bytes
        self.bitmap.resize(new_bytes, 0);
        self.capacity = new_capacity;
    }
    
    /// Check if all chunks have been received
    /// 
    /// # Returns
    /// `true` if we've seen the EOF chunk and received all chunks
    pub fn is_complete(&self) -> bool {
        self.have_eof && 
        self.total_chunks.map_or(false, |total| self.received_count == total)
    }
    
    /// Get the total number of chunks (if known)
    pub fn total_chunks(&self) -> Option<u32> {
        self.total_chunks
    }
    
    /// Get the number of chunks received so far
    pub fn received_count(&self) -> u32 {
        self.received_count
    }
    
    /// Calculate completion percentage
    /// 
    /// # Returns
    /// Percentage (0.0 to 100.0) if total is known, or 0.0 if not
    pub fn progress(&self) -> f64 {
        match self.total_chunks {
            Some(total) if total > 0 => {
                (self.received_count as f64 / total as f64) * 100.0
            }
            _ => 0.0
        }
    }
    
    /// Find all missing chunks
    /// 
    /// # Returns
    /// Vector of missing chunk numbers (for retransmission requests)
    pub fn find_missing(&self) -> Vec<u32> {
        let mut missing = Vec::new();
        
        if let Some(total) = self.total_chunks {
            for chunk_num in 0..total {
                if !self.is_received(chunk_num) {
                    missing.push(chunk_num);
                }
            }
        }
        
        missing
    }
    
    /// Find missing chunks in a specific range
    /// Useful for selective retransmission
    pub fn find_missing_in_range(&self, start: u32, end: u32) -> Vec<u32> {
        let mut missing = Vec::new();
        let limit = self.total_chunks.unwrap_or(self.capacity).min(end);
        
        for chunk_num in start..limit {
            if !self.is_received(chunk_num) {
                missing.push(chunk_num);
            }
        }
        
        missing
    }
    
    /// Find the first N missing chunks
    /// Useful for prioritized retransmission
    pub fn find_first_missing(&self, max_count: usize) -> Vec<u32> {
        let mut missing = Vec::new();
        
        if let Some(total) = self.total_chunks {
            for chunk_num in 0..total {
                if !self.is_received(chunk_num) {
                    missing.push(chunk_num);
                    if missing.len() >= max_count {
                        break;
                    }
                }
            }
        }
        
        missing
    }
    
    /// Find contiguous gaps in received chunks
    /// Returns ranges of missing chunks as (start, end) pairs
    pub fn find_gaps(&self) -> Vec<(u32, u32)> {
        let mut gaps = Vec::new();
        
        if let Some(total) = self.total_chunks {
            let mut gap_start: Option<u32> = None;
            
            for chunk_num in 0..total {
                if !self.is_received(chunk_num) {
                    // Start or continue gap
                    if gap_start.is_none() {
                        gap_start = Some(chunk_num);
                    }
                } else {
                    // End gap if one was in progress
                    if let Some(start) = gap_start {
                        gaps.push((start, chunk_num - 1));
                        gap_start = None;
                    }
                }
            }
            
            // Close final gap if it extends to end
            if let Some(start) = gap_start {
                gaps.push((start, total - 1));
            }
        }
        
        gaps
    }
    
    /// Reset the bitmap to empty state
    pub fn reset(&mut self) {
        for byte in &mut self.bitmap {
            *byte = 0;
        }
        self.received_count = 0;
        self.have_eof = false;
        self.total_chunks = None;
    }
    
    /// Get memory usage in bytes
    pub fn memory_usage(&self) -> usize {
        self.bitmap.len()
    }
    
    /// Check if we've seen the EOF chunk
    pub fn has_eof(&self) -> bool {
        self.have_eof
    }
}

impl Default for ChunkBitmap {
    fn default() -> Self {
        Self::new(0)
    }
}

impl std::fmt::Debug for ChunkBitmap {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChunkBitmap")
            .field("total_chunks", &self.total_chunks)
            .field("received_count", &self.received_count)
            .field("have_eof", &self.have_eof)
            .field("capacity", &self.capacity)
            .field("memory_bytes", &self.bitmap.len())
            .field("progress", &format!("{:.2}%", self.progress()))
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_bitmap() {
        let bitmap = ChunkBitmap::new(100);
        assert_eq!(bitmap.received_count(), 0);
        assert_eq!(bitmap.total_chunks(), None);
        assert!(!bitmap.is_complete());
    }

    #[test]
    fn test_mark_received() {
        let mut bitmap = ChunkBitmap::new(10);
        
        assert!(bitmap.mark_received(0, false)); // New chunk
        assert!(!bitmap.mark_received(0, false)); // Duplicate
        
        assert_eq!(bitmap.received_count(), 1);
        assert!(bitmap.is_received(0));
        assert!(!bitmap.is_received(1));
    }

    #[test]
    fn test_eof_handling() {
        let mut bitmap = ChunkBitmap::new(10);
        
        bitmap.mark_received(0, false);
        bitmap.mark_received(1, false);
        bitmap.mark_received(4, true); // EOF on chunk 4 (total = 5 chunks)
        
        assert_eq!(bitmap.total_chunks(), Some(5));
        assert!(!bitmap.is_complete()); // Missing chunks 2 and 3
        
        // Receive remaining chunks
        bitmap.mark_received(2, false);
        bitmap.mark_received(3, false);
        
        // Now all chunks received
        assert!(bitmap.is_complete());
    }

    #[test]
    fn test_completion() {
        let mut bitmap = ChunkBitmap::new(10);
        
        for i in 0..5 {
            bitmap.mark_received(i, i == 4);
        }
        
        assert!(bitmap.is_complete());
        assert_eq!(bitmap.progress(), 100.0);
    }

    #[test]
    fn test_find_missing() {
        let mut bitmap = ChunkBitmap::new(10);
        
        bitmap.mark_received(0, false);
        bitmap.mark_received(2, false);
        bitmap.mark_received(4, true); // EOF - total 5 chunks
        
        let missing = bitmap.find_missing();
        assert_eq!(missing, vec![1, 3]);
    }

    #[test]
    fn test_find_gaps() {
        let mut bitmap = ChunkBitmap::new(20);
        
        bitmap.mark_received(0, false);
        bitmap.mark_received(1, false);
        // Gap: 2-4
        bitmap.mark_received(5, false);
        // Gap: 6-8
        bitmap.mark_received(9, true); // EOF - total 10
        
        let gaps = bitmap.find_gaps();
        assert_eq!(gaps, vec![(2, 4), (6, 8)]);
    }

    #[test]
    fn test_dynamic_growth() {
        let mut bitmap = ChunkBitmap::new(10);
        
        // Should grow to accommodate
        bitmap.mark_received(1000, false);
        assert!(bitmap.is_received(1000));
        assert!(bitmap.capacity >= 1001);
    }

    #[test]
    fn test_progress() {
        let mut bitmap = ChunkBitmap::new(10);
        
        for i in 0..10 {
            bitmap.mark_received(i, i == 9);
        }
        
        assert_eq!(bitmap.progress(), 100.0);
    }

    #[test]
    fn test_memory_efficiency() {
        let bitmap = ChunkBitmap::new(10000);
        // 10000 chunks = 1250 bytes
        assert_eq!(bitmap.memory_usage(), 1250);
    }

    #[test]
    fn test_reset() {
        let mut bitmap = ChunkBitmap::new(10);
        bitmap.mark_received(0, false);
        bitmap.mark_received(1, true);
        
        bitmap.reset();
        
        assert_eq!(bitmap.received_count(), 0);
        assert!(!bitmap.has_eof());
        assert!(!bitmap.is_received(0));
    }
}
