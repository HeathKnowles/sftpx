// Main entry point for the application

use clap::{Parser, Subcommand};
use sftpx::{Client, ClientConfig, Result};
use sftpx::server::{Server, ServerConfig};


#[derive(Parser)]
#[command(name = "sftpx")]
#[command(about = "QUIC-based file transfer tool", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Send a file to a remote server
    Send {
        /// File to send
        #[arg(short, long)]
        file: String,
        
        /// Destination path on server
        #[arg(short, long)]
        destination: String,
        
        /// Server address (e.g., 127.0.0.1:4433)
        #[arg(short, long)]
        server: String,
    },
    
    /// Receive a file from a server
    Recv {
        /// Session ID to resume
        #[arg(short, long)]
        session_id: String,
        
        /// Server address (e.g., 127.0.0.1:4433)
        #[arg(short, long)]
        server: String,
    },
    
    /// Resume a transfer
    Resume {
        /// Session ID to resume
        #[arg(short, long)]
        session_id: String,
        
        /// Server address (e.g., 127.0.0.1:4433)
        #[arg(short, long)]
        server: String,
    },
}

fn main() -> Result<()> {
    env_logger::init();
    
    let cli = Cli::parse();
    
    match cli.command {
        Commands::Send { file, destination, server } => {
            println!("Sending file: {} to {}", file, destination);
            let server_addr = server.parse()
                .map_err(|_| sftpx::Error::ConfigError("Invalid server address".to_string()))?;
            
            let config = ClientConfig::new(server_addr, "localhost".to_string());
            let client = Client::new(config);
            
            let mut transfer = client.send_file(&file, &destination)?;
            transfer.run()?;
            
            println!("Transfer complete!");
        }
        
        Commands::Recv { session_id, server } => {
            println!("Receiving file with session: {}", session_id);
            let server_addr = server.parse()
                .map_err(|_| sftpx::Error::ConfigError("Invalid server address".to_string()))?;
            
            let config = ClientConfig::new(server_addr, "localhost".to_string());
            let client = Client::new(config);
            
            let mut transfer = client.receive_file(&session_id)?;
            transfer.run()?;
            
            println!("Transfer complete!");
        }
        
        Commands::Resume { session_id, server } => {
            println!("Resuming transfer: {}", session_id);
            let server_addr = server.parse()
                .map_err(|_| sftpx::Error::ConfigError("Invalid server address".to_string()))?;
            
            let config = ClientConfig::new(server_addr, "localhost".to_string());
            let client = Client::new(config);
            
            let mut transfer = client.resume_transfer(&session_id)?;
            transfer.run()?;
            
            println!("Transfer resumed and completed!");
        }
    }
    
    Ok(())
}
