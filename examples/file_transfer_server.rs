// QUIC File Transfer Server
// Receives files from clients using the chunking pipeline

use sftpx::chunking::{ChunkTable, ChunkBitmap, ChunkHasher, ChunkCompressor, ChunkMetadata, CompressionAlgorithm};
use std::collections::HashMap;
use std::net::UdpSocket;
use std::sync::{Arc, Mutex};
use std::fs::File;
use std::io::{Write, Seek, SeekFrom};
use std::path::PathBuf;

const MAX_DATAGRAM_SIZE: usize = 1350;
const CHUNK_DATA_SIZE: usize = 8192; // 8KB chunks

/// File transfer session
struct TransferSession {
    file_path: PathBuf,
    file_size: u64,
    total_chunks: u64,
    table: ChunkTable,
    bitmap: ChunkBitmap,
    compression_algorithm: CompressionAlgorithm,
    file: Option<File>,
}

impl TransferSession {
    fn new(file_path: PathBuf, file_size: u64, total_chunks: u64, compression_algorithm: CompressionAlgorithm) -> Self {
        let table = ChunkTable::with_capacity(total_chunks as usize);
        let bitmap = ChunkBitmap::with_exact_size(total_chunks as u32);
        
        Self {
            file_path,
            file_size,
            total_chunks,
            table,
            bitmap,
            compression_algorithm,
            file: None,
        }
    }
    
    fn receive_chunk(&mut self, chunk_number: u64, compressed_data: Vec<u8>, hash: Vec<u8>, is_eof: bool) -> Result<(), Box<dyn std::error::Error>> {
        // Verify hash
        if !ChunkHasher::verify(&compressed_data, &hash) {
            return Err(format!("Chunk {} hash verification failed", chunk_number).into());
        }
        
        // Decompress the chunk
        let decompressed = ChunkCompressor::decompress(&compressed_data, self.compression_algorithm, Some(CHUNK_DATA_SIZE))?;
        
        // Calculate metadata
        let byte_offset = chunk_number * CHUNK_DATA_SIZE as u64;
        let chunk_length = decompressed.len() as u32;
        
        // Store in table
        let metadata = ChunkMetadata::new(
            chunk_number,
            byte_offset,
            chunk_length,
            hash,
            is_eof,
        );
        self.table.insert(metadata);
        
        // Mark in bitmap
        self.bitmap.mark_received(chunk_number as u32, is_eof);
        
        // Write to file at the correct position
        if self.file.is_none() {
            std::fs::create_dir_all(self.file_path.parent().unwrap())?;
            self.file = Some(std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(false)
                .open(&self.file_path)?);
        }
        
        if let Some(ref mut file) = self.file {
            // Seek to correct position
            file.seek(SeekFrom::Start(byte_offset))?;
            file.write_all(&decompressed)?;
            file.sync_all()?;
        }
        
        println!("✓ Chunk {}/{}: {} bytes received, progress: {:.1}%", 
            chunk_number + 1, 
            self.total_chunks,
            decompressed.len(),
            self.bitmap.progress()
        );
        
        Ok(())
    }
    
    fn is_complete(&self) -> bool {
        self.bitmap.is_complete()
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let bind_addr = if args.len() > 1 {
        args[1].clone()
    } else {
        "0.0.0.0:4443".to_string()
    };

    println!("=== QUIC File Transfer Server ===\n");
    println!("Listening on: {}", bind_addr);
    println!("Waiting for file transfers...\n");

    let socket = UdpSocket::bind(&bind_addr)?;
    let sessions: Arc<Mutex<HashMap<String, TransferSession>>> = Arc::new(Mutex::new(HashMap::new()));
    
    let mut buf = [0u8; MAX_DATAGRAM_SIZE];
    
    loop {
        let (len, from) = socket.recv_from(&mut buf)?;
        let data = &buf[..len];
        
        // Parse packet: [type:1][session_id:32][payload]
        if len < 33 {
            eprintln!("Packet too short");
            continue;
        }
        
        let packet_type = data[0];
        let session_id = String::from_utf8_lossy(&data[1..33]).to_string();
        let payload = &data[33..];
        
        match packet_type {
            0x01 => {
                // START packet: [file_size:8][total_chunks:8][compression_algo:1][filename_len:2][filename]
                if payload.len() < 19 {
                    eprintln!("Invalid START packet");
                    continue;
                }
                
                let file_size = u64::from_be_bytes(payload[0..8].try_into()?);
                let total_chunks = u64::from_be_bytes(payload[8..16].try_into()?);
                let compression_algo = match payload[16] {
                    0 => CompressionAlgorithm::None,
                    1 => CompressionAlgorithm::Lz4,
                    2 => CompressionAlgorithm::Lz4Hc(9),
                    3 => CompressionAlgorithm::Zstd(5),
                    4 => CompressionAlgorithm::Lzma2(6),
                    _ => CompressionAlgorithm::None,
                };
                
                let filename_len = u16::from_be_bytes(payload[17..19].try_into()?) as usize;
                let filename = String::from_utf8_lossy(&payload[19..19+filename_len]).to_string();
                
                let file_path = PathBuf::from(format!("received/{}", filename));
                
                println!("\n📁 New transfer started:");
                println!("   Session: {}", &session_id[..8]);
                println!("   File: {}", filename);
                println!("   Size: {} bytes", file_size);
                println!("   Chunks: {}", total_chunks);
                println!("   Compression: {:?}\n", compression_algo);
                
                let session = TransferSession::new(file_path, file_size, total_chunks, compression_algo);
                sessions.lock().unwrap().insert(session_id.clone(), session);
                
                // Send ACK
                let mut ack = vec![0x02]; // ACK type
                ack.extend_from_slice(session_id.as_bytes());
                socket.send_to(&ack, from)?;
            }
            
            0x03 => {
                // CHUNK packet: [chunk_number:8][hash_len:2][hash][is_eof:1][compressed_data]
                if payload.len() < 11 {
                    eprintln!("Invalid CHUNK packet");
                    continue;
                }
                
                let chunk_number = u64::from_be_bytes(payload[0..8].try_into()?);
                let hash_len = u16::from_be_bytes(payload[8..10].try_into()?) as usize;
                let hash = payload[10..10+hash_len].to_vec();
                let is_eof = payload[10+hash_len] == 1;
                let compressed_data = payload[10+hash_len+1..].to_vec();
                
                if let Some(session) = sessions.lock().unwrap().get_mut(&session_id) {
                    match session.receive_chunk(chunk_number, compressed_data, hash, is_eof) {
                        Ok(_) => {
                            // Send ACK for this chunk
                            let mut ack = vec![0x04]; // CHUNK_ACK type
                            ack.extend_from_slice(session_id.as_bytes());
                            ack.extend_from_slice(&chunk_number.to_be_bytes());
                            socket.send_to(&ack, from)?;
                            
                            if session.is_complete() {
                                println!("\n✅ Transfer complete!");
                                println!("   File saved to: {:?}", session.file_path);
                                println!("   Total size: {} bytes\n", session.file_size);
                                
                                // Send COMPLETE
                                let mut complete = vec![0x05];
                                complete.extend_from_slice(session_id.as_bytes());
                                socket.send_to(&complete, from)?;
                            }
                        }
                        Err(e) => {
                            eprintln!("Error receiving chunk {}: {}", chunk_number, e);
                        }
                    }
                }
            }
            
            _ => {
                eprintln!("Unknown packet type: {}", packet_type);
            }
        }
    }
}
