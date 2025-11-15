# SFTPX Chunking Protocol Documentation

## Overview

The SFTPX chunking protocol enables reliable, resumable file transfers over QUIC by breaking files into fixed-size chunks with metadata. Each chunk is independently verifiable and can be transmitted out-of-order, making the protocol robust against network interruptions.

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                     File Transfer Flow                       │
└─────────────────────────────────────────────────────────────┘

Server Side:                                Client Side:
┌──────────────┐                            ┌──────────────┐
│  File        │                            │  FileReceiver│
│  large.dat   │                            │  (.part)     │
└──────┬───────┘                            └──────▲───────┘
       │                                           │
       ▼                                           │
┌──────────────┐                            ┌──────────────┐
│ FileChunker  │                            │ ChunkPacket  │
│              │                            │ Parser       │
│ - Read 1MB   │                            └──────▲───────┘
│ - Compute    │                                   │
│   BLAKE3     │                                   │
│ - Build      │      ┌──────────────┐            │
│   Protobuf   │─────▶│ QUIC Stream  │────────────┘
└──────────────┘      │ (Data)       │
                      └──────────────┘
                      
       Chunk #0 ────────────────────────▶ Write @ offset 0
       Chunk #1 ────────────────────────▶ Write @ offset 1MB
       Chunk #2 ────────────────────────▶ Write @ offset 2MB
       ...
       Chunk #N (EOF=true) ─────────────▶ Finalize & Rename
```

## Protocol Buffer Schema

### ChunkPacket Message

```protobuf
syntax = "proto3";

package sftpx.protocol;

message ChunkPacket {
  uint64 chunk_id = 1;       // Sequential chunk number (0-indexed)
  uint64 byte_offset = 2;    // Absolute position in file
  uint32 chunk_length = 3;   // Size of data in this chunk
  bytes checksum = 4;        // BLAKE3 hash (32 bytes)
  bool end_of_file = 5;      // true if last chunk
  bytes data = 6;            // Chunk payload
}
```

### Field Descriptions

| Field | Type | Description | Notes |
|-------|------|-------------|-------|
| `chunk_id` | uint64 | Unique sequential identifier | Starts at 0, increments by 1 |
| `byte_offset` | uint64 | Starting position in file | Allows out-of-order assembly |
| `chunk_length` | uint32 | Length of `data` field | Must match actual data length |
| `checksum` | bytes | BLAKE3 hash of `data` | 32 bytes, verified on receive |
| `end_of_file` | bool | Last chunk indicator | Triggers finalization |
| `data` | bytes | Actual chunk content | Up to chunk_size bytes |

## Core Components

### 1. FileChunker (Server-Side)

**Location**: `src/chunking/chunker.rs`

Reads files and produces chunk packets.

```rust
use sftpx::chunking::FileChunker;
use std::path::Path;

// Create chunker with custom chunk size (default: 1MB)
let mut chunker = FileChunker::new(
    Path::new("large_file.dat"),
    Some(1024 * 1024) // 1MB chunks
)?;

// Get file metadata
let total_chunks = chunker.total_chunks();
let file_size = chunker.file_size();

// Iterate through chunks
while let Some(chunk_packet) = chunker.next_chunk()? {
    // chunk_packet is a serialized protobuf ready to send
    send_over_quic(stream_id, &chunk_packet)?;
}
```

**Key Methods**:
- `new(path, chunk_size)` - Create chunker for a file
- `next_chunk()` - Read and encode next chunk (returns `Option<Vec<u8>>`)
- `total_chunks()` - Calculate total number of chunks
- `progress()` - Get progress as 0.0-1.0
- `seek_to_chunk(id)` - Jump to specific chunk (for retransmission)
- `reset()` - Start from beginning

### 2. FileReceiver (Client-Side)

**Location**: `src/client/receiver.rs`

Receives and assembles chunk packets into a file.

```rust
use sftpx::client::FileReceiver;
use std::path::Path;

// Create receiver
let mut receiver = FileReceiver::new(
    Path::new("/output"),    // Output directory
    "large_file.dat",        // Filename
    expected_file_size       // Pre-allocate space
)?;

