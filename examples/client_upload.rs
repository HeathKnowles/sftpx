// Example: Client file upload using integrated orchestration
// Run with: cargo run --example client_upload -- <file_path> [server_ip]

use sftpx::client::transfer::Transfer;
use sftpx::common::config::ClientConfig;
use std::env;
use std::path::Path;

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
    println!("File to upload:");
    println!("  Path: {:?}", file_path);
    println!("  Size: {} bytes ({:.2} MB)\n", file_size, file_size as f64 / 1_048_576.0);
    
    // Create client configuration
    let server_addr = format!("{}:4443", server_ip).parse()?;
    let server_name = if server_ip == "127.0.0.1" || server_ip == "localhost" {
        "localhost".to_string()
    } else {
        server_ip.to_string() // Use IP address as server name for remote connections
    };
    
    let config = ClientConfig::new(server_addr, server_name)
        .disable_cert_verification()  // Skip cert verification for testing
        .with_chunk_size(262144)?; // 256 KB chunks
    
    println!("Client Configuration:");
    println!("  Server: {}", server_addr);
    println!("  Chunk Size: {} bytes ({} KB)", config.chunk_size, config.chunk_size / 1024);
    println!("\nFeatures:");
    println!("  ✓ Integrated orchestration (handshake → manifest → chunks)");
    println!("  ✓ BLAKE3 integrity verification per chunk");
    println!("  ✓ Protocol Buffers serialization");
    println!("  ✓ 4 QUIC streams (Control, Manifest, Data, Status)");
    println!("\nStarting upload...\n");
    
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
