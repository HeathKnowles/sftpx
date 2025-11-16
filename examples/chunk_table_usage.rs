// Example demonstrating ChunkTable usage for tracking chunk metadata
// and integration with ChunkBitmap

use sftpx::chunking::{ChunkTable, ChunkMetadata, ChunkBitmap};

fn main() {
    println!("=== Chunk Table and Metadata Example ===\n");

    // Scenario: Receiving a 10KB file split into 1KB chunks (10 chunks total)
    let file_size = 10240u64;
    let chunk_size = 1024u32;
    let total_chunks = 10u32;

    // Create chunk table and bitmap
    let mut table = ChunkTable::with_capacity(total_chunks as usize);
    let mut bitmap = ChunkBitmap::with_exact_size(total_chunks);

    // Set file info
    table.set_file_info(file_size, total_chunks as u64);
    println!("File size: {} bytes", file_size);
    println!("Chunk size: {} bytes", chunk_size);
    println!("Total chunks: {}\n", total_chunks);

    // Simulate receiving chunks out of order
    println!("--- Receiving Chunks ---");
    
    let received_chunks = vec![
        (0, 0, 1024, false),      // chunk 0
        (2, 2048, 1024, false),   // chunk 2 (gap!)
        (1, 1024, 1024, false),   // chunk 1 (fills gap)
        (5, 5120, 1024, false),   // chunk 5
        (3, 3072, 1024, false),   // chunk 3
    ];

    for (chunk_num, offset, length, is_eof) in received_chunks {
        // Create metadata
        let checksum = vec![0xAB; 32]; // dummy checksum
        let metadata = ChunkMetadata::new(chunk_num as u64, offset, length, checksum, is_eof);
        
        // Store in table
        table.insert(metadata.clone());
        
        // Mark in bitmap
        bitmap.mark_received(chunk_num, is_eof);
        
        println!("Received chunk {}: offset={}, length={}", chunk_num, offset, length);
        println!("  Chunks stored: {}/{}", table.len(), total_chunks);
        println!("  Progress: {:.1}%", (table.bytes_stored() as f64 / file_size as f64) * 100.0);
    }

    println!("\n--- Current State ---");
    println!("Chunks in table: {}", table.chunk_numbers().iter().map(|n| n.to_string()).collect::<Vec<_>>().join(", "));
    println!("Missing chunks: {}", table.missing_chunks().iter().map(|n| n.to_string()).collect::<Vec<_>>().join(", "));
    println!("Bytes stored: {}/{} ({:.1}%)", 
        table.bytes_stored(), 
        file_size, 
        (table.bytes_stored() as f64 / file_size as f64) * 100.0
    );
    println!("Is complete: {}", table.is_complete());

    // Verify bitmap matches table
    println!("\n--- Bitmap Verification ---");
    let bitmap_missing = bitmap.find_missing();
    let table_missing = table.missing_chunks();
    println!("Bitmap missing: {:?}", bitmap_missing);
    println!("Table missing: {:?}", table_missing);
    
    // Compare (convert u64 to u32 for comparison)
    let table_missing_u32: Vec<u32> = table_missing.iter().map(|&n| n as u32).collect();
    assert_eq!(bitmap_missing, table_missing_u32, "Bitmap and table should agree on missing chunks");
    println!("✓ Bitmap and table are synchronized");

    // Complete the transfer
    println!("\n--- Completing Transfer ---");
    let remaining_chunks = vec![
        (4, 4096, 1024, false),
        (6, 6144, 1024, false),
        (7, 7168, 1024, false),
        (8, 8192, 1024, false),
        (9, 9216, 1024, true),  // last chunk with EOF flag
    ];

    for (chunk_num, offset, length, is_eof) in remaining_chunks {
        let checksum = vec![0xCD; 32];
        let metadata = ChunkMetadata::new(chunk_num as u64, offset, length, checksum, is_eof);
        table.insert(metadata);
        bitmap.mark_received(chunk_num, is_eof);
    }

    println!("All chunks received!");
    println!("  Table complete: {}", table.is_complete());
    println!("  Bitmap complete: {}", bitmap.is_complete());
    println!("  Total bytes: {}", table.bytes_stored());

    // Verify integrity
    println!("\n--- Integrity Verification ---");
    match table.verify_integrity() {
        Ok(()) => println!("✓ Chunk sequence is valid (no gaps, correct offsets)"),
        Err(e) => println!("✗ Integrity check failed: {}", e),
    }

    // Find the last chunk
    if let Some(last) = table.last_chunk() {
        println!("Last chunk (EOF): chunk #{}, offset={}, length={}", 
            last.chunk_number, last.byte_offset, last.chunk_length);
    }

    // Demonstrate serialization
    println!("\n--- Serialization ---");
    let json = serde_json::to_string_pretty(&table).unwrap();
    println!("Table serialized to JSON ({} bytes)", json.len());
    
    // Could save to disk for resume capability
    // std::fs::write("transfer_state.json", &json).unwrap();
    println!("✓ Table can be serialized for persistence");

    println!("\n=== Example Complete ===");
}
