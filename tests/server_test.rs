// Integration test for QUIC server
// Run with: cargo test --test server_test

use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

#[test]
#[ignore] // Run with: cargo test --test server_test -- --ignored
fn test_server_client_communication() {
    // Start server in background
    let mut server = Command::new("cargo")
        .args(&["run", "--example", "test_server"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to start server");
    
    // Wait for server to start
    thread::sleep(Duration::from_secs(2));
    
    // Run client
    let client_output = Command::new("cargo")
        .args(&["run", "--example", "test_client"])
        .output()
        .expect("Failed to run client");
    
    // Kill server
    server.kill().expect("Failed to kill server");
    
    // Check results
    let stdout = String::from_utf8_lossy(&client_output.stdout);
    let stderr = String::from_utf8_lossy(&client_output.stderr);
    
    println!("=== Client Output ===");
    println!("{}", stdout);
    
    if !stderr.is_empty() {
        println!("=== Client Errors ===");
        println!("{}", stderr);
    }
    
    // Verify communication was successful
    assert!(
        stdout.contains("Connection established") || stdout.contains("Test PASSED"),
        "Client did not establish connection successfully"
    );
}

#[test]
fn test_server_config_defaults() {
    use sftpx::server::ServerConfig;
    
    let config = ServerConfig::default();
    
    assert_eq!(config.bind_addr, "127.0.0.1:4443");
    assert_eq!(config.cert_path, "certs/cert.pem");
    assert_eq!(config.key_path, "certs/key.pem");
    assert_eq!(config.max_idle_timeout, 5000);
    assert_eq!(config.max_data, 10_000_000);
    assert_eq!(config.max_stream_data, 1_000_000);
    assert_eq!(config.max_streams, 100);
}

#[test]
fn test_stream_manager() {
    use sftpx::server::{StreamManager, StreamType};
    
    let mut manager = StreamManager::new();
    
    // Test stream type IDs
    assert_eq!(StreamType::Control.stream_id(), 0);
    assert_eq!(StreamType::Data1.stream_id(), 4);
    assert_eq!(StreamType::Data2.stream_id(), 8);
    assert_eq!(StreamType::Data3.stream_id(), 12);
    
    // Test all streams
    let all_streams = StreamType::all();
    assert_eq!(all_streams.len(), 4);
    
    // Test initial state
    assert_eq!(manager.stream_count(), 0);
    
    let stats = manager.get_statistics();
    assert_eq!(stats.total_streams, 0);
    assert_eq!(stats.active_streams, 0);
}

#[test]
fn test_data_sender() {
    use sftpx::server::DataSender;
    
    let sender = DataSender::new();
    assert_eq!(sender.total_bytes_sent(), 0);
    
    let mut sender = DataSender::default();
    sender.reset_counter();
    assert_eq!(sender.total_bytes_sent(), 0);
}

#[test]
fn test_transfer_manager() {
    use sftpx::server::TransferManager;
    
    let manager = TransferManager::new();
    assert_eq!(manager.chunk_size(), 8192);
    assert_eq!(manager.total_bytes_sent(), 0);
    
    let manager = TransferManager::with_chunk_size(16384);
    assert_eq!(manager.chunk_size(), 16384);
    
    let mut manager = TransferManager::default();
    manager.set_chunk_size(32768);
    assert_eq!(manager.chunk_size(), 32768);
}
