# Chunking Module Summary

The chunking module provides a complete system for splitting files into chunks, computing checksums, and tracking chunk reception for QUIC-based file transfers.

## Module Structure

```
src/chunking/
├── mod.rs          - Module exports
├── bitmap.rs       - Efficient chunk reception tracking (427 lines)
├── table.rs        - Chunk metadata storage (470 lines)
├── chunker.rs      - File splitting and chunk creation (233 lines)
└── hasher.rs       - Checksum computation and verification (54 lines)
```

## Components Overview

### 1. ChunkBitmap (`bitmap.rs`)
**Purpose**: Efficiently track which chunks have been received using bit-level operations

**Key Features**:
- 1 bit per chunk (~16KB bitmap for 1GB file with 64KB chunks)
- O(1) mark and query operations
- Dynamic power-of-2 growth
- EOF handling and completion detection
- Gap/missing chunk detection

**Tests**: 10/10 passing

**API Highlights**:
```rust
let mut bitmap = ChunkBitmap::with_exact_size(total_chunks);
bitmap.mark_received(chunk_num, is_eof);
if bitmap.is_complete() { /* transfer done */ }
let missing = bitmap.find_missing();
```

### 2. ChunkTable (`table.rs`)
**Purpose**: Store detailed metadata for each chunk

**Key Features**:
- HashMap-based fast lookup (O(1) average)
- Metadata: chunk_number, byte_offset, chunk_length, checksum, end_of_file_flag
- Find missing chunks
- Verify sequence integrity
- JSON serialization for persistence
- Integration with ChunkBitmap

**Tests**: 16/16 passing

**API Highlights**:
```rust
let mut table = ChunkTable::with_capacity(1000);
table.insert(ChunkMetadata::new(chunk_num, offset, length, checksum, eof));
table.verify_integrity()?;
let missing = table.missing_chunks();
```

### 3. FileChunker (`chunker.rs`)
**Purpose**: Split files into fixed-size chunks with metadata

**Key Features**:
- Iterator-based chunk generation
- BLAKE3 checksums
- Progress tracking
- Seek to specific chunks
- Configurable chunk size
- ChunkPacket creation with metadata

**Tests**: 4/4 passing

**API Highlights**:
```rust
let mut chunker = FileChunker::new(path, Some(chunk_size))?;
while let Some(packet) = chunker.next_chunk()? {
    send_chunk(packet);
}
```

### 4. ChunkHasher (`hasher.rs`)
**Purpose**: Compute and verify BLAKE3 checksums

**Key Features**:
- BLAKE3 hashing (fast, cryptographically secure)
- Deterministic hash computation
- Verification helper

**Tests**: 3/3 passing

**API Highlights**:
```rust
let hash = ChunkHasher::hash(data);
if ChunkHasher::verify(data, &expected_hash) { /* valid */ }
```

## Test Coverage

**Total**: 33 tests passing

| Component     | Tests | Status |
|---------------|-------|--------|
| ChunkBitmap   | 10    | ✅ All passing |
| ChunkTable    | 16    | ✅ All passing |
| FileChunker   | 4     | ✅ All passing |
| ChunkHasher   | 3     | ✅ All passing |

## Integration Example

Complete file transfer with all components:

```rust
use sftpx::chunking::{FileChunker, ChunkTable, ChunkBitmap, ChunkHasher};

// Sender side
let mut chunker = FileChunker::new(file_path, Some(64 * 1024))?;
let total_chunks = chunker.total_chunks();

for chunk_packet in chunker.iter() {
    let packet = chunk_packet?;
    send_over_quic(packet);
}

// Receiver side
let mut table = ChunkTable::with_capacity(total_chunks as usize);
let mut bitmap = ChunkBitmap::with_exact_size(total_chunks as u32);

while let Some(chunk_packet) = receive_from_quic() {
    // Verify checksum
    if !ChunkHasher::verify(&chunk_packet.data, &chunk_packet.checksum) {
        continue; // Corrupt chunk
    }
    
    // Store metadata
    table.insert(ChunkMetadata::new(
        chunk_packet.chunk_number,
        chunk_packet.byte_offset,
        chunk_packet.length,
        chunk_packet.checksum,
        chunk_packet.end_of_file_flag,
    ));
    
    // Mark received
    bitmap.mark_received(chunk_packet.chunk_number as u32, chunk_packet.end_of_file_flag);
    
    // Check completion
    if bitmap.is_complete() && table.is_complete() {
        table.verify_integrity()?;
        break;
    }
}
```

