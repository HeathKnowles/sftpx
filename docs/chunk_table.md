# Chunk Table and Metadata

The `ChunkTable` provides storage and management of chunk metadata for file transfers in the sftpx system.

## Overview

The chunk table works in conjunction with the `ChunkBitmap` to provide complete chunk tracking:
- **ChunkBitmap**: Efficiently tracks which chunks have been received (1 bit per chunk)
- **ChunkTable**: Stores detailed metadata for each chunk

## ChunkMetadata Structure

Each chunk has the following metadata:

```rust
pub struct ChunkMetadata {
    pub chunk_number: u64,        // Chunk sequence number (0-based)
    pub byte_offset: u64,         // Starting byte position in file
    pub chunk_length: u32,        // Length of this chunk in bytes
    pub checksum: Vec<u8>,        // BLAKE3 hash of chunk data
    pub end_of_file_flag: bool,   // True if this is the last chunk
}
```

## Usage

### Creating a Chunk Table

```rust
use sftpx::chunking::{ChunkTable, ChunkMetadata};

// Create empty table
let mut table = ChunkTable::new();

// Or with pre-allocated capacity
let mut table = ChunkTable::with_capacity(1000);

// Set file information
table.set_file_info(file_size, total_chunks);
```

### Storing Chunk Metadata

```rust
// Create metadata for a chunk
let metadata = ChunkMetadata::new(
    chunk_number,     // e.g., 42
    byte_offset,      // e.g., 42 * 1024 = 43008
    chunk_length,     // e.g., 1024
    checksum,         // BLAKE3 hash bytes
    end_of_file_flag, // false for most chunks, true for last
);

// Store in table
table.insert(metadata);
```

### Querying Chunk Information

```rust
// Get metadata for a specific chunk
if let Some(metadata) = table.get(chunk_number) {
    println!("Chunk {} starts at byte {}", 
        metadata.chunk_number, 
        metadata.byte_offset
    );
}

// Check if chunk metadata exists
if table.contains(42) {
    println!("Have metadata for chunk 42");
}

// Get all stored chunk numbers (sorted)
let chunks = table.chunk_numbers(); // e.g., [0, 1, 2, 5, 7]

// Find missing chunks
let missing = table.missing_chunks(); // e.g., [3, 4, 6, 8, 9]
```

### Tracking Transfer Progress

```rust
// Check completion status
if table.is_complete() {
    println!("All chunk metadata received!");
}

// Get progress information
println!("Stored: {}/{} chunks", table.len(), table.total_chunks());
println!("Bytes stored: {}/{}", table.bytes_stored(), table.total_size());

// Find the EOF chunk
if let Some(last) = table.last_chunk() {
    println!("Last chunk is #{}", last.chunk_number);
}
```

### Verifying Integrity

The chunk table can verify that stored chunks form a valid sequence:

```rust
match table.verify_integrity() {
    Ok(()) => println!("Chunk sequence is valid"),
    Err(e) => println!("Integrity problem: {}", e),
}
```

Integrity checks verify:
- Chunks are numbered sequentially starting from 0
- No gaps in chunk numbers
- Byte offsets are contiguous (no overlaps or gaps)
- EOF flag only appears on the last chunk

### Integration with ChunkBitmap

The table and bitmap should be kept synchronized:

```rust
use sftpx::chunking::{ChunkTable, ChunkMetadata, ChunkBitmap};

let mut table = ChunkTable::new();
let mut bitmap = ChunkBitmap::with_exact_size(total_chunks);

// When receiving a chunk:
let metadata = ChunkMetadata::new(chunk_num, offset, length, checksum, is_eof);
table.insert(metadata);
bitmap.mark_received(chunk_num as u32, is_eof);

// Verify they agree on missing chunks
let table_missing = table.missing_chunks();
let bitmap_missing = bitmap.find_missing();
assert_eq!(table_missing.len(), bitmap_missing.len());
```

### Serialization for Persistence

The chunk table supports serialization to save transfer state:

```rust
use std::fs;

// Save to disk
let json = serde_json::to_string_pretty(&table)?;
fs::write("transfer_state.json", json)?;

// Load from disk
let json = fs::read_to_string("transfer_state.json")?;
let table: ChunkTable = serde_json::from_str(&json)?;
```

This enables:
- Resume capability after interruption
- Transfer state inspection
- Debugging and diagnostics

## API Reference

### ChunkTable Methods

#### Creation
- `new()` - Create empty table
- `with_capacity(capacity)` - Pre-allocate for expected chunks
- `set_file_info(total_size, total_chunks)` - Set file parameters

#### Insertion & Removal
- `insert(metadata)` - Add/update chunk metadata
- `remove(chunk_number)` - Remove chunk metadata
- `clear()` - Remove all metadata

#### Queries
- `get(chunk_number)` - Get metadata reference
- `contains(chunk_number)` - Check if chunk exists
- `len()` - Number of stored chunks
- `is_empty()` - Check if table is empty

#### Analysis
- `chunk_numbers()` - Get sorted list of chunk numbers
- `missing_chunks()` - Find gaps in sequence
- `bytes_stored()` - Total bytes covered
- `last_chunk()` - Get EOF chunk metadata
- `is_complete()` - Check if all chunks present
- `verify_integrity()` - Validate chunk sequence

#### File Information
- `total_size()` - Get file size in bytes
- `total_chunks()` - Get expected chunk count

#### Iteration
- `iter_sorted()` - Iterate over metadata sorted by chunk number

## Performance Characteristics

- **Memory**: O(n) where n = number of chunks stored
  - ~100 bytes per chunk for metadata
  - HashMap overhead for fast lookup
- **Insert**: O(1) average
- **Get**: O(1) average
- **Missing chunks**: O(total_chunks) - must check entire range
- **Verify integrity**: O(n log n) - requires sorting

## Example: Complete Transfer Flow

```rust
use sftpx::chunking::{ChunkTable, ChunkMetadata, ChunkBitmap};

// Setup
let file_size = 1024 * 1024; // 1MB
let chunk_size = 1024;       // 1KB chunks
let total_chunks = (file_size + chunk_size - 1) / chunk_size;

let mut table = ChunkTable::with_capacity(total_chunks as usize);
let mut bitmap = ChunkBitmap::with_exact_size(total_chunks as u32);

table.set_file_info(file_size, total_chunks);

// Receive chunks
for chunk_packet in receive_chunks() {
    // Verify checksum
    if !verify_checksum(&chunk_packet.data, &chunk_packet.checksum) {
        continue; // Skip corrupt chunk
    }
    
    // Store metadata
    let metadata = ChunkMetadata::new(
        chunk_packet.chunk_number,
        chunk_packet.byte_offset,
        chunk_packet.length,
        chunk_packet.checksum,
        chunk_packet.end_of_file_flag,
    );
    table.insert(metadata);
    bitmap.mark_received(chunk_packet.chunk_number as u32, chunk_packet.end_of_file_flag);
    
    // Check progress
    if table.len() % 100 == 0 {
        println!("Progress: {:.1}% ({} chunks)", 
            bitmap.progress(),
            table.len()
        );
    }
}

// Verify completion
if table.is_complete() && bitmap.is_complete() {
    table.verify_integrity()?;
    println!("Transfer complete and verified!");
} else {
    let missing = table.missing_chunks();
    println!("Missing chunks: {:?}", missing);
    // Request retransmission...
}
```

## See Also

- [ChunkBitmap Documentation](./chunk_bitmap.md) - Efficient chunk tracking
- [FileChunker Documentation](./file_chunker.md) - Creating chunks from files
- [ChunkHasher Documentation](./chunk_hasher.md) - Computing checksums
