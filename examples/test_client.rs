// Example: Test QUIC Client
// Run with: cargo run --example test_client

use quiche::Config;
use std::net::UdpSocket;

const MAX_DATAGRAM_SIZE: usize = 1350;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== QUIC Client Test ===\n");
    
    let server_addr = "127.0.0.1:4443";
    let socket = UdpSocket::bind("0.0.0.0:0")?;
    let local_addr = socket.local_addr()?;
    
    println!("Client Configuration:");
    println!("  - Local Address: {}", local_addr);
    println!("  - Server Address: {}", server_addr);
    println!("  - Max Datagram Size: {} bytes\n", MAX_DATAGRAM_SIZE);
    
    // Configure QUIC client
    let mut config = Config::new(quiche::PROTOCOL_VERSION)?;
    config.set_application_protos(&[b"hq-29"])?;
    config.verify_peer(false);
    config.set_max_idle_timeout(5000);
    config.set_max_recv_udp_payload_size(MAX_DATAGRAM_SIZE);
    config.set_max_send_udp_payload_size(MAX_DATAGRAM_SIZE);
    config.set_initial_max_data(10_000_000);
    config.set_initial_max_stream_data_bidi_local(1_000_000);
    config.set_initial_max_stream_data_bidi_remote(1_000_000);
    config.set_initial_max_streams_bidi(100);
    
    // Generate a random connection ID
    let mut scid = [0; quiche::MAX_CONN_ID_LEN];
    let rng = ring::rand::SystemRandom::new();
    ring::rand::SecureRandom::fill(&rng, &mut scid).unwrap();
    let scid = quiche::ConnectionId::from_ref(&scid);
    
    // Create connection
    let server_name = "localhost";
    let mut conn = quiche::connect(
        Some(server_name),
        &scid,
        local_addr,
        server_addr.parse()?,
        &mut config,
    )?;
    
    println!("✓ QUIC connection initialized");
    
    let mut buf = [0u8; MAX_DATAGRAM_SIZE];
    let mut out = [0u8; MAX_DATAGRAM_SIZE];
    
    // Send initial packet
    let (write, send_info) = conn.send(&mut out)?;
    socket.send_to(&out[..write], send_info.to)?;
    println!("✓ Sent initial packet ({} bytes)", write);
    
    // Complete handshake
    println!("\nCompleting handshake...");
    loop {
        if conn.is_established() {
            println!("✓ Connection established!\n");
            break;
        }
        
        // Receive packets
        match socket.recv_from(&mut buf) {
            Ok((len, from)) => {
                println!("  - Received {} bytes from {}", len, from);
                let recv_info = quiche::RecvInfo {
                    from,
                    to: local_addr,
                };
                conn.recv(&mut buf[..len], recv_info)?;
            }
            Err(e) => {
                eprintln!("Socket recv error: {:?}", e);
                break;
            }
        }
        
        // Send any response packets
        while let Ok((write, send_info)) = conn.send(&mut out) {
            socket.send_to(&out[..write], send_info.to)?;
        }
    }
    
    // Send application data on stream 0
    println!("Sending application data...");
    let stream_id = 0u64;
    let message = b"Hello from QUIC client!";
    
    match conn.stream_send(stream_id, message, false) {
        Ok(written) => {
            println!("✓ Sent {} bytes on stream {}", written, stream_id);
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
    
    // Wait for server response
    println!("\nWaiting for server response...");
    socket.set_read_timeout(Some(std::time::Duration::from_secs(5)))?;
    
    let mut response_received = false;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    
    while std::time::Instant::now() < deadline && !conn.is_closed() {
        // Receive packets
        if let Ok((len, from)) = socket.recv_from(&mut buf) {
            let recv_info = quiche::RecvInfo { from, to: local_addr };
            match conn.recv(&mut buf[..len], recv_info) {
                Ok(_) => {}
                Err(e) => eprintln!("Client: conn.recv error: {:?}", e),
            }
        }
        
        // Check for readable streams
        for stream_id in conn.readable() {
            println!("✓ Stream {} is readable", stream_id);
            
            while let Ok((read, fin)) = conn.stream_recv(stream_id, &mut buf) {
                if read > 0 {
                    let response = String::from_utf8_lossy(&buf[..read]);
                    println!("✓ Received from server: \"{}\"", response);
                    response_received = true;
                }
                
                if fin {
                    println!("✓ Stream {} finished", stream_id);
                    break;
                }
            }
        }
        
        if response_received {
            break;
        }
        
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    
    if response_received {
        println!("\n✓ Test PASSED: Communication successful!");
    } else {
        println!("\n✗ Test FAILED: No response from server");
    }
    
    // Close connection
    conn.close(true, 0x00, b"done")?;
    let (write, send_info) = conn.send(&mut out)?;
    socket.send_to(&out[..write], send_info.to)?;
    
    println!("✓ Connection closed\n");
    
    Ok(())
}
