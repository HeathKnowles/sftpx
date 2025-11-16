// Example: Client file upload using integrated orchestration with auto-resume
// Run with: cargo run --example client_upload -- <file_path> [server_ip]

use sftpx::client::transfer::Transfer;
use sftpx::common::config::ClientConfig;
use sftpx::chunking::compress::CompressionType;
use sftpx::chunking::ChunkBitmap;
use std::env;
use std::path::{Path, PathBuf};

fn get_session_id_for_file(file_path: &Path) -> String {
    // Generate deterministic session ID based on file path and name
    // This allows automatic resume for the same file
    let file_name = file_path.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown");
    
    // Use blake3 hash of absolute path for deterministic session ID
    let abs_path = std::fs::canonicalize(file_path)
        .unwrap_or_else(|_| file_path.to_path_buf());
    let path_str = abs_path.to_string_lossy();
    let hash = blake3::hash(path_str.as_bytes());
    
    format!("upload_{}_{}", file_name, hex::encode(&hash.as_bytes()[..8]))
}

fn check_for_resume(session_id: &str) -> Option<u32> {
    let resume_dir = PathBuf::from(".sftpx_resume");
    let bitmap_path = resume_dir.join(format!("{}.bitmap", session_id));
    
    if bitmap_path.exists() {
        if let Ok(bitmap) = ChunkBitmap::load_from_disk(&bitmap_path) {
            let received = bitmap.received_count();
            let total = bitmap.total_chunks().unwrap_or(0);
            if received > 0 {
                println!("\n📁 Found previous transfer:");
                println!("  Session ID: {}", session_id);
                println!("  Progress: {}/{} chunks ({:.1}%)", 
                    received, total, 
                    (received as f64 / total as f64) * 100.0);
                println!("  Will resume from chunk {}", received);
                return Some(received);
            }
        }
    }
    None
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    
    println!("=== SFTPX Client Upload Example ===\n");
    
    // Get file path and optional server IP from command line
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <file_path> [server_ip]", args[0]);
        eprintln!("\nExamples:");
        eprintln!("  Local:  cargo run --example client_upload -- ./test.txt");
        eprintln!("  Remote: cargo run --example client_upload -- ./test.txt 192.168.1.100");
        return Ok(());
    }
    
    let file_path = Path::new(&args[1]);
    let server_ip = if args.len() >= 3 {
        &args[2]
    } else {
        "127.0.0.1"
    };
    
    // Verify file exists
    if !file_path.exists() {
        eprintln!("Error: File not found: {:?}", file_path);
        return Ok(());
    }
    
    let file_size = std::fs::metadata(file_path)?.len();
    
    // Generate session ID for this file
    let session_id = get_session_id_for_file(file_path);
    
    println!("File to upload:");
    println!("  Path: {:?}", file_path);
    println!("  Size: {} bytes ({:.2} MB)", file_size, file_size as f64 / 1_048_576.0);
    println!("  Session ID: {}", session_id);
    
    // Check for existing transfer to resume
    let resume_from = check_for_resume(&session_id);
    
    // Create client configuration
    let server_addr = format!("{}:4443", server_ip).parse()?;
    let server_name = if server_ip == "127.0.0.1" || server_ip == "localhost" {
        "localhost".to_string()
    } else {
        server_ip.to_string() // Use IP address as server name for remote connections
    };
    
    let config = ClientConfig::new(server_addr, server_name)
        .disable_cert_verification()  // Skip cert verification for testing
        .with_chunk_size(2097152)?    // 2 MB chunks - balanced
        .with_compression(CompressionType::None);  // Disable compression for max speed
    
    println!("\nClient Configuration:");
    println!("  Server: {}", server_addr);
    println!("  Chunk Size: {} bytes ({} MB)", config.chunk_size, config.chunk_size / (1024*1024));
    println!("  Compression: {:?}", config.compression);
    println!("\nFeatures:");
    println!("  ✓ Integrated orchestration (handshake → manifest → chunks)");
    println!("  ✓ BLAKE3 integrity verification per chunk");
    println!("  ✓ Chunk-level deduplication (hash-based)");
    println!("  ✓ Auto-resume capability (saves every 100 chunks)");
    println!("  ✓ Protocol Buffers serialization");
    println!("  ✓ 4 QUIC streams (Control, Manifest, Data, Status)");
    
    if resume_from.is_some() {
        println!("\n🔄 RESUMING interrupted transfer...\n");
    } else {
        println!("\n▶️  Starting new upload...\n");
    }
    
    // Create transfer and run upload
    let mut transfer = Transfer::send_file(config, file_path.to_str().unwrap(), "server")?;
    
    match transfer.run_send(file_path) {
        Ok(bytes_sent) => {
            println!("\n✅ Upload successful!");
            println!("  Total bytes sent: {} ({:.2} MB)", bytes_sent, bytes_sent as f64 / 1_048_576.0);
            println!("  Transfer state: {:?}", transfer.state());
        }
        Err(e) => {
            eprintln!("\n❌ Upload failed: {:?}", e);
            return Err(e.into());
        }
    }
    
    Ok(())
}