## Performance Characteristics

### ChunkBitmap
- **Memory**: ~1 bit per chunk (128KB for 1M chunks)
- **Mark received**: O(1)
- **Check received**: O(1)
- **Find missing**: O(total_chunks)

### ChunkTable
- **Memory**: ~100 bytes per chunk
- **Insert**: O(1) average
- **Get**: O(1) average
- **Verify integrity**: O(n log n)

### FileChunker
- **Read chunk**: O(chunk_size)
- **Seek**: O(1)
- **Hash**: O(chunk_size) - BLAKE3 is very fast

### ChunkHasher
- **Hash computation**: ~1-2 GB/s (BLAKE3)
- **Verification**: Same as computation

## Usage Scenarios

### Scenario 1: Simple File Transfer
```rust
// Sender
let mut chunker = FileChunker::new(path, None)?; // Default chunk size
for chunk_result in chunker.iter() {
    send(chunk_result?)?;
}

// Receiver
let mut bitmap = ChunkBitmap::new(1024); // Initial capacity
while let Some(chunk) = receive()? {
    bitmap.mark_received(chunk.number, chunk.is_eof);
    write_to_file(chunk)?;
}
```

### Scenario 2: Resume Capability
```rust
// Save state periodically
let json = serde_json::to_string(&table)?;
std::fs::write("state.json", json)?;

// Resume later
let json = std::fs::read_to_string("state.json")?;
let table: ChunkTable = serde_json::from_str(&json)?;
let missing = table.missing_chunks();
request_retransmit(missing);
```

### Scenario 3: Parallel Chunk Reception
```rust
// Chunks can arrive out of order
let mut table = ChunkTable::new();
let mut bitmap = ChunkBitmap::new(0); // Dynamic growth

loop {
    match receive_any_chunk()? {
        Some(chunk) => {
            table.insert(chunk.metadata);
            bitmap.mark_received(chunk.number, chunk.is_eof);
            
            // Check if we can verify a sequence
            if table.len() % 100 == 0 {
                match table.verify_integrity() {
                    Ok(()) => println!("Valid sequence so far"),
                    Err(_) => println!("Still have gaps"),
                }
            }
        }
        None => break,
    }
}
```

### Scenario 4: Selective Retransmission
```rust
// After initial transfer
let missing = bitmap.find_missing();
if !missing.is_empty() {
    println!("Need to retransmit {} chunks", missing.len());
    for chunk_num in missing {
        request_chunk(chunk_num)?;
    }
}

// Verify completeness
assert!(bitmap.is_complete());
assert!(table.is_complete());
table.verify_integrity()?;
```

## Documentation

- [ChunkBitmap API](./chunk_bitmap.md) - Detailed bitmap documentation
- [ChunkTable API](./chunk_table.md) - Detailed table documentation
- `examples/bitmap_usage.rs` - Bitmap usage example
- `examples/chunk_table_usage.rs` - Table + bitmap integration

## Dependencies

External crates:
- `blake3` - Fast cryptographic hashing
- `serde` + `serde_json` - Serialization

Internal dependencies:
- `common::error` - Error types
- `common::types` - DEFAULT_CHUNK_SIZE
- `protocol::chunk` - ChunkPacketBuilder

## Design Principles

1. **Efficiency**: Bitmap uses bits, not bytes, for minimal memory
2. **Safety**: All chunk reception verified with checksums
3. **Flexibility**: Dynamic growth, configurable chunk sizes
4. **Robustness**: Integrity verification, gap detection
5. **Persistence**: JSON serialization for resume capability
6. **Integration**: Components work together seamlessly

## Future Enhancements

Potential improvements:
- [ ] Sparse chunk table (for very large files)
- [ ] Priority-based retransmission
- [ ] Chunk deduplication
- [ ] Compression-aware chunking
- [ ] Parallel checksum computation
- [ ] Adaptive chunk sizing

## Status: Complete ✅

All core functionality implemented and tested:
- ✅ ChunkBitmap (10 tests)
- ✅ ChunkTable (16 tests)
- ✅ FileChunker (4 tests)
- ✅ ChunkHasher (3 tests)
- ✅ Integration examples
- ✅ Documentation

**Total**: 33/33 tests passing
