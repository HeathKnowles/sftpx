// Complete example showing the full chunking pipeline:
// FileChunker → ChunkHasher → ChunkTable → ChunkBitmap

use sftpx::chunking::{FileChunker, ChunkHasher, ChunkTable, ChunkMetadata, ChunkBitmap};
use std::fs::File;
use std::io::Write;
use tempfile::tempdir;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Complete Chunking Pipeline Example ===\n");

    // Create a test file
    let temp_dir = tempdir()?;
    let file_path = temp_dir.path().join("test_file.bin");
    let mut file = File::create(&file_path)?;
    let test_data = vec![0xAB; 10240]; // 10KB file
    file.write_all(&test_data)?;
    file.sync_all()?;
    drop(file);

    let chunk_size = 1024; // 1KB chunks
    
    println!("📁 Created test file: {} bytes\n", test_data.len());

    // ============================================================
    // SENDER SIDE: Chunk → Hash → Send
    // ============================================================
    println!("=== SENDER: Creating and Hashing Chunks ===\n");

    let mut chunker = FileChunker::new(&file_path, Some(chunk_size))?;
    let total_chunks = chunker.total_chunks();
    
    println!("Total chunks to send: {}", total_chunks);
    println!("Chunk size: {} bytes\n", chunk_size);

    // Simulate sending chunks (we'll collect them for demo)
    let mut sent_chunks = Vec::new();
    let mut chunk_count = 0;

    while let Some(chunk_packet) = chunker.next_chunk()? {
        chunk_count += 1;
        
        // The FileChunker already computed the hash internally!
        // Extract metadata from the packet (in real code, this would be serialized)
        println!("Chunk {}: {} bytes, hash computed", 
            chunk_count - 1,
            chunk_packet.len()
        );
        
        sent_chunks.push(chunk_packet);
    }

    println!("\n✅ Sent {} chunks with hashes\n", sent_chunks.len());

    // ============================================================
    // RECEIVER SIDE: Receive → Verify Hash → Table → Bitmap
    // ============================================================
    println!("=== RECEIVER: Hash Verification → Table → Bitmap ===\n");

    let mut table = ChunkTable::with_capacity(total_chunks as usize);
    let mut bitmap = ChunkBitmap::with_exact_size(total_chunks as u32);
    
    table.set_file_info(test_data.len() as u64, total_chunks);

    // Simulate receiving chunks OUT OF ORDER
    let receive_order = vec![0, 2, 4, 1, 3, 5, 7, 9, 6, 8]; // Scrambled order
    
    println!("Receiving chunks in scrambled order: {:?}\n", receive_order);

    for &chunk_idx in &receive_order {
        if chunk_idx >= sent_chunks.len() {
            continue;
        }

        // Simulate receiving chunk data
        let chunk_packet = &sent_chunks[chunk_idx];
        
        // In a real implementation, we'd parse the packet to extract:
        // - chunk_number, byte_offset, length, checksum, data, eof_flag
        // For this demo, we'll create dummy metadata
        
        let chunk_number = chunk_idx as u64;
        let byte_offset = chunk_idx as u64 * chunk_size as u64;
        let chunk_length = chunk_size as u32;
        let is_eof = chunk_idx == (total_chunks - 1) as usize;
        
        // Extract data (first 1024 bytes of packet as dummy)
        let data_start = chunk_packet.len().saturating_sub(chunk_size);
        let chunk_data = &chunk_packet[data_start..];
        
        // STEP 1: HASH VERIFICATION
        let computed_hash = ChunkHasher::hash(chunk_data);
        
        // Simulate stored checksum (in real code, extracted from packet)
        let stored_checksum = computed_hash.clone();
        
        if ChunkHasher::verify(chunk_data, &stored_checksum) {
            println!("✓ Chunk {}: Hash verified", chunk_number);
            
            // STEP 2: SAVE TO TABLE
            let metadata = ChunkMetadata::new(
                chunk_number,
                byte_offset,
                chunk_length,
                stored_checksum,
                is_eof,
            );
            table.insert(metadata);
            
            // STEP 3: TRACK IN BITMAP
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
    println!("Total file size: {} bytes", test_data.len());
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

    // Show the complete pipeline summary
    println!("\n=== Pipeline Summary ===\n");
    println!("1. FileChunker: Split file into {} chunks", total_chunks);
    println!("2. ChunkHasher: Computed BLAKE3 hash for each chunk");
    println!("3. ChunkTable: Stored metadata for {} chunks", table.len());
    println!("4. ChunkBitmap: Tracked reception with 1 bit per chunk");
    println!("\nPipeline: File → Chunk → Hash → Verify → Table → Bitmap → Complete!");

    Ok(())
}
