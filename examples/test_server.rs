// Example: Test QUIC Server with Migration & Heartbeat support
// Run with: cargo run --example test_server

use sftpx::server::{Server, ServerConfig};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== QUIC Server Test with Migration & Heartbeat ===\n");
    
    // Create server configuration
    let config = ServerConfig {
        bind_addr: "127.0.0.1:4443".to_string(),
        cert_path: "certs/cert.pem".to_string(),
        key_path: "certs/key.pem".to_string(),
        max_idle_timeout: 30000, // 30 seconds
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
    println!("  - Max Streams: {}", config.max_streams);
    println!("\nFeatures:");
    println!("  ✓ Connection Migration Support");
    println!("  ✓ Heartbeat/Keepalive (30s interval)");
    println!("  ✓ 4 Streams: Control, Manifest, Data, Status\n");
    
    // Create and run server
    println!("Starting QUIC server...");
    let mut server = Server::new(config)?;
    
    println!("✓ Server initialized successfully");
    println!("✓ Listening for connections...\n");
    println!("Server will automatically:");
    println!("  - Detect client migrations");
    println!("  - Respond to PING with PONG");
    println!("  - Track idle connections\n");
    
    // Run server (blocks until connection is handled)
    server.run()?;
    
    println!("\n✓ Server completed successfully");
    
    Ok(())
}
