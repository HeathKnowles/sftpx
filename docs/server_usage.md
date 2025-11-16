# QUIC Server Usage Guide

## Overview

This guide demonstrates how to import and use the QUIC server implementation in your Rust application.

## Table of Contents

1. [Basic Setup](#basic-setup)
2. [Importing the Server](#importing-the-server)
3. [Basic Usage](#basic-usage)
4. [Advanced Usage](#advanced-usage)
5. [Stream Management](#stream-management)
6. [Data Sending](#data-sending)
7. [File Transfer](#file-transfer)
8. [Complete Examples](#complete-examples)

---

## Basic Setup

### Prerequisites

Ensure you have the required dependencies in your `Cargo.toml`:

```toml
[dependencies]
quiche = "0.20"
```

### Certificate Setup

Place your TLS certificates in the `certs/` folder at the project root:

```
project_root/
├── certs/
│   ├── cert.pem    # Server certificate
│   └── key.pem     # Private key
├── src/
└── Cargo.toml
```

You can generate self-signed certificates for testing:

```bash
cd certs
openssl req -x509 -newkey rsa:2048 -nodes -keyout key.pem -out cert.pem -subj "/CN=localhost"
```

---

## Importing the Server

### Import the Server Module

```rust
// Import the entire server module
use sftpx::server;

// Or import specific components
use sftpx::server::{
    Server,
    ServerConfig,
    ServerConnection,
    ServerSession,
    StreamManager,
    StreamType,
    DataSender,
    TransferManager,
};
```

---

## Basic Usage

### Example 1: Simple Server with Default Configuration

```rust
use sftpx::server::{Server, ServerConfig};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create server with default configuration
    let config = ServerConfig::default();
    let mut server = Server::new(config)?;
    
    // Run the server (blocks until connection is handled)
    server.run()?;
    
    Ok(())
}
```

### Example 2: Custom Server Configuration

```rust
use sftpx::server::{Server, ServerConfig};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create custom configuration
    let config = ServerConfig {
        bind_addr: "0.0.0.0:8443".to_string(),
        cert_path: "certs/cert.pem".to_string(),
        key_path: "certs/key.pem".to_string(),
        max_idle_timeout: 10000,  // 10 seconds
        max_data: 50_000_000,     // 50 MB
        max_stream_data: 5_000_000, // 5 MB per stream
        max_streams: 200,
    };
    
    let mut server = Server::new(config)?;
    server.run()?;
    
    Ok(())
}
```

---

## Advanced Usage

### Example 3: Manual Connection Handling

```rust
use sftpx::server::{ServerConnection, ServerSession};
use quiche::{Config, ConnectionId};
use std::net::UdpSocket;

fn handle_connection() -> Result<(), Box<dyn std::error::Error>> {
    let socket = UdpSocket::bind("127.0.0.1:4443")?;
    let mut config = Config::new(quiche::PROTOCOL_VERSION)?;
    
    // Configure QUIC...
    config.set_application_protos(&[b"hq-29"])?;
    config.verify_peer(false);
    config.load_cert_chain_from_pem_file("certs/cert.pem")?;
    config.load_priv_key_from_pem_file("certs/key.pem")?;
    
    let mut buf = [0u8; 1350];
    let mut out = [0u8; 1350];
    
    // Accept connection
    let (len, from) = socket.recv_from(&mut buf)?;
    let hdr = quiche::Header::from_slice(
        &mut buf[..len], 
        quiche::MAX_CONN_ID_LEN
    )?;
    
    let scid = ConnectionId::from_ref(&hdr.dcid);
    let mut conn = ServerConnection::accept(
        &scid,
        socket.local_addr()?,
        from,
        &mut config,
    )?;
    
    // Process initial packet
    conn.process_packet(&buf[..len], from, socket.local_addr()?)?;
    conn.send_packets(&socket, &mut out)?;
    
    // Create and run session
    let mut session = ServerSession::new(&mut conn);
    session.run(&socket, &mut buf, &mut out)?;
    
    Ok(())
}
```

---

## Stream Management

### Example 4: Working with Multiple Streams

The server automatically creates **4 streams per connection**:
- **Control Stream** (ID: 0) - For control messages
- **Data Stream 1** (ID: 4) - For data transfer
- **Data Stream 2** (ID: 8) - For data transfer
- **Data Stream 3** (ID: 12) - For data transfer

```rust
use sftpx::server::{StreamManager, StreamType};

fn manage_streams() {
    let mut stream_manager = StreamManager::new();
    
    // Get all stream types
    let all_streams = StreamType::all();
    for stream_type in all_streams {
        println!("Stream: {:?}, ID: {}", stream_type, stream_type.stream_id());
    }
    
    // After initialization (done automatically in ServerSession)
    // Access stream information
    let control_stream = stream_manager.get_stream(0);
    if let Some(info) = control_stream {
        println!("Control stream - Sent: {} bytes, Received: {} bytes",
                 info.bytes_sent, info.bytes_received);
    }
    
    // Get statistics
    let stats = stream_manager.get_statistics();
    println!("Total streams: {}", stats.total_streams);
    println!("Active streams: {}", stats.active_streams);
    println!("Total sent: {} bytes", stats.total_bytes_sent);
    println!("Total received: {} bytes", stats.total_bytes_received);
}
```

---

## Data Sending

### Example 5: Send Data to a Client

```rust
use sftpx::server::{ServerConnection, DataSender};

fn send_data_example(connection: &mut ServerConnection) -> Result<(), Box<dyn std::error::Error>> {
    let mut sender = DataSender::new();
    let stream_id = 0; // Control stream
    
    // Simple send
    let data = b"Hello, client!";
    let bytes_sent = sender.send_data(connection, stream_id, data, false)?;
    println!("Sent {} bytes", bytes_sent);
    
    // Send with FIN flag (close stream)
    sender.send_data(connection, stream_id, b"Goodbye!", true)?;
    
    Ok(())
}
```

### Example 6: Send Large Data in Chunks

```rust
use sftpx::server::{ServerConnection, DataSender};

fn send_chunked_example(connection: &mut ServerConnection) -> Result<(), Box<dyn std::error::Error>> {
    let mut sender = DataSender::new();
    let stream_id = 4; // Data stream 1
    
    // Prepare large data
    let large_data = vec![0u8; 100_000]; // 100 KB
    
    // Send in 8 KB chunks
    let chunk_size = 8192;
    let total_sent = sender.send_chunked(
        connection,
        stream_id,
        &large_data,
        chunk_size,
        true, // Send FIN after last chunk
    )?;
    
    println!("Sent {} bytes in chunks of {}", total_sent, chunk_size);
    
    Ok(())
}
```

### Example 7: Distribute Data Across Multiple Streams

```rust
use sftpx::server::{ServerConnection, DataSender};

fn send_distributed_example(connection: &mut ServerConnection) -> Result<(), Box<dyn std::error::Error>> {
    let mut sender = DataSender::new();
    
    // Use all data streams (round-robin distribution)
    let stream_ids = vec![4, 8, 12]; // Data streams 1, 2, 3
    
    let data = vec![0u8; 50_000]; // 50 KB
    let chunk_size = 4096; // 4 KB chunks
    
    // Data will be distributed: chunk 0 -> stream 4, chunk 1 -> stream 8, 
    // chunk 2 -> stream 12, chunk 3 -> stream 4, etc.
    let total_sent = sender.send_distributed(
        connection,
        &stream_ids,
        &data,
        chunk_size,
    )?;
    
    println!("Distributed {} bytes across {} streams", total_sent, stream_ids.len());
    
    Ok(())
}
```

---

## File Transfer

### Example 8: Transfer a File on a Single Stream

```rust
use sftpx::server::{ServerConnection, TransferManager};
use std::path::Path;

fn transfer_file_example(connection: &mut ServerConnection) -> Result<(), Box<dyn std::error::Error>> {
    let mut transfer_manager = TransferManager::new();
    
    let file_path = Path::new("data/example.bin");
    let stream_id = 4; // Data stream 1
    
    let bytes_transferred = transfer_manager.transfer_file(
        connection,
        stream_id,
        file_path,
    )?;
    
    println!("Transferred {} bytes from {:?}", bytes_transferred, file_path);
    
    Ok(())
}
```

### Example 9: Transfer a File Using All Streams (Parallel)

```rust
use sftpx::server::{ServerConnection, StreamManager, TransferManager};
use std::path::Path;

fn transfer_file_multistream(
    connection: &mut ServerConnection,
    stream_manager: &StreamManager,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut transfer_manager = TransferManager::with_chunk_size(16384); // 16 KB chunks
    
    let file_path = Path::new("data/large_file.bin");
    
    // Transfer using all active streams
    let bytes_transferred = transfer_manager.transfer_file_multistream(
        connection,
        stream_manager,
        file_path,
    )?;
    
    println!("Transferred {} bytes using multiple streams", bytes_transferred);
    println!("Total sent so far: {} bytes", transfer_manager.total_bytes_sent());
    
    Ok(())
}
```

### Example 10: Custom Chunk Size for Transfer

```rust
use sftpx::server::TransferManager;

fn custom_transfer_manager() {
    // Create with custom chunk size
    let mut manager = TransferManager::with_chunk_size(32768); // 32 KB
    
    println!("Chunk size: {} bytes", manager.chunk_size());
    
    // Change chunk size dynamically
    manager.set_chunk_size(65536); // 64 KB
    
    println!("New chunk size: {} bytes", manager.chunk_size());
}
```

---

## Complete Examples

### Example 11: Complete Server Application

```rust
use sftpx::server::{Server, ServerConfig};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Starting QUIC server...");
    
    // Configure server
    let config = ServerConfig {
        bind_addr: "0.0.0.0:4443".to_string(),
        cert_path: "certs/cert.pem".to_string(),
        key_path: "certs/key.pem".to_string(),
        max_idle_timeout: 5000,
        max_data: 10_000_000,
        max_stream_data: 1_000_000,
        max_streams: 100,
    };
    
    // Create and run server
    let mut server = Server::new(config)?;
    
    println!("Server ready and listening...");
    server.run()?;
    
    println!("Server shutting down.");
    Ok(())
}
```

### Example 12: Custom Session Handler with File Transfer

```rust
use sftpx::server::{
    ServerConnection, ServerSession, StreamManager, 
    TransferManager, DataSender
};
use std::net::UdpSocket;
use std::path::Path;

fn custom_session_handler(
    connection: &mut ServerConnection,
    socket: &UdpSocket,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut buf = [0u8; 1350];
    let mut out = [0u8; 1350];
    
    // Create session
    let mut session = ServerSession::new(connection);
    
    // Note: session.run() handles stream initialization automatically
    // If you need custom handling, you can access components:
    
    // Initialize streams manually (if not using session.run)
    let mut stream_manager = StreamManager::new();
    stream_manager.initialize_streams(connection)?;
    
    // Create transfer manager
    let mut transfer_manager = TransferManager::new();
    
    // Transfer a file across all streams
    let file_path = Path::new("data/transfer.dat");
    transfer_manager.transfer_file_multistream(
        connection,
        &stream_manager,
        file_path,
    )?;
    
    // Send confirmation message
    let mut sender = DataSender::new();
    sender.send_data(
        connection,
        0, // Control stream
        b"Transfer complete",
        true,
    )?;
    
    // Flush packets
    connection.send_packets(socket, &mut out)?;
    
    println!("Session completed successfully");
    Ok(())
}
```

### Example 13: Multi-Connection Server Loop

```rust
use sftpx::server::{ServerConnection, ServerSession};
use quiche::{Config, ConnectionId};
use std::net::UdpSocket;
use std::collections::HashMap;

fn multi_connection_server() -> Result<(), Box<dyn std::error::Error>> {
    let socket = UdpSocket::bind("0.0.0.0:4443")?;
    let mut config = Config::new(quiche::PROTOCOL_VERSION)?;
    
    // Configure QUIC
    config.set_application_protos(&[b"hq-29"])?;
    config.verify_peer(false);
    config.load_cert_chain_from_pem_file("certs/cert.pem")?;
    config.load_priv_key_from_pem_file("certs/key.pem")?;
    config.set_max_idle_timeout(5000);
    config.set_max_recv_udp_payload_size(1350);
    config.set_max_send_udp_payload_size(1350);
    config.set_initial_max_data(10_000_000);
    config.set_initial_max_stream_data_bidi_local(1_000_000);
    config.set_initial_max_stream_data_bidi_remote(1_000_000);
    config.set_initial_max_streams_bidi(100);
    
    let mut connections: HashMap<Vec<u8>, ServerConnection> = HashMap::new();
    let mut buf = [0u8; 1350];
    let mut out = [0u8; 1350];
    
    println!("Server listening on 0.0.0.0:4443");
    
    loop {
        let (len, from) = socket.recv_from(&mut buf)?;
        
        let hdr = match quiche::Header::from_slice(
            &mut buf[..len],
            quiche::MAX_CONN_ID_LEN
        ) {
            Ok(h) => h,
            Err(_) => continue,
        };
        
        let conn_id = hdr.dcid.to_vec();
        
        // Check if this is an existing connection
        if let Some(conn) = connections.get_mut(&conn_id) {
            conn.process_packet(&buf[..len], from, socket.local_addr()?)?;
            conn.send_packets(&socket, &mut out)?;
        } else {
            // New connection
            let scid = ConnectionId::from_ref(&hdr.dcid);
            let mut conn = ServerConnection::accept(
                &scid,
                socket.local_addr()?,
                from,
                &mut config,
            )?;
            
            conn.process_packet(&buf[..len], from, socket.local_addr()?)?;
            conn.send_packets(&socket, &mut out)?;
            
            connections.insert(conn_id, conn);
        }
        
        // Handle established connections
        let mut to_remove = Vec::new();
        for (id, conn) in connections.iter_mut() {
            if conn.is_established() {
                println!("Connection established: handling session");
                let mut session = ServerSession::new(conn);
                if let Err(e) = session.run(&socket, &mut buf, &mut out) {
                    eprintln!("Session error: {:?}", e);
                }
                to_remove.push(id.clone());
            }
        }
        
        // Remove completed connections
        for id in to_remove {
            connections.remove(&id);
        }
    }
}
```

---

## API Quick Reference

### Server Components

| Component | Purpose |
|-----------|---------|
| `Server` | Main server instance, handles lifecycle |
| `ServerConfig` | Configuration for the server |
| `ServerConnection` | Wrapper around QUIC connection |
| `ServerSession` | Manages a complete client session |
| `StreamManager` | Manages the 4 streams per connection |
| `DataSender` | Sends data to clients |
| `TransferManager` | Handles file transfers |

### Stream IDs

| Stream Type | ID | Purpose |
|------------|-----|---------|
| Control | 0 | Control messages |
| Data1 | 4 | Data transfer |
| Data2 | 8 | Data transfer |
| Data3 | 12 | Data transfer |

### Key Methods

#### DataSender
- `send_data(connection, stream_id, data, fin)` - Send data on a stream
- `send_chunked(connection, stream_id, data, chunk_size, fin)` - Send in chunks
- `send_distributed(connection, stream_ids, data, chunk_size)` - Distribute across streams

#### TransferManager
- `transfer_file(connection, stream_id, path)` - Transfer file on one stream
- `transfer_file_multistream(connection, stream_manager, path)` - Transfer across all streams
- `transfer_data_multistream(connection, stream_manager, data)` - Transfer data across all streams

---

## Error Handling

All methods return `Result<T, Box<dyn std::error::Error>>` for comprehensive error handling:

```rust
use sftpx::server::{Server, ServerConfig};

fn main() {
    let config = ServerConfig::default();
    
    match Server::new(config) {
        Ok(mut server) => {
            if let Err(e) = server.run() {
                eprintln!("Server error: {:?}", e);
                std::process::exit(1);
            }
        }
        Err(e) => {
            eprintln!("Failed to create server: {:?}", e);
            std::process::exit(1);
        }
    }
}
```

---

## Notes

1. **Certificates**: Always use valid TLS certificates. Self-signed certificates work for testing but should not be used in production.

2. **4 Streams**: The server automatically creates 4 bidirectional streams per connection. You can use them independently or distribute data across them for parallel transfer.

3. **Thread Safety**: The current implementation handles one connection at a time. For production use, consider wrapping connections in `Arc<Mutex<>>` for concurrent handling.

4. **Buffer Sizes**: Default buffer size is 1350 bytes (MAX_DATAGRAM_SIZE). Adjust based on your network MTU.

5. **Timeouts**: Default session timeout is 10 seconds. Configure via `ServerConfig.max_idle_timeout`.

---

## Additional Resources

- [QUIC Protocol Specification](https://www.rfc-editor.org/rfc/rfc9000.html)
- [quiche Documentation](https://docs.rs/quiche/)
- Project-specific docs in `docs/` directory
