# Chunk Compression

The compression module provides adaptive compression for file transfer chunks using LZ4 and Zstd algorithms.

## Overview

Compression is integrated into the chunking pipeline:

```
Chunk → Compress → Hash → Table → Bitmap
```

## Compression Algorithms

### Algorithm Selection Strategy

The compressor automatically selects the best algorithm based on chunk size:

| Chunk Size | Algorithm | Reason |
|------------|-----------|--------|
| < 4 KB     | LZ4       | Speed prioritized for small chunks |
| 4-64 KB    | Zstd (level 3) | Balanced compression/speed |
| > 64 KB    | Zstd (level 5) | Better compression for large chunks |

### LZ4
- **Speed**: ~500 MB/s compression, ~2 GB/s decompression
- **Ratio**: ~2-3x compression on text
- **Use case**: Real-time transfers, small chunks

### Zstd
- **Speed**: ~100-400 MB/s (level dependent)
- **Ratio**: 3-5x compression on text (better than LZ4)
- **Levels**: 1-22 (higher = better compression, slower)
- **Use case**: Larger chunks where compression ratio matters

## Usage

### Basic Compression

```rust
use sftpx::chunking::{ChunkCompressor, CompressionAlgorithm};

// Automatic algorithm selection
let data = b"Your chunk data here";
let compressed = ChunkCompressor::compress_auto(data)?;

println!("Original: {} bytes", compressed.original_size);
println!("Compressed: {} bytes", compressed.compressed_size);
println!("Ratio: {:.1}%", compressed.ratio * 100.0);
println!("Algorithm: {:?}", compressed.algorithm);
```

### Manual Algorithm Selection

```rust
// Use LZ4
let compressed = ChunkCompressor::compress(data, CompressionAlgorithm::Lz4)?;

// Use Zstd with level 5
let compressed = ChunkCompressor::compress(data, CompressionAlgorithm::Zstd(5))?;

// No compression
let compressed = ChunkCompressor::compress(data, CompressionAlgorithm::None)?;
```

### Conditional Compression

Only compress if it saves at least 5% of space:

```rust
let compressed = ChunkCompressor::compress_if_beneficial(
    data,
    CompressionAlgorithm::Lz4,
    0.05, // 5% minimum reduction
)?;

if compressed.is_compressed() {
    println!("Saved {} bytes", compressed.space_saved());
} else {
    println!("Compression not beneficial, using original");
}
```

### Decompression

```rust
let decompressed = ChunkCompressor::decompress(
    &compressed.compressed_data,
    compressed.algorithm,
    Some(original_size), // Optional hint for allocation
)?;

assert_eq!(decompressed, original_data);
```

## Complete Pipeline Integration

### Sender Side

```rust
use sftpx::chunking::{
    FileChunker, ChunkCompressor, ChunkHasher, CompressionStats
};

let mut chunker = FileChunker::new(file_path, Some(chunk_size))?;
let mut stats = CompressionStats::new();

while let Some(chunk_data) = chunker.next_chunk()? {
    // 1. Compress
    let compressed = ChunkCompressor::compress_auto(&chunk_data)?;
    stats.add_chunk(&compressed);
    
    // 2. Hash the compressed data
    let hash = ChunkHasher::hash(compressed.data_to_send());
    
    // 3. Transmit
    send_chunk(compressed.data_to_send(), hash)?;
}

println!("Space saved: {} bytes ({:.1}%)",
    stats.space_saved(),
    (1.0 - stats.overall_ratio()) * 100.0
);
```

### Receiver Side

```rust
use sftpx::chunking::{
    ChunkCompressor, ChunkHasher, ChunkTable, ChunkBitmap
};

let mut table = ChunkTable::new();
let mut bitmap = ChunkBitmap::with_exact_size(total_chunks);

for chunk_packet in receive_chunks() {
    // 1. Verify hash (of compressed data)
    if !ChunkHasher::verify(&chunk_packet.data, &chunk_packet.hash) {
        continue; // Skip corrupted chunk
    }
    
    // 2. Decompress
    let decompressed = ChunkCompressor::decompress(
        &chunk_packet.data,
        chunk_packet.algorithm,
        Some(chunk_packet.original_size),
    )?;
    
    // 3. Store metadata
    let metadata = ChunkMetadata::new(
        chunk_packet.chunk_number,
        chunk_packet.byte_offset,
        decompressed.len() as u32,
        chunk_packet.hash,
        chunk_packet.is_eof,
    );
    table.insert(metadata);
    
    // 4. Track reception
    bitmap.mark_received(chunk_packet.chunk_number, chunk_packet.is_eof);
    
    // 5. Write to file
    write_chunk(&decompressed)?;
}
```

## CompressedChunk API

```rust
pub struct CompressedChunk {
    pub original_data: Vec<u8>,      // Original uncompressed data
    pub compressed_data: Vec<u8>,    // Compressed data
    pub algorithm: CompressionAlgorithm,
    pub original_size: usize,
    pub compressed_size: usize,
    pub ratio: f64,                  // compressed/original
}

impl CompressedChunk {
    // Check if compression helped
    pub fn is_compressed(&self) -> bool;
    
    // Bytes saved
    pub fn space_saved(&self) -> usize;
    
    // Get data for transmission (compressed or original)
    pub fn data_to_send(&self) -> &[u8];
}
```

