// Example: Test QUIC Client with Migration & Heartbeat support
// Run with: cargo run --example test_client

use sftpx::client::ClientConnection;
use sftpx::common::config::ClientConfig;
use sftpx::common::types::{HEARTBEAT_INTERVAL, KEEPALIVE_IDLE_THRESHOLD};
use std::net::UdpSocket;
use std::time::{Duration, Instant};

const MAX_DATAGRAM_SIZE: usize = 1350;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== QUIC Client Test with Migration & Heartbeat ===\n");
    
    let server_addr = "127.0.0.1:4443".parse()?;
    let socket = UdpSocket::bind("0.0.0.0:0")?;
    let local_addr = socket.local_addr()?;
    
    println!("Client Configuration:");
    println!("  - Local Address: {}", local_addr);
    println!("  - Server Address: {}", server_addr);
    println!("  - Max Datagram Size: {} bytes", MAX_DATAGRAM_SIZE);
    println!("\nFeatures:");
    println!("  ✓ Connection Migration Support");
    println!("  ✓ Heartbeat Interval: {:?}", HEARTBEAT_INTERVAL);
    println!("  ✓ Idle Threshold: {:?}", KEEPALIVE_IDLE_THRESHOLD);
    println!("  ✓ 4 Streams: Control, Manifest, Data, Status\n");
    
    // Create client configuration
    let config = ClientConfig::new(server_addr, "localhost".to_string())
        .disable_cert_verification();
    
    // Create client connection
    println!("Creating QUIC connection...");
    let mut conn = ClientConnection::new(&config, local_addr)?;
    println!("✓ Connection initialized");
    println!("✓ Migration enabled: {}", conn.is_migration_enabled());
    
    let mut buf = [0u8; MAX_DATAGRAM_SIZE];
    let mut out = [0u8; MAX_DATAGRAM_SIZE];
    
    // Send initial packet
    let (write, send_info) = conn.send(&mut out)?;
    socket.send_to(&out[..write], send_info.to)?;
    println!("✓ Sent initial packet ({} bytes)", write);
    
    // Complete handshake
    println!("\nCompleting handshake...");
    socket.set_read_timeout(Some(Duration::from_secs(5)))?;
    
    let handshake_deadline = Instant::now() + Duration::from_secs(5);
    while !conn.is_established() && Instant::now() < handshake_deadline {
        // Receive packets
        match socket.recv_from(&mut buf) {
            Ok((len, from)) => {
                println!("  - Received {} bytes", len);
                let recv_info = quiche::RecvInfo {
                    from,
                    to: local_addr,
                };
                match conn.recv(&mut buf[..len], recv_info) {
                    Ok(_) => {
                        // Check if peer migrated
                        if conn.has_peer_migrated(from) {
                            println!("  ! Server migrated to new address: {}", from);
                            conn.update_peer_address(from);
                        }
                    },
                    Err(e) => eprintln!("  - recv error: {:?}", e),
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock || 
                      e.kind() == std::io::ErrorKind::TimedOut => {}
            Err(e) => {
                eprintln!("Socket recv error: {:?}", e);
                break;
            }
        }
        
        // Send any response packets
        loop {
            match conn.send(&mut out) {
                Ok((write, send_info)) => {
                    socket.send_to(&out[..write], send_info.to)?;
                }
                Err(_) => break,
            }
        }
        
        if conn.is_established() {
            println!("✓ Connection established!\n");
            break;
        }
        
        std::thread::sleep(Duration::from_millis(10));
    }
    
    if !conn.is_established() {
        return Err("Handshake timeout".into());
    }
    
    // Send test heartbeat
    println!("Testing heartbeat functionality...");
    if conn.should_send_heartbeat() {
        println!("  - Sending heartbeat PING...");
        conn.send_heartbeat()?;
        
        // Flush packets
        while let Ok((write, send_info)) = conn.send(&mut out) {
            socket.send_to(&out[..write], send_info.to)?;
        }
        println!("✓ Heartbeat sent");
    }
    
    // Send application data on stream 0 (Control)
    println!("\nSending application data...");
    let stream_id = 0u64; // Control stream
    let message = b"Hello from QUIC client with migration support!";
    
    match conn.stream_send(stream_id, message, false) {
        Ok(written) => {
            println!("✓ Sent {} bytes on stream {} (Control)", written, stream_id);
        }
        Err(e) => {
            eprintln!("✗ Failed to send data: {:?}", e);
            return Err(Box::new(e));
        }
    }
    
    // Flush the stream
    while let Ok((write, send_info)) = conn.send(&mut out) {
        socket.send_to(&out[..write], send_info.to)?;
    }
    
    // Monitor connection and wait for response
    println!("\nMonitoring connection...");
    let mut response_received = false;
    let deadline = Instant::now() + Duration::from_secs(5);
    
    while Instant::now() < deadline && !conn.is_closed() {
        // Check idle status
        if conn.is_idle() {
            println!("  ! Connection idle for {:?}", conn.idle_duration());
        }
        
        // Send heartbeat if needed
        if conn.should_send_heartbeat() {
            println!("  - Sending periodic heartbeat...");
            let _ = conn.send_heartbeat();
        }
        
        // Receive packets
        if let Ok((len, from)) = socket.recv_from(&mut buf) {
            let recv_info = quiche::RecvInfo { from, to: local_addr };
            match conn.recv(&mut buf[..len], recv_info) {
                Ok(_) => {
                    // Detect migration
                    if conn.has_peer_migrated(from) {
                        println!("  ! Server migrated (count: {})", 
                            if from != conn.server_addr() { 1 } else { 0 });
                    }
                }
                Err(e) => eprintln!("  - recv error: {:?}", e),
            }
        }
        
        // Check for readable streams
        let readable_streams: Vec<u64> = conn.readable().collect();
        for stream_id in readable_streams {
            println!("✓ Stream {} is readable", stream_id);
            
            let mut stream_buf = [0u8; 1024];
            while let Ok((read, fin)) = conn.stream_recv(stream_id, &mut stream_buf) {
                if read > 0 {
                    // Check if it's a heartbeat response
                    if conn.handle_heartbeat(&stream_buf[..read]) {
                        println!("  - Heartbeat response received");
                    } else {
                        let response = String::from_utf8_lossy(&stream_buf[..read]);
                        println!("✓ Received from server: \"{}\"", response);
                        response_received = true;
                    }
                }
                
                if fin {
                    println!("✓ Stream {} finished", stream_id);
                    break;
                }
            }
        }
        
        // Flush any outgoing packets
        while let Ok((write, send_info)) = conn.send(&mut out) {
            socket.send_to(&out[..write], send_info.to)?;
        }
        
        if response_received {
            break;
        }
        
        std::thread::sleep(Duration::from_millis(10));
    }
    
    // Display connection statistics
    println!("\nConnection Statistics:");
    let stats = conn.stats();
    println!("  - Bytes sent: {}", stats.bytes_sent);
    println!("  - Bytes received: {}", stats.bytes_received);
    println!("  - Packets sent: {}", stats.packets_sent);
    println!("  - Packets received: {}", stats.packets_received);
    println!("  - Idle duration: {:?}", conn.idle_duration());
    println!("  - Time since heartbeat: {:?}", conn.time_since_heartbeat());
    
    if response_received {
        println!("\n✓ Test PASSED: Communication successful!");
    } else {
        println!("\n⚠ Test PARTIAL: No application response (heartbeat may have worked)");
    }
    
    // Close connection
    println!("\nClosing connection...");
    conn.close(true, 0x00, b"done")?;
    while let Ok((write, send_info)) = conn.send(&mut out) {
        socket.send_to(&out[..write], send_info.to)?;
    }
    println!("✓ Connection closed\n");
    
    Ok(())
}
