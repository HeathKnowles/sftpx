// Complete example showing the full chunking pipeline:
// FileChunker → ChunkCompressor → ChunkHasher → ChunkTable → ChunkBitmap

use sftpx::chunking::{
    FileChunker, ChunkCompressor, ChunkHasher, ChunkTable, 
    ChunkMetadata, ChunkBitmap, CompressionStats, CompressionAlgorithm,
};
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use tempfile::tempdir;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Complete Chunking Pipeline Example ===\n");

    // Use custom file if provided as argument, otherwise create test file
    let (file_path, _temp_dir) = if let Some(custom_path) = std::env::args().nth(1) {
        println!("Using custom file: {}\n", custom_path);
        (PathBuf::from(custom_path), None)
    } else {
        println!("No file specified, creating test file...\n");
        let temp_dir = tempdir()?;
        let file_path = temp_dir.path().join("test_file.bin");
        let mut file = File::create(&file_path)?;
        let test_data = vec![0xAB; 10240]; // 10KB file
        file.write_all(&test_data)?;
        file.sync_all()?;
        drop(file);
        (file_path, Some(temp_dir))
    };

    // Get file size for display
    let file_size = std::fs::metadata(&file_path)?.len();
    
    // Determine compression algorithm based on file extension
    let extension = file_path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");
    let compression_algorithm = CompressionAlgorithm::auto_select_by_extension(extension);
    
    println!("📁 File: {} bytes", file_size);
    println!("📝 Extension: .{}", extension);
    println!("🗜️  Compression: {:?}\n", compression_algorithm);

    let chunk_size = 8192; // 8KB chunks to demonstrate different compression algorithms
    // ============================================================
    // SENDER SIDE: Chunk → Compress → Hash → Send
    // ============================================================
    println!("=== SENDER: Creating, Compressing and Hashing Chunks ===\n");

    let mut chunker = FileChunker::new(&file_path, Some(chunk_size))?;
    let mut compression_stats = CompressionStats::new();
    let total_chunks = chunker.total_chunks();
    
    println!("Total chunks to send: {}", total_chunks);
    println!("Chunk size: {} bytes\n", chunk_size);

    // Simulate sending chunks (we'll collect them for demo)
    let mut sent_chunks = Vec::new();
    let mut chunk_count = 0;

    while let Some(chunk_packet) = chunker.next_chunk()? {
        chunk_count += 1;
        
        // Extract the chunk data (skip header/metadata)
        let data_start = chunk_packet.len().saturating_sub(chunk_size.min(chunk_packet.len()));
        let chunk_data = &chunk_packet[data_start..];
        
        // STEP 1: COMPRESS the chunk using file-type-specific algorithm
        let compressed_chunk = ChunkCompressor::compress(chunk_data, compression_algorithm)?;
        compression_stats.add_chunk(&compressed_chunk);
        
        // STEP 2: HASH the compressed data
        let hash = ChunkHasher::hash(&compressed_chunk.compressed_data);
        
        println!("Chunk {}: {} bytes → {} bytes ({:?}), hash computed", 
            chunk_count - 1,
            compressed_chunk.original_size,
            compressed_chunk.compressed_size,
            compressed_chunk.algorithm
        );
        
        // Store compressed data + metadata for transmission
        sent_chunks.push((compressed_chunk.compressed_data, hash, compressed_chunk.algorithm));
    }

    println!("\n✅ Sent {} chunks with compression and hashes", sent_chunks.len());
    let savings_percent = if compression_stats.original_bytes > 0 {
        (compression_stats.space_saved() as f64 / compression_stats.original_bytes as f64) * 100.0
    } else { 0.0 };
    println!("   Compression: {:.1}% saved\n", savings_percent);

    // ============================================================
    // RECEIVER SIDE: Receive → Verify Hash → Decompress → Table → Bitmap
    // ============================================================
    println!("=== RECEIVER: Hash Verification → Decompress → Table → Bitmap ===\n");

    let mut table = ChunkTable::with_capacity(total_chunks as usize);
    let mut bitmap = ChunkBitmap::with_exact_size(total_chunks as u32);
    
    table.set_file_info(file_size, total_chunks);

    // Simulate receiving chunks OUT OF ORDER
    let receive_order = vec![0, 2, 4, 1, 3, 5, 7, 9, 6, 8]; // Scrambled order
    
    println!("Receiving chunks in scrambled order: {:?}\n", receive_order);

    for &chunk_idx in &receive_order {
        if chunk_idx >= sent_chunks.len() {
            continue;
        }

        // Simulate receiving chunk data
        let (compressed_data, stored_hash, algorithm) = &sent_chunks[chunk_idx];
        
        // In a real implementation, we'd parse the packet to extract:
        // - chunk_number, byte_offset, length, checksum, data, eof_flag, compression_algorithm
        // For this demo, we'll create metadata
        
        let chunk_number = chunk_idx as u64;
        let byte_offset = chunk_idx as u64 * chunk_size as u64;
        let is_eof = chunk_idx == (total_chunks - 1) as usize;
        
        // STEP 1: HASH VERIFICATION (on compressed data)
        if ChunkHasher::verify(compressed_data, stored_hash) {
            println!("✓ Chunk {}: Hash verified", chunk_number);
            
            // STEP 2: DECOMPRESS
            let decompressed_data = ChunkCompressor::decompress(compressed_data, *algorithm, Some(chunk_size))?;
            let chunk_length = decompressed_data.len() as u32;
            
            println!("  → Decompressed: {} bytes → {} bytes ({:?})", 
                compressed_data.len(), decompressed_data.len(), algorithm);
            
            // STEP 3: SAVE TO TABLE
            let metadata = ChunkMetadata::new(
                chunk_number,
                byte_offset,
                chunk_length,
                stored_hash.clone(),
                is_eof,
            );
            table.insert(metadata);
            
            // STEP 4: TRACK IN BITMAP
            bitmap.mark_received(chunk_number as u32, is_eof);
            
            // Show progress
            println!("  → Saved to table (offset: {}, length: {})", byte_offset, chunk_length);
            println!("  → Marked in bitmap");
            println!("  → Progress: {:.1}%", bitmap.progress());
        } else {
            println!("✗ Chunk {}: Hash mismatch! Skipping.", chunk_number);
        }
    }

    // ============================================================
    // VERIFICATION AND STATUS
    // ============================================================
    println!("\n=== Transfer Status ===\n");

    println!("Chunks in table: {}", table.chunk_numbers()
        .iter()
        .map(|n| n.to_string())
        .collect::<Vec<_>>()
        .join(", "));
    
    println!("Table complete: {}", table.is_complete());
    println!("Bitmap complete: {}", bitmap.is_complete());
    println!("Progress: {:.1}%", bitmap.progress());

    // Check for missing chunks
    let missing_chunks = table.missing_chunks();
    if !missing_chunks.is_empty() {
        println!("\n⚠ Missing chunks: {:?}", missing_chunks);
        println!("→ Need to request retransmission");
    } else {
        println!("\n✅ All chunks received!");
        
        // Verify integrity
        match table.verify_integrity() {
            Ok(()) => {
                println!("✅ Chunk sequence integrity verified");
                println!("   - No gaps in sequence");
                println!("   - Correct byte offsets");
                println!("   - Proper EOF flag");
            }
            Err(e) => {
                println!("✗ Integrity check failed: {}", e);
            }
        }
    }

    // Show statistics
    println!("\n=== Statistics ===\n");
    println!("Total file size: {} bytes", file_size);
    println!("Bytes stored: {} bytes", table.bytes_stored());
    println!("Chunks received: {}/{}", table.len(), total_chunks);
    println!("Bitmap memory: ~{} bytes", (total_chunks + 7) / 8);
    println!("Table memory: ~{} bytes", table.len() * 100); // Approximate

    // Demonstrate synchronization between table and bitmap
    println!("\n=== Table ↔ Bitmap Synchronization ===\n");
    let table_missing = table.missing_chunks();
    let bitmap_missing = bitmap.find_missing();
    let table_missing_u32: Vec<u32> = table_missing.iter().map(|&n| n as u32).collect();
    
    if table_missing_u32 == bitmap_missing {
        println!("✅ Table and Bitmap are synchronized");
        println!("   Both report same missing chunks: {:?}", table_missing);
    } else {
        println!("⚠ Mismatch detected!");
        println!("   Table missing: {:?}", table_missing);
        println!("   Bitmap missing: {:?}", bitmap_missing);
    }

    // Show compression statistics
    println!("\n=== Compression Statistics ===\n");
    println!("Total chunks: {}", compression_stats.total_chunks);
    println!("Original size: {} bytes", compression_stats.original_bytes);
    println!("Compressed size: {} bytes", compression_stats.compressed_bytes);
    let final_savings_percent = if compression_stats.original_bytes > 0 {
        (compression_stats.space_saved() as f64 / compression_stats.original_bytes as f64) * 100.0
    } else { 0.0 };
    println!("Savings: {:.1}% ({} bytes saved)", 
        final_savings_percent,
        compression_stats.space_saved());
    println!("Algorithm usage:");
    if compression_stats.none_count > 0 {
        println!("  - None: {} chunks", compression_stats.none_count);
    }
    if compression_stats.lz4_count > 0 {
        println!("  - LZ4: {} chunks", compression_stats.lz4_count);
    }
    if compression_stats.lz4hc_count > 0 {
        println!("  - LZ4HC: {} chunks", compression_stats.lz4hc_count);
    }
    if compression_stats.zstd_count > 0 {
        println!("  - Zstd: {} chunks", compression_stats.zstd_count);
    }
    if compression_stats.lzma2_count > 0 {
        println!("  - LZMA2: {} chunks", compression_stats.lzma2_count);
    }

    // Show the complete pipeline summary
    println!("\n=== Pipeline Summary ===\n");
    println!("1. FileChunker: Split file into {} chunks", total_chunks);
    println!("2. ChunkCompressor: Compressed chunks ({:.1}% savings)", final_savings_percent);
    println!("3. ChunkHasher: Computed BLAKE3 hash for each compressed chunk");
    println!("4. ChunkTable: Stored metadata for {} chunks", table.len());
    println!("5. ChunkBitmap: Tracked reception with 1 bit per chunk");
    println!("\nPipeline: File → Chunk → Compress → Hash → Verify → Decompress → Table → Bitmap → Complete!");

    Ok(())
}
