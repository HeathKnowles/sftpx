// Integrated file transfer client using chunking pipeline with QUIC
// Usage: cargo run --example integrated_file_transfer_client <file_path>

use sftpx::chunking::{FileChunker, ChunkCompressor, ChunkHasher, CompressionAlgorithm};
use std::env;
use std::net::SocketAddr;
use std::path::Path;
use quiche::Config;

const MAX_DATAGRAM_SIZE: usize = 65535;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    
    println!("=== SFTPX Integrated File Transfer Client ===\n");
    println!("Client with chunking pipeline integration");
    println!("File → Chunk → Compress → Hash → Send over QUIC\n");
    
    // Parse command line arguments
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <file_path>", args[0]);
        eprintln!("\nExample:");
        eprintln!("  {} test.txt", args[0]);
        std::process::exit(1);
    }
    
    let file_path = Path::new(&args[1]);
    let filename = file_path.file_name().unwrap().to_string_lossy().to_string();
    
    // Verify file exists
    let metadata = std::fs::metadata(file_path)?;
    let file_size = metadata.len();
    
    // Determine compression based on file extension
    let extension = file_path.extension().and_then(|e| e.to_str()).unwrap_or("");
    let compression = CompressionAlgorithm::auto_select_by_extension(extension);
    
    println!("📄 File: {}", filename);
    println!("📊 Size: {} bytes", file_size);
    println!("📝 Extension: .{}", extension);
    println!("🗜️  Compression: {:?}\n", compression);
    
    // Chunking configuration
    let chunk_size = 8192; // 8KB chunks
    let mut chunker = FileChunker::new(file_path, Some(chunk_size))?;
    let total_chunks = chunker.total_chunks();
    
    println!("🔢 Total chunks: {}", total_chunks);
    println!("📦 Chunk size: {} bytes\n", chunk_size);
    
    // QUIC configuration
    let server_addr: SocketAddr = "127.0.0.1:4443".parse()?;
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
    
    println!("Connecting to {}...\n", server_addr);
    
    // Bind local socket
    let socket = std::net::UdpSocket::bind("0.0.0.0:0")?;
    socket.connect(server_addr)?;
    let local_addr = socket.local_addr()?;
    
    // Create QUIC connection
    let scid = quiche::ConnectionId::from_ref(&[0xba; 16]);
    let mut conn = quiche::connect(None, &scid, local_addr, server_addr, &mut config)?;
    
    let mut buf = vec![0u8; MAX_DATAGRAM_SIZE];
    let mut out = vec![0u8; MAX_DATAGRAM_SIZE];
    
    // Send initial packet
    let (write, send_info) = conn.send(&mut out)?;
    socket.send_to(&out[..write], send_info.to)?;
    
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
    println!("=== Starting File Transfer ===\n");
    
    // Send file metadata on control stream (stream 0)
    let metadata = serde_json::json!({
        "filename": filename,
        "size": file_size,
        "chunks": total_chunks,
        "chunk_size": chunk_size,
        "compression": format!("{:?}", compression),
    });
    
    let metadata_bytes = serde_json::to_vec(&metadata)?;
    conn.stream_send(0, &metadata_bytes, false)?;
    
    println!("📋 Sent file metadata");
    
    // Flush metadata
    while let Ok((write, send_info)) = conn.send(&mut out) {
        socket.send_to(&out[..write], send_info.to)?;
    }
    
    // Send chunks
    let mut chunk_count = 0;
    let mut stream_id = 4; // Start from stream 4 (0 is control)
    
    while let Some(chunk_packet) = chunker.next_chunk()? {
        // Extract chunk data
        let data_start = chunk_packet.len().saturating_sub(chunk_size.min(chunk_packet.len()));
        let chunk_data = &chunk_packet[data_start..];
        
        // Compress chunk
        let compressed_chunk = ChunkCompressor::compress(chunk_data, compression)?;
        
        // Hash compressed data
        let _hash = ChunkHasher::hash(&compressed_chunk.compressed_data);
        
        // Send on QUIC stream
        let is_last = chunk_count == total_chunks - 1;
        conn.stream_send(stream_id, &compressed_chunk.compressed_data, is_last)?;
        
        chunk_count += 1;
        stream_id += 1;
        
        if chunk_count % 100 == 0 {
            println!("📤 Sent {}/{} chunks ({:.1}%)", 
                chunk_count, total_chunks, 
                (chunk_count as f64 / total_chunks as f64) * 100.0);
        }
        
        // Flush packets periodically
        while let Ok((write, send_info)) = conn.send(&mut out) {
            socket.send_to(&out[..write], send_info.to)?;
        }
        
        // Receive ACKs
        match socket.recv_from(&mut buf) {
            Ok((len, from)) => {
                let recv_info = quiche::RecvInfo { from, to: local_addr };
                let _ = conn.recv(&mut buf[..len], recv_info);
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(e) => eprintln!("recv error: {}", e),
        }
    }
    
    println!("\n✅ Sent all {} chunks!", total_chunks);
    
    // Final flush
    std::thread::sleep(std::time::Duration::from_millis(100));
    while let Ok((write, send_info)) = conn.send(&mut out) {
        socket.send_to(&out[..write], send_info.to)?;
    }
    
    // Close connection
    let _ = conn.close(true, 0x00, b"done");
    while let Ok((write, send_info)) = conn.send(&mut out) {
        socket.send_to(&out[..write], send_info.to)?;
    }
    
    println!("\n🎉 File transfer complete!");
    
    // Print statistics
    let stats = conn.stats();
    println!("\n=== Transfer Statistics ===");
    println!("Sent: {} bytes", stats.sent);
    println!("Received: {} bytes", stats.recv);
    println!("Lost: {} packets", stats.lost);
    
    Ok(())
}
