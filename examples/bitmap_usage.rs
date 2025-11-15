// Example: Using ChunkBitmap for tracking received QUIC chunks

use sftpx::chunking::ChunkBitmap;

fn main() {
    println!("=== ChunkBitmap Usage Examples ===\n");
    
    // Example 1: Basic usage with known size
    println!("Example 1: Known file size");
    let bitmap = ChunkBitmap::with_exact_size(100);
    println!("Created bitmap for 100 chunks");
    println!("Memory usage: {} bytes\n", bitmap.memory_usage());
    
    // Example 2: Dynamic growth (size unknown)
    println!("Example 2: Dynamic growth");
    let mut bitmap = ChunkBitmap::new(0); // Start small
    
    // Simulate receiving chunks out of order
    bitmap.mark_received(0, false);
    bitmap.mark_received(5, false);
    bitmap.mark_received(10, false);
    
    println!("Received chunks: {}", bitmap.received_count());
    println!("Progress: {:.2}%", bitmap.progress());
    println!("Has EOF: {}\n", bitmap.has_eof());
    
    // Example 3: Real-world scenario with EOF
    println!("Example 3: Real-world transfer");
    let mut bitmap = ChunkBitmap::new(1024);
    
    // Simulate receiving 1GB file in 64KB chunks = ~16384 chunks
    // Receiving chunks out of order
    let chunks_to_receive = vec![
        (0, false),
        (1, false),
        (2, false),
        // Gap: chunk 3 missing
        (4, false),
        (5, false),
        // Gap: chunks 6-8 missing
        (9, true), // EOF - total is 10 chunks
    ];
    
    for (chunk_num, is_eof) in chunks_to_receive {
        let is_new = bitmap.mark_received(chunk_num, is_eof);
        println!("Chunk {}: {} (EOF: {})", 
            chunk_num, 
            if is_new { "NEW" } else { "DUPLICATE" },
            is_eof
        );
    }
    
    println!("\nTransfer Status:");
    println!("  Total chunks: {:?}", bitmap.total_chunks());
    println!("  Received: {}", bitmap.received_count());
    println!("  Progress: {:.2}%", bitmap.progress());
    println!("  Complete: {}", bitmap.is_complete());
    
    // Find missing chunks
    let missing = bitmap.find_missing();
    println!("\nMissing chunks: {:?}", missing);
    
    // Find gaps
    let gaps = bitmap.find_gaps();
    println!("Missing gaps: {:?}", gaps);
    
    // Simulate receiving missing chunks
    println!("\nReceiving missing chunks...");
    for chunk_num in missing {
        bitmap.mark_received(chunk_num, false);
        println!("  Received chunk {}", chunk_num);
    }
    
    println!("\nFinal Status:");
    println!("  Complete: {}", bitmap.is_complete());
    println!("  Progress: {:.2}%\n", bitmap.progress());
    
    // Example 4: Duplicate detection
    println!("Example 4: Duplicate detection");
    let mut bitmap = ChunkBitmap::new(10);
    
    let is_new = bitmap.mark_received(5, false);
    println!("First receive of chunk 5: {}", if is_new { "NEW" } else { "DUP" });
    
    let is_new = bitmap.mark_received(5, false);
    println!("Second receive of chunk 5: {}", if is_new { "NEW" } else { "DUP" });
    
    println!("\nReceived count: {} (duplicates not counted)", bitmap.received_count());
    
    // Example 5: Memory efficiency
    println!("\nExample 5: Memory efficiency");
    
    // 1 GB file with 64 KB chunks
    let file_size_gb = 1;
    let chunk_size_kb = 64;
    let file_size_kb = file_size_gb * 1024 * 1024; // Convert GB to KB
    let num_chunks = file_size_kb / chunk_size_kb;
    
    let bitmap = ChunkBitmap::with_exact_size(num_chunks);
    
    println!("File size: {} GB", file_size_gb);
    println!("Chunk size: {} KB", chunk_size_kb);
    println!("Total chunks: {}", num_chunks);
    println!("Bitmap memory: {} KB ({:.2}% of file size)", 
        bitmap.memory_usage() / 1024,
        (bitmap.memory_usage() as f64 / (file_size_gb * 1024 * 1024 * 1024) as f64) * 100.0
    );
    
    // Example 6: Selective retransmission
    println!("\nExample 6: Selective retransmission");
    let mut bitmap = ChunkBitmap::new(100);
    
    // Simulate partial receive - receive 2 out of every 3 chunks (66%)
    for i in 0..100 {
        // Receive chunks where i % 3 is 1 or 2, skip where i % 3 is 0
        // But always receive the last chunk (EOF)
        if i == 99 || i % 3 != 0 {
            let is_eof = i == 99; // Chunk 99 is EOF
            bitmap.mark_received(i, is_eof);
        }
    }
    
    println!("Received {}/100 chunks ({:.0}%)", bitmap.received_count(), bitmap.progress());
    
    // Request first 5 missing chunks
    let first_missing = bitmap.find_first_missing(5);
    println!("First 5 missing chunks: {:?}", first_missing);
    
    // Request missing in specific range
    let missing_in_range = bitmap.find_missing_in_range(20, 40);
    println!("Missing in range 20-40: {:?}", missing_in_range);
    
    println!("\n=== Examples Complete ===");
}
