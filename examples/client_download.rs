// Example: Client file download using integrated orchestration
// Run with: cargo run --example client_download

use sftpx::client::transfer::Transfer;
use sftpx::common::config::ClientConfig;
use std::env;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    
    println!("=== SFTPX Client Download Example ===\n");
    
    // Create client configuration
    let server_addr = "127.0.0.1:4443".parse()?;
    let mut config = ClientConfig::new(server_addr, "localhost".to_string())
        .disable_cert_verification()
        .with_chunk_size(262144)?; // 256 KB chunks
    
    // Set session directory
    let temp_dir = env::temp_dir().join("sftpx_downloads");
    std::fs::create_dir_all(&temp_dir)?;
    config.session_dir = temp_dir.clone();
    
    println!("Client Configuration:");
    println!("  Server: {}", server_addr);
    println!("  Download Directory: {:?}", temp_dir);
    println!("  Chunk Size: {} bytes ({} KB)", config.chunk_size, config.chunk_size / 1024);
    println!("\nFeatures:");
    println!("  ✓ Integrated orchestration (handshake → manifest → chunks)");
    println!("  ✓ BLAKE3 integrity verification per chunk");
    println!("  ✓ Automatic re-request on corruption");
    println!("  ✓ Protocol Buffers serialization");
    println!("  ✓ 4 QUIC streams (Control, Manifest, Data, Status)");
    println!("\nStarting download...\n");
    
    // Create transfer and run download
    let mut transfer = Transfer::receive_file(config, "download_session")?;
    
    match transfer.run_receive() {
        Ok(output_path) => {
            println!("\n✅ Download successful!");
            println!("  File saved to: {:?}", output_path);
            println!("  Transfer state: {:?}", transfer.state());
            
            if let Ok(metadata) = std::fs::metadata(&output_path) {
                println!("  File size: {} bytes ({:.2} MB)", 
                    metadata.len(), 
                    metadata.len() as f64 / 1_048_576.0
                );
            }
        }
        Err(e) => {
            eprintln!("\n❌ Download failed: {:?}", e);
            return Err(e.into());
        }
    }
    
    Ok(())
}