// Process each received packet
for packet_data in received_packets {
    let chunk = receiver.receive_chunk(&packet_data)?;
    
    // chunk is automatically:
    // - Verified (checksum)
    // - Written to correct offset
    // - Tracked for duplicates
    
    println!("Progress: {:.1}%", receiver.progress() * 100.0);
}

// Check completion
if receiver.is_complete() {
    let final_path = receiver.finalize()?;
    println!("File saved: {}", final_path.display());
} else {
    let missing = receiver.missing_chunks();
    println!("Missing chunks: {:?}", missing);
}
```

**Key Methods**:
- `new(dir, filename, size)` - Create receiver, pre-allocate `.part` file
- `receive_chunk(data)` - Parse, verify, and write chunk
- `is_complete()` - Check if all chunks received
- `missing_chunks()` - Get list of missing chunk IDs
- `progress()` - Get progress as 0.0-1.0
- `finalize()` - Verify completeness, rename `.part` to final file
- `stats()` - Get detailed statistics

### 3. ChunkPacketBuilder & Parser

**Location**: `src/protocol/chunk.rs`

Low-level protobuf serialization/deserialization.

```rust
use sftpx::protocol::chunk::{ChunkPacketBuilder, ChunkPacketParser};

// Building (usually done internally by FileChunker)
let mut builder = ChunkPacketBuilder::new();
let packet_bytes = builder.build(
    chunk_id,
    byte_offset,
    chunk_length,
    &checksum,
    end_of_file,
    &data
)?;

// Parsing (usually done internally by FileReceiver)
let chunk_view = ChunkPacketParser::parse(&packet_bytes)?;
assert_eq!(chunk_view.chunk_id, expected_id);
chunk_view.verify_checksum()?; // Throws error if mismatch
```

### 4. ChunkHasher

**Location**: `src/chunking/hasher.rs`

BLAKE3 hashing utilities.

```rust
use sftpx::chunking::ChunkHasher;

// Compute hash
let hash = ChunkHasher::hash(data);

// Verify data
assert!(ChunkHasher::verify(data, &expected_hash));
```

## Integration Points

### Server Integration

The `DataSender` in `src/server/sender.rs` provides high-level file sending:

```rust
use sftpx::server::DataSender;

let mut sender = DataSender::new();

// Send entire file as chunks
sender.send_file(
    &mut connection,
    data_stream_id,  // One of the 4 QUIC streams
    Path::new("file.dat"),
    Some(1024 * 1024) // Optional chunk size
)?;

// Track progress
println!("Sent {} bytes in {} chunks",
    sender.total_bytes_sent(),
    sender.total_chunks_sent()
);
```

### Client Integration

The `FileReceiver` handles incoming chunks:

```rust
// In your QUIC event loop
match stream_event {
    StreamData { stream_id, data } if stream_id == DATA_STREAM => {
        receiver.receive_chunk(&data)?;
        
        if receiver.is_complete() {
            receiver.finalize()?;
        }
    }
    // ... other events
}
```

## Advanced Use Cases

### 1. Resumable Transfers

```rust
// Server: Resume from specific chunk
let mut chunker = FileChunker::new(path, chunk_size)?;
chunker.seek_to_chunk(resume_chunk_id)?;

// Continue from where we left off
while let Some(packet) = chunker.next_chunk()? {
    send_packet(&packet)?;
}

// Client: Identify what's missing
let missing = receiver.missing_chunks();
send_retransmit_request(&missing)?;
```

### 2. Parallel Chunk Transmission

```rust
// Split chunk range across multiple streams
let total = chunker.total_chunks();
let per_stream = total / num_streams;

for stream_id in 0..num_streams {
    let start_chunk = stream_id * per_stream;
    let end_chunk = if stream_id == num_streams - 1 {
        total
    } else {
        (stream_id + 1) * per_stream
    };
    
    spawn_stream_sender(stream_id, start_chunk, end_chunk);
}
```

### 3. Selective Chunk Retransmission

```rust
// Client requests specific chunks
let missing = receiver.missing_chunks();
send_control_message(ControlMessage::RequestChunks(missing))?;