## Compression Statistics

Track compression effectiveness across multiple chunks:

```rust
use sftpx::chunking::CompressionStats;

let mut stats = CompressionStats::new();

for compressed_chunk in chunks {
    stats.add_chunk(&compressed_chunk);
}

println!("Total chunks: {}", stats.total_chunks);
println!("Compressed: {} ({:.1}%)", 
    stats.compressed_chunks,
    stats.compression_percentage()
);
println!("Original: {} bytes", stats.original_bytes);
println!("After compression: {} bytes", stats.compressed_bytes);
println!("Saved: {} bytes", stats.space_saved());
println!("Overall ratio: {:.2}", stats.overall_ratio());

// Algorithm breakdown
println!("LZ4: {} chunks", stats.lz4_count);
println!("Zstd: {} chunks", stats.zstd_count);
println!("None: {} chunks", stats.none_count);
```

## Performance Characteristics

### LZ4
- **Compression**: ~500 MB/s
- **Decompression**: ~2000 MB/s
- **Ratio**: 2-3x on text, ~1.5x on mixed data
- **Memory**: Low (~1KB overhead)

### Zstd
| Level | Speed (MB/s) | Ratio | Use Case |
|-------|--------------|-------|----------|
| 1     | ~400         | 2.5x  | Fast, light compression |
| 3     | ~200         | 3x    | Default balanced |
| 5     | ~100         | 3.5x  | Better compression |
| 10    | ~40          | 4x    | High compression |
| 22    | ~5           | 5x+   | Maximum compression |

### Algorithm Overhead
- LZ4 header: ~10 bytes
- Zstd header: ~15 bytes
- No compression: 0 bytes

## Best Practices

### 1. Use Auto-Selection
```rust
// Let the library choose based on chunk size
let compressed = ChunkCompressor::compress_auto(data)?;
```

### 2. Always Hash Compressed Data
```rust
// Hash what you send, not the original
let hash = ChunkHasher::hash(compressed.data_to_send());
```

### 3. Store Algorithm with Metadata
```rust
// Receiver needs to know how to decompress
struct ChunkPacket {
    data: Vec<u8>,
    algorithm: CompressionAlgorithm,
    original_size: usize,
    hash: Vec<u8>,
}
```

### 4. Set Minimum Reduction Threshold
```rust
// Don't compress if it only saves <5%
let compressed = ChunkCompressor::compress_if_beneficial(
    data,
    algorithm,
    0.05,
)?;
```

### 5. Provide Size Hint for Decompression
```rust
// Helps allocate correct buffer size
let decompressed = ChunkCompressor::decompress(
    compressed_data,
    algorithm,
    Some(expected_size), // Include this!
)?;
```

## Example: Real-World Transfer

```rust
use sftpx::chunking::*;

fn transfer_file(path: &Path) -> Result<()> {
    // Setup
    let chunk_size = 64 * 1024; // 64KB chunks
    let mut chunker = FileChunker::new(path, Some(chunk_size))?;
    let mut stats = CompressionStats::new();
    
    // Process each chunk
    while let Some(chunk_data) = chunker.next_chunk()? {
        // Compress with auto-selection
        let compressed = ChunkCompressor::compress_auto(&chunk_data)?;
        stats.add_chunk(&compressed);
        
        // Only send if compression saved >3%
        let to_send = if compressed.ratio < 0.97 {
            compressed.data_to_send()
        } else {
            &chunk_data // Send original
        };
        
        // Hash and transmit
        let hash = ChunkHasher::hash(to_send);
        transmit(to_send, hash)?;
    }
    
    // Report results
    println!("Transfer complete!");
    println!("  Compressed: {}/{} chunks ({:.1}%)",
        stats.compressed_chunks,
        stats.total_chunks,
        stats.compression_percentage()
    );
    println!("  Data sent: {} bytes (saved {})",
        stats.compressed_bytes,
        stats.space_saved()
    );
    
    Ok(())
}
```

## Compression Efficiency by Data Type

| Data Type | LZ4 Ratio | Zstd(5) Ratio | Recommendation |
|-----------|-----------|---------------|----------------|
| Text      | 2-3x      | 3-5x          | Zstd for large files |
| JSON/XML  | 3-5x      | 5-8x          | Zstd always |
| Images    | 1.0-1.2x  | 1.0-1.2x      | Skip compression |
| Video     | 1.0x      | 1.0x          | Skip compression |
| Binaries  | 1.2-2x    | 1.5-3x        | LZ4 for speed |
| Logs      | 5-10x     | 8-15x         | Zstd high level |

## Error Handling

```rust
match ChunkCompressor::decompress(data, algorithm, Some(size)) {
    Ok(decompressed) => {
        // Success
    }
    Err(Error::Protocol(msg)) => {
        // Decompression failed - data corrupted
        println!("Corrupt chunk: {}", msg);
    }
    Err(Error::Io(e)) => {
        // I/O error
        println!("I/O error: {}", e);
    }
    Err(e) => {
        // Other error
        println!("Error: {}", e);
    }
}
```

## See Also

- [ChunkBitmap](./chunk_bitmap.md) - Tracking received chunks
- [ChunkTable](./chunk_table.md) - Storing chunk metadata
- [FileChunker](./file_chunker.md) - Creating chunks
- [Complete Pipeline Example](../examples/compression_pipeline.rs)
