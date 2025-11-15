// QUIC File Transfer Client
// Sends files to server using the chunking pipeline with compression

use sftpx::chunking::{FileChunker, ChunkCompressor, ChunkHasher, CompressionAlgorithm, CompressionStats};
use std::net::UdpSocket;
use std::path::Path;
use std::time::Duration;

const MAX_DATAGRAM_SIZE: usize = 1350;
const CHUNK_DATA_SIZE: usize = 8192; // 8KB chunks

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("Usage: {} <server_addr> <file_path>", args[0]);
        eprintln!("Example: {} 192.168.1.100:4443 /path/to/file.txt", args[0]);
        std::process::exit(1);
    }

    let server_addr = &args[1];
    let file_path = Path::new(&args[2]);

    println!("=== QUIC File Transfer Client ===\n");
    println!("Server: {}", server_addr);
    println!("File: {}\n", file_path.display());

    // Validate file exists
    if !file_path.exists() {
        return Err(format!("File not found: {}", file_path.display()).into());
    }

    // Determine compression algorithm based on file extension
    let extension = file_path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");
    let compression_algorithm = CompressionAlgorithm::auto_select_by_extension(extension);
    
    println!("📝 Extension: .{}", extension);
    println!("🗜️  Compression: {:?}\n", compression_algorithm);

    // Create chunker
    let mut chunker = FileChunker::new(file_path, Some(CHUNK_DATA_SIZE))?;
    let total_chunks = chunker.total_chunks();
    let file_size = chunker.file_size();
    let filename = file_path.file_name().unwrap().to_str().unwrap();

    println!("File size: {} bytes", file_size);
    println!("Total chunks: {}", total_chunks);
    println!("Chunk size: {} bytes\n", CHUNK_DATA_SIZE);

    // Connect to server
    let socket = UdpSocket::bind("0.0.0.0:0")?;
    socket.set_read_timeout(Some(Duration::from_secs(5)))?;
    socket.connect(server_addr)?;

    // Generate session ID
    let session_id = format!("{:032x}", rand::random::<u128>());
    
    println!("Session ID: {}\n", &session_id[..16]);

    // Send START packet
    let compression_code = match compression_algorithm {
        CompressionAlgorithm::None => 0u8,
        CompressionAlgorithm::Lz4 => 1u8,
        CompressionAlgorithm::Lz4Hc(_) => 2u8,
        CompressionAlgorithm::Zstd(_) => 3u8,
        CompressionAlgorithm::Lzma2(_) => 4u8,
    };

    let mut start_packet = vec![0x01]; // START type
    start_packet.extend_from_slice(session_id.as_bytes());
    start_packet.extend_from_slice(&file_size.to_be_bytes());
    start_packet.extend_from_slice(&total_chunks.to_be_bytes());
    start_packet.push(compression_code);
    start_packet.extend_from_slice(&(filename.len() as u16).to_be_bytes());
    start_packet.extend_from_slice(filename.as_bytes());

    socket.send(&start_packet)?;
    println!("📤 Sent START packet");

    // Wait for ACK
    let mut ack_buf = [0u8; 256];
    match socket.recv(&mut ack_buf) {
        Ok(len) => {
            if len > 0 && ack_buf[0] == 0x02 {
                println!("✓ Server acknowledged\n");
            } else {
                return Err("Invalid ACK from server".into());
            }
        }
        Err(e) => {
            return Err(format!("Timeout waiting for server ACK: {}", e).into());
        }
    }

    // Send chunks with compression
    let mut compression_stats = CompressionStats::new();
    let mut chunk_count = 0;

    println!("🚀 Starting file transfer...\n");

    while let Some(chunk_packet) = chunker.next_chunk()? {
        // Extract chunk data
        let data_start = chunk_packet.len().saturating_sub(CHUNK_DATA_SIZE.min(chunk_packet.len()));
        let chunk_data = &chunk_packet[data_start..];

        // Compress the chunk
        let compressed_chunk = ChunkCompressor::compress(chunk_data, compression_algorithm)?;
        compression_stats.add_chunk(&compressed_chunk);

        // Hash the compressed data
        let hash = ChunkHasher::hash(&compressed_chunk.compressed_data);

        // Determine if this is the last chunk
        let is_eof = chunk_count == total_chunks - 1;

        // Build CHUNK packet
        let mut chunk_pkt = vec![0x03]; // CHUNK type
        chunk_pkt.extend_from_slice(session_id.as_bytes());
        chunk_pkt.extend_from_slice(&chunk_count.to_be_bytes());
        chunk_pkt.extend_from_slice(&(hash.len() as u16).to_be_bytes());
        chunk_pkt.extend_from_slice(&hash);
        chunk_pkt.push(if is_eof { 1 } else { 0 });
        chunk_pkt.extend_from_slice(&compressed_chunk.compressed_data);

        // Send chunk
        socket.send(&chunk_pkt)?;

        print!("📦 Chunk {}/{}: {} → {} bytes ({:?}) ",
            chunk_count + 1,
            total_chunks,
            compressed_chunk.original_size,
            compressed_chunk.compressed_size,
            compressed_chunk.algorithm
        );

        // Wait for chunk ACK (with short timeout)
        socket.set_read_timeout(Some(Duration::from_millis(100)))?;
        match socket.recv(&mut ack_buf) {
            Ok(len) => {
                if len > 0 && ack_buf[0] == 0x04 {
                    println!("✓");
                } else {
                    println!("⚠ No ACK");
                }
            }
            Err(_) => {
                println!("⚠ Timeout");
            }
        }

        chunk_count += 1;
    }

    println!("\n✅ All chunks sent!");

    // Wait for COMPLETE message
    socket.set_read_timeout(Some(Duration::from_secs(5)))?;
    match socket.recv(&mut ack_buf) {
        Ok(len) => {
            if len > 0 && ack_buf[0] == 0x05 {
                println!("✓ Server confirmed transfer complete\n");
            }
        }
        Err(_) => {
            println!("⚠ No completion confirmation from server\n");
        }
    }

    // Print compression statistics
    println!("=== Compression Statistics ===\n");
    println!("Total chunks: {}", compression_stats.total_chunks);
    println!("Original size: {} bytes", compression_stats.original_bytes);
    println!("Compressed size: {} bytes", compression_stats.compressed_bytes);
    let savings_percent = if compression_stats.original_bytes > 0 {
        (compression_stats.space_saved() as f64 / compression_stats.original_bytes as f64) * 100.0
    } else { 0.0 };
    println!("Savings: {:.1}% ({} bytes saved)", savings_percent, compression_stats.space_saved());
    
    println!("\nAlgorithm usage:");
    if compression_stats.none_count > 0 {
        println!("  - None: {} chunks", compression_stats.none_count);
    }
    if compression_stats.lz4_count > 0 {
        println!("  - LZ4: {} chunks", compression_stats.lz4_count);
    }
    if compression_stats.lz4hc_count > 0 {
        println!("  - LZ4HC: {} chunks", compression_stats.lz4hc_count);
    }
    if compression_stats.zstd_count > 0 {
        println!("  - Zstd: {} chunks", compression_stats.zstd_count);
    }
    if compression_stats.lzma2_count > 0 {
        println!("  - LZMA2: {} chunks", compression_stats.lzma2_count);
    }

    Ok(())
}
