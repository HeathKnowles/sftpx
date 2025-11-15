// Simple client example demonstrating 4-stream QUIC connection
// Run with: cargo run --example simple_client

use sftpx::client::Client;
use sftpx::common::{ClientConfig, Result};
use env_logger;

fn main() -> Result<()> {
    // Initialize logger
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    
    println!("=== SFTPX Client Examples ===\n");
    
    // Example 1: Using default configuration (localhost:4433 with certs/cert.pem)
    println!("Example 1: Default configuration");
    let client = Client::default();
    println!("  Server: {:?}", client.config().server_addr);
    println!("  Cert: {:?}", client.config().ca_cert_path);
    
    // Example 2: Using with_defaults helper for custom server address
    println!("\nExample 2: Custom server with defaults");
    let client = Client::with_defaults("127.0.0.1:8443")?;
    println!("  Server: {:?}", client.config().server_addr);
    println!("  Cert: {:?}", client.config().ca_cert_path);
    
    // Example 3: Fully custom configuration
    println!("\nExample 3: Fully custom configuration");
    let config = ClientConfig::default()
        .with_chunk_size(2 * 1024 * 1024)?  // 2MB chunks
        .with_max_retries(5)
        .enable_cert_verification()
        .with_ca_cert(std::path::PathBuf::from("certs/cert.pem"));
    
    let client = Client::new(config);
    println!("  Chunk size: {} bytes", client.config().chunk_size);
    println!("  Max retries: {}", client.config().max_retries);
    println!("  Verify cert: {}", client.config().verify_cert);
    
    // Example 4: Actually run a transfer (uncomment to test with real server)
    /*
    println!("\nExample 4: Running actual transfer");
    let mut transfer = client.send_file("test_file.txt", "output/")?;
    match transfer.run() {
        Ok(_) => {
            println!("Transfer completed successfully!");
            println!("Progress: {:.2}%", transfer.progress());
        }
        Err(e) => {
            eprintln!("Transfer failed: {:?}", e);
            return Err(e);
        }
    }
    */
    
    println!("\n=== Examples completed ===");
    Ok(())
}
