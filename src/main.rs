// Main entry point for the application

use clap::{Parser, Subcommand};
use sftpx::common::config::ClientConfig;
use sftpx::client::transfer::Transfer;
use sftpx::server::{Server, ServerConfig};
use sftpx::chunking::compress::CompressionType;
use sftpx::chunking::ChunkBitmap;
use std::path::{Path, PathBuf};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

#[derive(Parser)]
#[command(name = "sftpx")]
#[command(about = "QUIC-based file transfer tool with auto-resume", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Send a file to a remote server
    Send {
        /// File to send
        file: String,
        
        /// Server IP address (default: 127.0.0.1)
        server: Option<String>,
    },
    
    /// Start server to receive files
    Recv {
        /// Bind address (default: 0.0.0.0:4443)
        #[arg(long, default_value = "0.0.0.0:4443")]
        bind: String,
        
        /// Upload directory (default: ./uploads)
        #[arg(long, default_value = "./uploads")]
        upload_dir: String,
    },
}

fn get_session_id_for_file(file_path: &Path) -> String {
    // Generate deterministic session ID based on file path and name
    let file_name = file_path.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown");
    
    // Use blake3 hash of absolute path for deterministic session ID
    let abs_path = std::fs::canonicalize(file_path)
        .unwrap_or_else(|_| file_path.to_path_buf());
    let path_str = abs_path.to_string_lossy();
    let hash = blake3::hash(path_str.as_bytes());
    
    format!("upload_{}_{}", file_name, hex::encode(&hash.as_bytes()[..8]))
}

fn check_for_resume(session_id: &str) -> Option<u32> {
    let resume_dir = PathBuf::from("sftpx_resume");
    let bitmap_path = resume_dir.join(format!("{}.bitmap", session_id));
    
    if bitmap_path.exists() {
        if let Ok(bitmap) = ChunkBitmap::load_from_disk(&bitmap_path) {
            let received = bitmap.received_count();
            let total = bitmap.total_chunks().unwrap_or(0);
            if received > 0 {
                println!("\n📁 Found previous transfer:");
                println!("  Session ID: {}", session_id);
                println!("  Progress: {}/{} chunks ({:.1}%)", 
                    received, total, 
                    (received as f64 / total as f64) * 100.0);
                println!("  Will resume from chunk {}", received);
                return Some(received);
            }
        }
    }
    None
}

fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    
    let cli = Cli::parse();
    
    match cli.command {
        Commands::Send { file, server } => {
            println!("=== SFTPX Client Upload ===\n");
            
            let file_path = Path::new(&file);
            let server_ip = server.as_deref().unwrap_or("127.0.0.1");
            
            // Verify file exists
            if !file_path.exists() {
                eprintln!("Error: File not found: {:?}", file_path);
                return Ok(());
            }
            
            let file_size = std::fs::metadata(file_path)?.len();
            let session_id = get_session_id_for_file(file_path);
            
            println!("File to upload:");
            println!("  Path: {:?}", file_path);
            println!("  Size: {} bytes ({:.2} MB)", file_size, file_size as f64 / 1_048_576.0);
            println!("  Session ID: {}", session_id);
            
            // Check for existing transfer to resume
            let resume_from = check_for_resume(&session_id);
            
            // Create client configuration
            let server_addr = format!("{}:4443", server_ip).parse()?;
            let server_name = if server_ip == "127.0.0.1" || server_ip == "localhost" {
                "localhost".to_string()
            } else {
                server_ip.to_string()
            };
            
            let config = ClientConfig::new(server_addr, server_name)
                .disable_cert_verification()
                .with_chunk_size(2097152)?    // 2 MB chunks
                .with_compression(CompressionType::None);
            
            println!("\nClient Configuration:");
            println!("  Server: {}", server_addr);
            println!("  Chunk Size: {} MB", config.chunk_size / (1024*1024));
            println!("  Compression: {:?}", config.compression);
            println!("\nFeatures:");
            println!("  ✓ Integrated orchestration (handshake → manifest → chunks)");
            println!("  ✓ BLAKE3 integrity verification per chunk");
            println!("  ✓ Auto-resume capability (saves every 100 chunks)");
            println!("  ✓ 4 QUIC streams (Control, Manifest, Data, Status)");
            
            if resume_from.is_some() {
                println!("\n🔄 RESUMING interrupted transfer...\n");
            } else {
                println!("\n▶️  Starting new upload...\n");
            }
            
            // Create transfer and run upload
            let mut transfer = Transfer::send_file(config, file_path.to_str().unwrap(), "server")?;
            
            match transfer.run_send(file_path) {
                Ok(bytes_sent) => {
                    println!("\n✅ Upload successful!");
                    println!("  Total bytes sent: {} ({:.2} MB)", bytes_sent, bytes_sent as f64 / 1_048_576.0);
                    println!("  Transfer state: {:?}", transfer.state());
                }
                Err(e) => {
                    eprintln!("\n❌ Upload failed: {:?}", e);
                    return Err(e.into());
                }
            }
        }
        
        Commands::Recv { bind, upload_dir } => {
            println!("=== SFTPX File Server ===\n");
            
            // Create server configuration
            let config = ServerConfig {
                bind_addr: bind.clone(),
                cert_path: "certs/cert.pem".to_string(),
                key_path: "certs/key.pem".to_string(),
                max_idle_timeout: 30000,
                max_data: 100_000_000,
                max_stream_data: 10_000_000,
                max_streams: 100,
            };
            
            // Set up directories
            let upload_path = PathBuf::from(&upload_dir);
            std::fs::create_dir_all(&upload_path)?;
            
            println!("Server Configuration:");
            println!("  Address: {}", config.bind_addr);
            println!("  Certificate: {}", config.cert_path);
            println!("  Private Key: {}", config.key_path);
            println!("  Upload Directory: {:?}", upload_path);
            println!("  Max Data: {} MB", config.max_data / 1_048_576);
            println!("  Max Idle Timeout: {}ms", config.max_idle_timeout);
            
            println!("\nFeatures:");
            println!("  ✓ Integrated orchestration (manifest + chunks)");
            println!("  ✓ BLAKE3 integrity verification");
            println!("  ✓ Automatic retransmission on corruption");
            println!("  ✓ 4 QUIC streams (Control, Manifest, Data, Status)");
            
            println!("\nStarting QUIC file server...");
            let mut server = Server::new(config)?;
            
            println!("✓ Server initialized successfully");
            println!("✓ Listening for connections...");
            println!("Ready to accept uploads!");
            println!("Press Ctrl+C to stop\n");
            
            server.run()?;
        }
    }
    
    Ok(())
}
