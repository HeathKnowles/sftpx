// Complete pipeline example: Chunk → Compress → Hash → Store in Table → Track using Bitmap

use sftpx::chunking::{
    FileChunker, ChunkCompressor, ChunkHasher, ChunkTable, ChunkMetadata, 
    ChunkBitmap, CompressionAlgorithm, CompressionStats,
};
use std::fs::File;
use std::io::Write;
use tempfile::tempdir;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Complete Compression Pipeline ===\n");
    println!("Pipeline: Chunk → Compress → Hash → Table → Bitmap\n");

    // Create a test file with compressible data
    let temp_dir = tempdir()?;
    let file_path = temp_dir.path().join("test_file.txt");
    let mut file = File::create(&file_path)?;
    
    // Mix of text patterns (compressible) and varied data
    let mut test_data = Vec::new();
    test_data.extend_from_slice(b"Lorem ipsum dolor sit amet, ".repeat(100).as_slice());
    test_data.extend_from_slice(b"The quick brown fox jumps over the lazy dog. ".repeat(100).as_slice());
    test_data.extend_from_slice(&vec![0xFF; 2048]); // Highly compressible
    test_data.extend_from_slice(&(0..2048).map(|i| (i % 256) as u8).collect::<Vec<_>>()); // Less compressible
    
    file.write_all(&test_data)?;
    file.sync_all()?;
    drop(file);

    let file_size = test_data.len() as u64;
    let chunk_size = 2048;
    
    println!("📁 File: {} bytes", file_size);
    println!("📦 Chunk size: {} bytes\n", chunk_size);

    // ============================================================
    // SENDER SIDE: Complete Pipeline
    // ============================================================
    println!("=== SENDER: Processing Chunks ===\n");

    let mut chunker = FileChunker::new(&file_path, Some(chunk_size))?;
    let total_chunks = chunker.total_chunks();
    
    println!("Total chunks: {}\n", total_chunks);

    let mut compression_stats = CompressionStats::new();
    let mut chunk_packets = Vec::new();

    // Process each chunk through the pipeline
    while let Some(chunk_packet) = chunker.next_chunk()? {
        let chunk_num = chunk_packets.len() as u64;
        
        // Extract chunk data (in real code, parse from packet)
        let data_start = chunk_packet.len().saturating_sub(chunk_size.min(chunk_packet.len()));
        let chunk_data = &chunk_packet[data_start..];
        
        // STEP 1: COMPRESS
        let compressed = ChunkCompressor::compress_auto(chunk_data)?;
        compression_stats.add_chunk(&compressed);
        
        let compression_info = if compressed.is_compressed() {
            format!("{:?} ({:.1}% saved)", compressed.algorithm, (1.0 - compressed.ratio) * 100.0)
        } else {
            "None (not beneficial)".to_string()
        };
        
        // STEP 2: HASH (hash the compressed data for transmission)
        let hash = ChunkHasher::hash(compressed.data_to_send());
        
        println!("Chunk {}: {} bytes → {} bytes | {}",
            chunk_num,
            compressed.original_size,
            compressed.compressed_size,
            compression_info,
        );
        
        // Store for transmission
        chunk_packets.push((chunk_num, compressed, hash));
    }

    println!("\n--- Compression Summary ---");
    println!("Total chunks: {}", compression_stats.total_chunks);
    println!("Compressed chunks: {}", compression_stats.compressed_chunks);
    println!("Original size: {} bytes", compression_stats.original_bytes);
    println!("Compressed size: {} bytes", compression_stats.compressed_bytes);
    println!("Space saved: {} bytes ({:.1}% reduction)",
        compression_stats.space_saved(),
        (1.0 - compression_stats.overall_ratio()) * 100.0
    );
    println!("LZ4: {} | Zstd: {} | None: {}",
        compression_stats.lz4_count,
        compression_stats.zstd_count,
        compression_stats.none_count
    );

    // ============================================================
    // RECEIVER SIDE: Decompress → Verify → Table → Bitmap
    // ============================================================
    println!("\n=== RECEIVER: Processing Received Chunks ===\n");

    let mut table = ChunkTable::with_capacity(total_chunks as usize);
    let mut bitmap = ChunkBitmap::with_exact_size(total_chunks as u32);
    
    table.set_file_info(file_size, total_chunks);

    // Receive in scrambled order
    let mut receive_order: Vec<usize> = (0..chunk_packets.len()).collect();
    receive_order.reverse(); // Reverse order for demo
    
    println!("Receiving in reverse order...\n");

    for &idx in &receive_order {
        let (chunk_num, compressed_chunk, stored_hash) = &chunk_packets[idx];
        
        // STEP 1: HASH VERIFICATION (verify compressed data)
        let data_to_verify = compressed_chunk.data_to_send();
        
        if !ChunkHasher::verify(data_to_verify, stored_hash) {
            println!("✗ Chunk {}: Hash verification failed! Skipping.", chunk_num);
            continue;
        }
        
        // STEP 2: DECOMPRESS (if needed)
        let decompressed_data = if compressed_chunk.is_compressed() {
            ChunkCompressor::decompress(
                &compressed_chunk.compressed_data,
                compressed_chunk.algorithm,
                Some(compressed_chunk.original_size),
            )?
        } else {
            compressed_chunk.original_data.clone()
        };
        
        // Verify decompression produced correct size
        if decompressed_data.len() != compressed_chunk.original_size {
            println!("✗ Chunk {}: Decompression size mismatch!", chunk_num);
            continue;
        }
        
        println!("✓ Chunk {}: Hash OK | Decompressed {} → {} bytes",
            chunk_num,
            compressed_chunk.compressed_size,
            decompressed_data.len()
        );
        
        // STEP 3: STORE IN TABLE
        let byte_offset = chunk_num * chunk_size as u64;
        let chunk_length = decompressed_data.len() as u32;
        let is_eof = *chunk_num == total_chunks - 1;
        
        let metadata = ChunkMetadata::new(
            *chunk_num,
            byte_offset,
            chunk_length,
            stored_hash.clone(),
            is_eof,
        );
        table.insert(metadata);
        
        // STEP 4: TRACK IN BITMAP
        bitmap.mark_received(*chunk_num as u32, is_eof);
        
        // Could write decompressed_data to output file here
    }

    // ============================================================
    // VERIFICATION
    // ============================================================
    println!("\n=== Transfer Verification ===\n");

    println!("Chunks stored: {}/{}", table.len(), total_chunks);
    println!("Bitmap progress: {:.1}%", bitmap.progress());
    println!("Table complete: {}", table.is_complete());
    println!("Bitmap complete: {}", bitmap.is_complete());

    if table.is_complete() && bitmap.is_complete() {
        match table.verify_integrity() {
            Ok(()) => {
                println!("\n✅ SUCCESS!");
                println!("   ✓ All chunks received");
                println!("   ✓ Hash verification passed");
                println!("   ✓ Decompression successful");
                println!("   ✓ Sequence integrity verified");
                println!("   ✓ Table and bitmap synchronized");
            }
            Err(e) => {
                println!("\n✗ Integrity check failed: {}", e);
            }
        }
    } else {
        let missing = table.missing_chunks();
        println!("\n⚠ Missing chunks: {:?}", missing);
    }

    // ============================================================
    // PIPELINE SUMMARY
    // ============================================================
    println!("\n=== Complete Pipeline Summary ===\n");
    println!("┌─────────────────────────────────────────┐");
    println!("│  SENDER                                 │");
    println!("├─────────────────────────────────────────┤");
    println!("│  1. FileChunker   → Split into chunks  │");
    println!("│  2. Compress      → Reduce size        │");
    println!("│  3. Hash          → Compute checksum   │");
    println!("│  4. Transmit      → Send over network  │");
    println!("└─────────────────────────────────────────┘");
    println!("");
    println!("┌─────────────────────────────────────────┐");
    println!("│  RECEIVER                               │");
    println!("├─────────────────────────────────────────┤");
    println!("│  1. Receive       → Get chunk packet   │");
    println!("│  2. Hash verify   → Check integrity    │");
    println!("│  3. Decompress    → Restore original   │");
    println!("│  4. ChunkTable    → Store metadata     │");
    println!("│  5. ChunkBitmap   → Track reception    │");
    println!("└─────────────────────────────────────────┘");
    
    println!("\n📊 Efficiency Metrics:");
    println!("   Compression: {:.1}% of chunks benefited", 
        compression_stats.compression_percentage());
    println!("   Size reduction: {:.1}%", 
        (1.0 - compression_stats.overall_ratio()) * 100.0);
    println!("   Bytes saved: {}", compression_stats.space_saved());
    println!("   Bitmap memory: ~{} bytes", (total_chunks + 7) / 8);
    
    Ok(())
}
