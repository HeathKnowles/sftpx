# ChunkBitmap - Efficient Chunk Tracking

## Overview

`ChunkBitmap` is a highly efficient bitmap-based data structure for tracking which chunks have been received in a QUIC-based file transfer. It provides O(1) chunk lookup, minimal memory overhead (~1 bit per chunk), and automatic dynamic growth.

## Memory Efficiency

For a **1 GB file** with **64 KB chunks**:
- Total chunks: ~16,384
- Bitmap memory: **2 KB** (0.0002% of file size)
- Lookup time: O(1) constant time

## Core Features

### ✅ Implemented

1. **Bit-level chunk tracking** - 1 bit per chunk (minimal memory)
2. **Dynamic growth** - Power-of-2 resizing strategy
3. **Duplicate detection** - O(1) check before marking received
4. **EOF handling** - Tracks total chunks when EOF received
5. **Gap detection** - Find missing chunks for retransmission
6. **Progress tracking** - Real-time completion percentage
7. **Checksum integration** - Only mark received after verification

## API Reference

### Creation

```rust
use sftpx::chunking::ChunkBitmap;

// Create with initial capacity (dynamic growth)
let bitmap = ChunkBitmap::new(1024);

// Create with exact known size
let bitmap = ChunkBitmap::with_exact_size(10000);

// Default (lazy allocation)
let bitmap = ChunkBitmap::default();
```

### Core Operations

#### Mark Chunk as Received

**CRITICAL**: Only call this AFTER verifying the chunk's checksum.

```rust
let is_new = bitmap.mark_received(chunk_number, is_eof);

if is_new {
    println!("New chunk received");
} else {
    println!("Duplicate chunk ignored");
}
```

#### Check if Chunk Received

```rust
if bitmap.is_received(chunk_number) {
    println!("Already have this chunk");
}
```

#### Check Completion

```rust
if bitmap.is_complete() {
    println!("All chunks received!");
}
```

### Progress Tracking

```rust
let progress = bitmap.progress(); // 0.0 to 100.0
let received = bitmap.received_count();
let total = bitmap.total_chunks(); // Some(n) or None
```

### Gap Detection

```rust
// Find all missing chunks
let missing: Vec<u32> = bitmap.find_missing();

// Find first N missing (for prioritized retransmission)
let first_five = bitmap.find_first_missing(5);

// Find missing in specific range
let missing_in_range = bitmap.find_missing_in_range(100, 200);

// Find contiguous gaps as ranges
let gaps: Vec<(u32, u32)> = bitmap.find_gaps();
// Example: [(5, 10), (20, 25)] means chunks 5-10 and 20-25 are missing
```

## Integration with QUIC Chunks

### Chunk Structure (FlatBuffers)

Assuming your FlatBuffers chunk table has:

```flatbuffers
table Chunk {
    chunk_number: uint32;
    byte_offset: uint64;
    chunk_length: uint32;
    checksum: [ubyte];
    end_of_file_flag: bool;
}
```

### Receive Flow

```rust
use sftpx::chunking::ChunkBitmap;

struct ChunkReceiver {
    bitmap: ChunkBitmap,
    output_file: File,
}

impl ChunkReceiver {
    fn on_chunk_received(&mut self, chunk: &ChunkTable) -> Result<()> {
        let chunk_num = chunk.chunk_number();
        let is_eof = chunk.end_of_file_flag();
        
        // Step 1: Check for duplicate BEFORE processing
        if self.bitmap.is_received(chunk_num) {
            return Ok(()); // Ignore duplicate
        }
        
        // Step 2: Verify checksum
        let computed_hash = compute_checksum(chunk.data());
        if computed_hash != chunk.checksum() {
            return Err("Checksum mismatch".into());
        }
        
        // Step 3: Write to file at correct offset
        self.output_file.seek(SeekFrom::Start(chunk.byte_offset()))?;
        self.output_file.write_all(chunk.data())?;
        
        // Step 4: Mark as received ONLY after successful write
        self.bitmap.mark_received(chunk_num, is_eof);
        
        // Step 5: Check completion
        if self.bitmap.is_complete() {
            println!("Transfer complete!");
        }
        
        Ok(())
    }
}
```

## Usage Patterns

### Pattern 1: Known File Size

When the server sends total chunk count upfront:

```rust
// Server sends metadata first
let total_chunks = metadata.total_chunks;
let mut bitmap = ChunkBitmap::with_exact_size(total_chunks);

// Receive chunks in any order
for chunk in chunks {
    bitmap.mark_received(chunk.number, false);
}
```

### Pattern 2: Unknown Size (Stream)

When file size is unknown until EOF:

