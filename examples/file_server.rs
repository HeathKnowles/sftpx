// Example: Integrated file server that handles both uploads and downloads
// Run with: cargo run --example file_server

use sftpx::server::{Server, ServerConfig};
use std::env;
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    
    println!("=== SFTPX Integrated File Server ===\n");
    
    // Create server configuration
    let config = ServerConfig {
        bind_addr: "0.0.0.0:4443".to_string(),  // Bind to all interfaces for remote access
        cert_path: "certs/cert.pem".to_string(),
        key_path: "certs/key.pem".to_string(),
        max_idle_timeout: 30000, // 30 seconds
        max_data: 100_000_000,   // 100 MB
        max_stream_data: 10_000_000, // 10 MB per stream
        max_streams: 100,
    };
    
    // Set up directories
    let upload_dir = PathBuf::from("./uploads");
    let download_dir = PathBuf::from("./test_files");
    
    std::fs::create_dir_all(&upload_dir)?;
    std::fs::create_dir_all(&download_dir)?;
    
    println!("Server Configuration:");
    println!("  Address: {}", config.bind_addr);
    println!("  Certificate: {}", config.cert_path);
    println!("  Private Key: {}", config.key_path);
    println!("  Upload Directory: {:?}", upload_dir);
    println!("  Download Directory: {:?}", download_dir);
    println!("  Max Data: {} bytes ({} MB)", config.max_data, config.max_data / 1_048_576);
    println!("  Max Idle Timeout: {}ms", config.max_idle_timeout);
    
    println!("\nFeatures:");
    println!("  ✓ Bidirectional file transfer (upload & download)");
    println!("  ✓ Integrated orchestration (manifest + chunks)");
    println!("  ✓ BLAKE3 integrity verification");
    println!("  ✓ Automatic retransmission on corruption");
    println!("  ✓ Protocol Buffers serialization");
    println!("  ✓ 4 QUIC streams: Control(0), Manifest(1), Data(2), Status(3)");
    println!("  ✓ Connection migration support");
    println!("  ✓ Heartbeat/keepalive");
    
    println!("\nServer will:");
    println!("  • Accept file uploads to: {:?}", upload_dir);
    println!("  • Serve file downloads from: {:?}", download_dir);
    println!("  • Automatically handle manifest exchange");
    println!("  • Verify chunk integrity with BLAKE3");
    println!("  • Support automatic re-request on corruption\n");
    
    // Create and run server
    println!("Starting QUIC file server...");
    let mut server = Server::new(config)?;
    
    println!("✓ Server initialized successfully");
    println!("✓ Listening for connections...\n");
    println!("Ready to accept uploads and downloads!");
    println!("Press Ctrl+C to stop\n");
    
    // Run server (blocks until connection is handled)
    server.run()?;
    
    println!("\n✓ Server completed successfully");
    
    Ok(())
}
