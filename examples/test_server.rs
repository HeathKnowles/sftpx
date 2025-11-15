// Example: Test QUIC Server
// Run with: cargo run --example test_server

use sftpx::server::{Server, ServerConfig};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== QUIC Server Test ===\n");
    
    // Create server configuration
    let config = ServerConfig {
        bind_addr: "127.0.0.1:4443".to_string(),
        cert_path: "certs/cert.pem".to_string(),
        key_path: "certs/key.pem".to_string(),
        max_idle_timeout: 5000,
        max_data: 10_000_000,
        max_stream_data: 1_000_000,
        max_streams: 100,
    };
    
    println!("Server Configuration:");
    println!("  - Address: {}", config.bind_addr);
    println!("  - Certificate: {}", config.cert_path);
    println!("  - Private Key: {}", config.key_path);
    println!("  - Max Idle Timeout: {}ms", config.max_idle_timeout);
    println!("  - Max Data: {} bytes", config.max_data);
    println!("  - Max Streams: {}\n", config.max_streams);
    
    // Create and run server
    println!("Starting QUIC server...");
    let mut server = Server::new(config)?;
    
    println!("✓ Server initialized successfully");
    println!("✓ Listening for connections...\n");
    
    // Run server (blocks until connection is handled)
    server.run()?;
    
    println!("\n✓ Server completed successfully");
    
    Ok(())
}