// Server retransmits only requested chunks
for chunk_id in requested_chunks {
    chunker.seek_to_chunk(chunk_id)?;
    if let Some(packet) = chunker.next_chunk()? {
        send_packet(&packet)?;
    }
}
```

### 4. Priority-Based Transmission

```rust
// Send initial chunks first for streaming/preview
let preview_chunks = vec![0, 1, 2]; // First 3 chunks
for id in preview_chunks {
    chunker.seek_to_chunk(id)?;
    send_packet(&chunker.next_chunk()?.unwrap())?;
}

// Then send rest in order
chunker.seek_to_chunk(3)?;
while let Some(packet) = chunker.next_chunk()? {
    send_packet(&packet)?;
}
```

## Error Handling

### Common Errors

| Error | Cause | Handling |
|-------|-------|----------|
| `Protocol("Checksum mismatch")` | Data corruption | Request retransmission |
| `Protocol("Chunk size mismatch")` | Invalid packet | Discard, log error |
| `Io(...)` | File I/O failure | Check permissions, disk space |
| `SerializationError` | Invalid protobuf | Bug or version mismatch |
| `DeserializationError` | Corrupted packet | Request retransmission |

### Best Practices

```rust
// Robust chunk reception
match receiver.receive_chunk(&packet) {
    Ok(chunk) => {
        log::info!("Received chunk {}", chunk.chunk_id);
    }
    Err(Error::Protocol(msg)) if msg.contains("Duplicate") => {
        // Harmless duplicate, ignore
        log::debug!("Ignoring duplicate chunk");
    }
    Err(Error::Protocol(msg)) if msg.contains("Checksum") => {
        // Request retransmission
        request_retransmit(chunk_id)?;
    }
    Err(e) => {
        log::error!("Chunk reception failed: {}", e);
        return Err(e);
    }
}
```

## Performance Considerations

### Chunk Size Selection

```rust
// Trade-offs:
// Small chunks (64KB - 256KB):
// + Lower memory usage
// + Faster retransmission
// - More protobuf overhead
// - More system calls

// Large chunks (1MB - 10MB):
// + Less overhead
// + Fewer system calls
// - Higher memory usage
// - Slower retransmission

// Recommended: 1MB for most use cases
let chunk_size = 1024 * 1024;
```

### Memory Management

```rust
// FileReceiver pre-allocates file
// No need to keep all chunks in memory
let receiver = FileReceiver::new(
    output_dir,
    filename,
    file_size // Pre-allocates disk space
)?;

// Each chunk is written immediately and dropped
// Memory usage: ~1-2x chunk_size regardless of file size
```

### Parallelization

The protocol supports parallel transmission:

```rust
// Server: Multiple senders on different streams
for stream_id in data_stream_ids {
    let chunker = FileChunker::new(path, chunk_size)?;
    spawn_sender(stream_id, chunker);
}

// Client: Single receiver handles all streams
// Thread-safe with mutex if needed
let receiver = Arc::new(Mutex::new(receiver));
```

## Testing

### Unit Tests

```rust
// Run all chunking tests
cargo test --lib chunking::

// Specific test
cargo test --lib test_chunker_small_file
```

### Integration Test

```rust
// Run the example
cargo run --example chunk_test

// Creates a test file, chunks it, reassembles, verifies
```

### Property Testing (Future)

```rust
// Properties to test:
// 1. Chunking then reassembly = original file
// 2. Out-of-order chunks still reassemble correctly
// 3. Any subset of chunks can be retransmitted
// 4. Checksum always catches corruption
```

## Future Enhancements

### 1. Chunk Priority Queue
```rust
// Implement priority-based chunk transmission
struct ChunkQueue {
    high_priority: VecDeque<ChunkId>,
    normal_priority: VecDeque<ChunkId>,
    retransmits: HashSet<ChunkId>,
}
```

### 2. Chunk Compression
```rust
// Add optional compression field to protobuf
message ChunkPacket {
    // ... existing fields ...
    CompressionType compression = 7;
    uint32 original_length = 8; // Before compression
}

