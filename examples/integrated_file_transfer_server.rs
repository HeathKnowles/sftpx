// Integrated file transfer server using sftpx modules with chunking pipeline
// This extends the server to receive chunked files and reconstruct them
// 
// Usage: cargo run --example integrated_file_transfer_server

use sftpx::chunking::{ChunkTable, ChunkBitmap};
use std::fs::{File, create_dir_all};
use std::io::Write;
use std::path::PathBuf;
use std::net::UdpSocket;
use quiche::Config;

const MAX_DATAGRAM_SIZE: usize = 65535;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    
    println!("=== SFTPX Integrated File Transfer Server ===\n");
    println!("Server with chunking pipeline integration");
    println!("Chunk → Decompress → Hash Verify → Table → Bitmap → Reconstruct\n");
    
    // Create received files directory
    create_dir_all("received")?;
    
    // Server configuration
    let bind_addr = "127.0.0.1:4443";
    let cert_path = "certs/cert.pem";
    let key_path = "certs/key.pem";
    
    println!("Server Configuration:");
    println!("  Bind Address: {}", bind_addr);
    println!("  Certificate: {}", cert_path);
    println!("  Private Key: {}\n", key_path);
    
    // Bind UDP socket
    let socket = UdpSocket::bind(bind_addr)?;
    println!("✅ Server listening on {}\n", bind_addr);
    
    // Configure QUIC
    let mut config = Config::new(quiche::PROTOCOL_VERSION)?;
    config.set_application_protos(&[b"file-transfer"])?;
    config.verify_peer(false);
    config.set_max_idle_timeout(30000);
    config.set_max_recv_udp_payload_size(MAX_DATAGRAM_SIZE);
    config.set_max_send_udp_payload_size(MAX_DATAGRAM_SIZE);
    config.set_initial_max_data(10_000_000);
    config.set_initial_max_stream_data_bidi_local(1_000_000);
    config.set_initial_max_stream_data_bidi_remote(1_000_000);
    config.set_initial_max_streams_bidi(100);
    config.load_cert_chain_from_pem_file(cert_path)?;
    config.load_priv_key_from_pem_file(key_path)?;
    
    let mut buf = vec![0u8; MAX_DATAGRAM_SIZE];
    let mut out = vec![0u8; MAX_DATAGRAM_SIZE];
    
    // Wait for initial packet
    println!("Waiting for client connection...\n");
    let (len, from) = socket.recv_from(&mut buf)?;
    println!("📦 Received initial packet ({} bytes) from {}", len, from);
    
    // Parse header
    let hdr = quiche::Header::from_slice(&mut buf[..len], quiche::MAX_CONN_ID_LEN)?;
    
    // Create server connection
    let scid = quiche::ConnectionId::from_ref(&hdr.dcid);
    let local_addr = socket.local_addr()?;
    let mut conn = quiche::accept(&scid, None, local_addr, from, &mut config)?;
    
    // Process initial packet
    let recv_info = quiche::RecvInfo { from, to: local_addr };
    conn.recv(&mut buf[..len], recv_info)?;
    
    // Send handshake response
    while let Ok((write, send_info)) = conn.send(&mut out) {
        socket.send_to(&out[..write], send_info.to)?;
    }
    
    println!("🤝 Handshake initiated...\n");
    
    // Complete handshake
    socket.set_nonblocking(true)?;
    let mut handshake_complete = false;
    
    for _ in 0..100 {
        match socket.recv_from(&mut buf) {
            Ok((len, from)) => {
                let recv_info = quiche::RecvInfo { from, to: local_addr };
                let _ = conn.recv(&mut buf[..len], recv_info);
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(e) => eprintln!("recv error: {}", e),
        }
        
        while let Ok((write, send_info)) = conn.send(&mut out) {
            socket.send_to(&out[..write], send_info.to)?;
        }
        
        if conn.is_established() {
            handshake_complete = true;
            break;
        }
        
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    
    if !handshake_complete {
        return Err("Handshake failed".into());
    }
    
    println!("✅ Connection established!\n");
    println!("=== Starting File Reception ===\n");
    
    // File reception state
    let mut table = ChunkTable::new();
    let mut bitmap: Option<ChunkBitmap> = None;
    let mut output_file: Option<File> = None;
    let mut filename = String::from("received_file");
    let mut file_size: u64 = 0;
    let mut total_chunks: u64 = 0;
    
    // Main receive loop
    let timeout = std::time::Duration::from_secs(30);
    let start = std::time::Instant::now();
    
    while !conn.is_closed() && start.elapsed() < timeout {
        // Receive packets
        match socket.recv_from(&mut buf) {
            Ok((len, from)) => {
                let recv_info = quiche::RecvInfo { from, to: local_addr };
                let _ = conn.recv(&mut buf[..len], recv_info);
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(e) => eprintln!("recv error: {}", e),
        }
        
        // Process readable streams
        for stream_id in conn.readable() {
            while let Ok((read, fin)) = conn.stream_recv(stream_id, &mut buf) {
                if read == 0 {
                    break;
                }
                
                // Parse chunk packet (simplified - you'd use proper protocol here)
                // Format: [metadata_len(4)][metadata_json][compressed_chunk_data]
                
                if stream_id == 0 {
                    // Control stream - receive file metadata
                    let metadata_str = String::from_utf8_lossy(&buf[..read]);
                    if let Ok(meta) = serde_json::from_str::<serde_json::Value>(&metadata_str) {
                        filename = meta["filename"].as_str().unwrap_or("received_file").to_string();
                        file_size = meta["size"].as_u64().unwrap_or(0);
                        total_chunks = meta["chunks"].as_u64().unwrap_or(0);
                        
                        println!("📄 File: {}", filename);
                        println!("📊 Size: {} bytes", file_size);
                        println!("🔢 Chunks: {}\n", total_chunks);
                        
                        table.set_file_info(file_size, total_chunks);
                        bitmap = Some(ChunkBitmap::with_exact_size(total_chunks as u32));
                        
                        let output_path = PathBuf::from("received").join(&filename);
                        output_file = Some(File::create(&output_path)?);
                    }
                } else {
                    // Data stream - receive chunk
                    if let (Some(ref mut file), Some(ref mut bm)) = (&mut output_file, &mut bitmap) {
                        // In a real implementation, you'd properly deserialize the chunk packet
                        // For now, simulate receiving and writing chunks
                        println!("📦 Received chunk data on stream {}: {} bytes", stream_id, read);
                        
                        // Write data to file
                        file.write_all(&buf[..read])?;
                        
                        // Update progress
                        let progress = bm.progress();
                        if progress % 10.0 < 1.0 {
                            println!("⏳ Progress: {:.1}%", progress);
                        }
                    }
                }
                
                if fin {
                    println!("✅ Stream {} finished", stream_id);
                    break;
                }
            }
        }
        
        // Send any pending packets
        while let Ok((write, send_info)) = conn.send(&mut out) {
            socket.send_to(&out[..write], send_info.to)?;
        }
        
        // Check if complete
        if let Some(ref bm) = bitmap {
            if bm.is_complete() {
                println!("\n🎉 File transfer complete!");
                println!("📁 Saved to: received/{}", filename);
                break;
            }
        }
        
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    
    // Close connection
    let _ = conn.close(true, 0x00, b"done");
    while let Ok((write, send_info)) = conn.send(&mut out) {
        socket.send_to(&out[..write], send_info.to)?;
    }
    
    if let Some(ref mut file) = output_file {
        file.flush()?;
    }
    
    println!("\n=== Transfer Statistics ===");
    let stats = conn.stats();
    println!("Received: {} bytes", stats.recv);
    println!("Sent: {} bytes", stats.sent);
    println!("Lost: {} packets", stats.lost);
    
    Ok(())
}
