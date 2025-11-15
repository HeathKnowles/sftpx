# SFTPX - QUIC-Based File Transfer with 4-Stream Architecture

A high-performance file transfer system built with QUIC protocol using the `quiche` crate, featuring a 4-stream architecture for efficient data transfer.

## Features

✅ **Implemented:**
- 4-stream QUIC architecture (Control, Data1, Data2, Data3)
- Complete QUIC handshake and connection management
- Stream priority management
- Session persistence and resumption
- Chunked file transfer
- Comprehensive error handling
- Modular architecture

🔄 **In Progress:**
- Server implementation
- Protocol message definitions (Protobuf/FlatBuffers)
- Multi-threaded stream handlers
- Chunk validation and integrity checks

## Architecture

### 4-Stream Design

The client establishes 4 bidirectional QUIC streams:

1. **STREAM_CONTROL (ID: 0)** - Control messages, metadata, commands
   - Priority: Highest (urgency=0, non-incremental)
   
2. **STREAM_DATA1 (ID: 4)** - Primary data stream
   - Priority: Medium (urgency=3, incremental)
   
3. **STREAM_DATA2 (ID: 8)** - Secondary data stream
   - Priority: Medium (urgency=3, incremental)
   
4. **STREAM_DATA3 (ID: 12)** - Tertiary data stream
   - Priority: Medium (urgency=3, incremental)

### Module Structure

```
src/
├── common/           # Shared types and utilities
│   ├── error.rs     # Error types (16 variants)
│   ├── types.rs     # Enums, constants, type aliases
│   ├── config.rs    # ClientConfig, ServerConfig
│   └── utils.rs     # Helper functions
├── client/          # Client implementation
│   ├── mod.rs       # Public API facade
│   ├── connection.rs # QUIC connection wrapper
│   ├── streams.rs   # 4-stream manager
│   ├── session.rs   # Session tracking & persistence
│   ├── receiver.rs  # File receiving logic
│   └── transfer.rs  # Main transfer event loop
└── main.rs          # CLI application
```

## Client Implementation

### Key Components

**ClientConnection** - Wraps quiche::Connection with TLS config, stats tracking, stream helpers

**StreamManager** - Manages 4 streams with priorities, send/recv wrappers, state monitoring

**Transfer** - Main event loop: handshake → stream init → data transfer → shutdown

**ClientSession** - Persistent state with chunk bitmaps, progress tracking, JSON serialization

## Usage Example

```rust
use sftpx::common::{ClientConfig, Result};
use sftpx::client::Transfer;

fn main() -> Result<()> {
    env_logger::init();
    
    let server_addr = "127.0.0.1:4443".parse().unwrap();
    let config = ClientConfig::new(server_addr, "localhost".to_string())
        .with_chunk_size(1024 * 1024)?
        .disable_cert_verification();
    
    let mut transfer = Transfer::send_file(config, "myfile.dat", "output/")?;
    transfer.run()?;
    
    println!("Progress: {:.2}%", transfer.progress());
    Ok(())
}
```

See `examples/simple_client.rs` for complete example.

## Building

```bash
cargo check              # Check compilation
cargo build --release    # Build release
cargo run --example simple_client
cargo run -- send 127.0.0.1:4443 /path/to/file
```

## Dependencies

- **quiche 0.24.6** - QUIC protocol
- **ring 0.17** - Cryptography
- **serde + serde_json** - Serialization
- **clap 4.5** - CLI
- **log + env_logger** - Logging
- **cmake** (system dependency)

## Configuration

```rust
pub struct ClientConfig {
    pub server_addr: SocketAddr,
    pub server_name: String,
    pub chunk_size: usize,           // Default: 1MB
    pub timeout: Duration,
    pub session_dir: PathBuf,
    pub verify_cert: bool,
    pub ca_cert_path: Option<PathBuf>,
}
```

## Status

✅ Client fully implemented with 4-stream QUIC
🔄 Server implementation in progress
📋 Protocol schemas planned

## License

MIT