enum CompressionType {
    NONE = 0;
    ZSTD = 1;
    LZ4 = 2;
}
```

### 3. Erasure Coding
```rust
// Add redundant chunks for fault tolerance
// Generate N data chunks + M parity chunks
// Any N chunks can reconstruct the file
```

### 4. Adaptive Chunk Sizing
```rust
// Adjust chunk size based on network conditions
impl FileChunker {
    fn adjust_chunk_size(&mut self, rtt: Duration, bandwidth: u64) {
        // Smaller chunks for high-latency networks
        // Larger chunks for high-bandwidth networks
    }
}
```

### 5. Chunk Manifests
```rust
// Send manifest before chunks
message ChunkManifest {
    uint64 total_chunks = 1;
    uint64 file_size = 2;
    bytes file_hash = 3;
    repeated ChunkInfo chunks = 4;
}

message ChunkInfo {
    uint64 chunk_id = 1;
    uint64 byte_offset = 2;
    uint32 chunk_length = 3;
    bytes checksum = 4;
}
```

### 6. Bitmap Tracking
```rust
// Efficient tracking of received chunks
// Currently using HashSet, could use bitmap for millions of chunks
use bitvec::prelude::*;

struct ChunkBitmap {
    bits: BitVec,
    total_chunks: u64,
}

impl ChunkBitmap {
    fn mark_received(&mut self, chunk_id: u64) {
        self.bits.set(chunk_id as usize, true);
    }
    
    fn is_complete(&self) -> bool {
        self.bits.count_ones() == self.total_chunks as usize
    }
    
    fn missing_chunks(&self) -> Vec<u64> {
        self.bits.iter_zeros()
            .map(|i| i as u64)
            .collect()
    }
}
```

## Debugging Tips

### Enable Logging

```rust
// Set environment variable
RUST_LOG=sftpx=debug cargo run

// View chunk operations
// DEBUG sftpx::chunking: Reading chunk 5 at offset 5242880
// DEBUG sftpx::protocol: Encoded chunk packet: 8196 bytes
// DEBUG sftpx::client: Received chunk 5, verified checksum
```

### Inspect Packets

```rust
use sftpx::protocol::chunk::ChunkPacketParser;

// Parse and inspect packet
let chunk = ChunkPacketParser::parse(&packet_bytes)?;
println!("Chunk ID: {}", chunk.chunk_id);
println!("Offset: {}", chunk.byte_offset);
println!("Length: {}", chunk.chunk_length);
println!("Checksum: {:x?}", &chunk.checksum[..8]);
println!("EOF: {}", chunk.end_of_file);
println!("Data preview: {:x?}", &chunk.data[..16]);
```

### Monitor Progress

```rust
// Server side
loop {
    if let Some(packet) = chunker.next_chunk()? {
        send(&packet)?;
        println!("Sent chunk {}/{} ({:.1}%)",
            chunker.current_chunk(),
            chunker.total_chunks(),
            chunker.progress() * 100.0
        );
    } else {
        break;
    }
}

// Client side
let chunk = receiver.receive_chunk(&packet)?;
println!("Received {}/{} chunks ({:.1}%)",
    receiver.stats().chunks_received,
    receiver.stats().total_chunks,
    receiver.progress() * 100.0
);
```

## References

- Protocol Buffers: https://protobuf.dev/
- BLAKE3 Hashing: https://github.com/BLAKE3-team/BLAKE3
- QUIC Protocol: https://www.rfc-editor.org/rfc/rfc9000.html
- Prost (Rust Protobuf): https://github.com/tokio-rs/prost

## API Summary

### Types
```rust
pub struct FileChunker { /* ... */ }
pub struct FileReceiver { /* ... */ }
pub struct ChunkPacketBuilder;
pub struct ChunkPacketParser;
pub struct ChunkHasher;

pub struct ChunkPacketView {
    pub chunk_id: u64,
    pub byte_offset: u64,
    pub chunk_length: u32,
    pub checksum: Vec<u8>,
    pub end_of_file: bool,
    pub data: Vec<u8>,
}

pub struct ReceiverStats {
    pub bytes_received: u64,
    pub chunks_received: u64,
    pub total_chunks: u64,
    pub is_complete: bool,
    pub progress: f64,
}
```

### Module Exports
```rust
// From sftpx::chunking
pub use FileChunker;
pub use ChunkHasher;

// From sftpx::protocol::chunk
pub use ChunkPacketBuilder;
pub use ChunkPacketParser;
pub use ChunkPacketView;

// From sftpx::client
pub use FileReceiver;
```