```rust
let mut bitmap = ChunkBitmap::new(0); // Start small, grow dynamically

loop {
    let chunk = receive_chunk()?;
    let is_eof = chunk.end_of_file_flag();
    
    bitmap.mark_received(chunk.number, is_eof);
    
    if is_eof && bitmap.is_complete() {
        break;
    }
}
```

### Pattern 3: Retransmission Requests

Request missing chunks periodically:

```rust
use std::time::{Duration, Instant};

let mut last_request = Instant::now();
let request_interval = Duration::from_secs(2);

loop {
    // Receive chunks...
    
    if last_request.elapsed() > request_interval && !bitmap.is_complete() {
        let missing = bitmap.find_first_missing(10); // Request 10 at a time
        
        for chunk_num in missing {
            send_retransmit_request(chunk_num);
        }
        
        last_request = Instant::now();
    }
    
    if bitmap.is_complete() {
        break;
    }
}
```

### Pattern 4: Progress Reporting

Real-time progress updates:

```rust
use indicatif::{ProgressBar, ProgressStyle};

let total = metadata.total_chunks;
let bar = ProgressBar::new(total as u64);
bar.set_style(ProgressStyle::default_bar()
    .template("{wide_bar} {pos}/{len} chunks ({percent}%)")
);

loop {
    let chunk = receive_chunk()?;
    
    if bitmap.mark_received(chunk.number, chunk.is_eof) {
        bar.set_position(bitmap.received_count() as u64);
    }
    
    if bitmap.is_complete() {
        bar.finish();
        break;
    }
}
```

## Performance Characteristics

| Operation | Time Complexity | Space Complexity |
|-----------|----------------|------------------|
| `mark_received` | O(1) | O(1) |
| `is_received` | O(1) | O(1) |
| `is_complete` | O(1) | O(1) |
| `find_missing` | O(n) | O(m) where m = missing count |
| `find_gaps` | O(n) | O(g) where g = gap count |
| Dynamic growth | Amortized O(1) | O(n) where n = new capacity |

## Best Practices

### ✅ DO

1. **Verify checksum before marking received**
   ```rust
   if verify_checksum(&chunk) {
       bitmap.mark_received(chunk.number, chunk.is_eof);
   }
   ```

2. **Handle duplicates efficiently**
   ```rust
   if bitmap.is_received(chunk_num) {
       return; // Early exit, don't process duplicates
   }
   ```

3. **Request missing chunks in batches**
   ```rust
   let missing = bitmap.find_first_missing(10);
   ```

4. **Use exact size if known**
   ```rust
   let bitmap = ChunkBitmap::with_exact_size(total_chunks);
   ```

### ❌ DON'T

1. **Don't mark received before checksum verification**
   ```rust
   // BAD: Corrupted chunk pollutes bitmap
   bitmap.mark_received(chunk.number, chunk.is_eof);
   if !verify_checksum(&chunk) { /* too late! */ }
   ```

2. **Don't use byte offset for bitmap indexing**
   ```rust
   // BAD: Use chunk_number, not byte_offset
   bitmap.mark_received(chunk.byte_offset, false); // WRONG
   ```

3. **Don't request all missing chunks at once**
   ```rust
   // BAD: Could be thousands of chunks
   let all_missing = bitmap.find_missing();
   for chunk in all_missing { /* network flood */ }
   ```

## Thread Safety

`ChunkBitmap` is **not** thread-safe by default. Wrap in `Arc<Mutex<>>` for concurrent access:

```rust
use std::sync::{Arc, Mutex};

let bitmap = Arc::new(Mutex::new(ChunkBitmap::new(1024)));

// Thread 1
let bitmap_clone = bitmap.clone();
std::thread::spawn(move || {
    let mut bm = bitmap_clone.lock().unwrap();
    bm.mark_received(0, false);
});
```

## Examples

See `examples/bitmap_usage.rs` for complete examples including:

- Basic usage with known/unknown size
- Real-world transfer simulation
- Duplicate detection
- Memory efficiency demonstration
- Selective retransmission
- Gap detection

Run: `cargo run --example bitmap_usage`

## Testing

Run the test suite:

```bash
cargo test chunking::bitmap
```

All 10 tests covering:
- Creation and initialization
- Marking received/duplicates
- EOF handling and completion
- Missing chunk detection
- Gap finding
- Dynamic growth
- Memory efficiency
- Progress calculation
- Reset functionality

## See Also

- [QUIC Protocol RFC 9000](https://www.rfc-editor.org/rfc/rfc9000.html)
- [Engineering Documentation](../FIXES.md)
- [Architecture Overview](../ARCHITECTURE.md)
