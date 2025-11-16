// Example demonstrating the chunk protocol

use sftpx::chunking::FileChunker;
use sftpx::protocol::chunk::ChunkPacketParser;
use sftpx::client::FileReceiver;
use std::path::Path;
use tempfile::TempDir;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== SFTPX Chunk Protocol Test ===\n");

    // Input file
    let input_path = Path::new("/tmp/test_input.dat");
    if !input_path.exists() {
        eprintln!("Error: Test file not found. Create it with:");
        eprintln!("  dd if=/dev/zero of=/tmp/test_input.dat bs=1024 count=100");
        std::process::exit(1);
    }

    // Output directory
    let output_dir = TempDir::new()?;
    let output_filename = "test_output.dat";

    println!("Input file: {}", input_path.display());
    println!("Output dir: {}", output_dir.path().display());
    println!();

    // Step 1: Create a file chunker
    let chunk_size = 8192; // 8KB chunks
    let mut chunker = FileChunker::new(input_path, Some(chunk_size))?;
    
    let file_size = chunker.file_size();
    let total_chunks = chunker.total_chunks();
    
    println!("File size: {} bytes", file_size);
    println!("Chunk size: {} bytes", chunk_size);
    println!("Total chunks: {}", total_chunks);
    println!();

    // Step 2: Create a file receiver
    let mut receiver = FileReceiver::new(output_dir.path(), output_filename, file_size)?;

    // Step 3: Simulate transfer by reading chunks and writing them
    println!("Starting transfer...");
    let mut chunks_transferred = 0;

    while let Some(chunk_packet) = chunker.next_chunk()? {
        // Parse the chunk (simulating network receive)
        let chunk_view = ChunkPacketParser::parse(&chunk_packet)?;
        
        println!(
            "Chunk {}: offset={}, length={}, eof={}",
            chunk_view.chunk_id,
            chunk_view.byte_offset,
            chunk_view.chunk_length,
            chunk_view.end_of_file
        );

        // Receive the chunk
        receiver.receive_chunk(&chunk_packet)?;
        chunks_transferred += 1;

        // Show progress
        if chunks_transferred % 5 == 0 || chunk_view.end_of_file {
            println!("Progress: {:.1}%", receiver.progress() * 100.0);
        }
    }

    println!();
    println!("Transfer complete!");
    println!("Chunks transferred: {}", chunks_transferred);
    println!();

    // Step 4: Check if transfer is complete
    if receiver.is_complete() {
        println!("✓ All chunks received");
        
        // Finalize the transfer
        let final_path = receiver.finalize()?;
        println!("✓ File saved to: {}", final_path.display());
        
        // Verify file size
        let output_size = std::fs::metadata(&final_path)?.len();
        println!("✓ Output file size: {} bytes", output_size);
        
        if output_size == file_size {
            println!("✓ File size matches!");
        } else {
            eprintln!("✗ File size mismatch! Expected {}, got {}", file_size, output_size);
        }
    } else {
        let missing = receiver.missing_chunks();
        eprintln!("✗ Transfer incomplete. Missing {} chunks: {:?}", missing.len(), missing);
        return Err("Transfer incomplete".into());
    }

    // Show statistics
    let stats = receiver.stats();
    println!();
    println!("=== Statistics ===");
    println!("Bytes received: {}", stats.bytes_received);
    println!("Chunks received: {}", stats.chunks_received);
    println!("Total chunks: {}", stats.total_chunks);
    println!("Complete: {}", stats.is_complete);
    println!("Progress: {:.1}%", stats.progress * 100.0);

    println!();
    println!("=== Test PASSED ===");
    
    Ok(())
}
