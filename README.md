# SFTPX - QUIC-Based File Transfer

A high-performance file transfer system built with QUIC protocol using the `quiche` crate, featuring integrated orchestration, and BLAKE3 integrity verification.

## Features

✅ **Core Features:**
- QUIC-based transport with CUBIC congestion control
- Integrated file transfer orchestration (handshake → manifest → chunks)
- BLAKE3 integrity verification per chunk
- 4 QUIC streams (Control, Manifest, Data, Status)
- CLI tool with send/recv commands
- Cross-platform certificate generation

## Quick Start

### 1. Initialize Certificates

First-time setup - generate TLS certificates for QUIC:

```bash
# Linux/macOS/Windows - works everywhere, no external dependencies
sftpx init --ip 127.0.0.1

# Or specify your server's IP for remote connections
sftpx init --ip 192.168.1.100
```

This creates `certs/cert.pem` and `certs/key.pem` with Subject Alternative Names for localhost and your specified IP.

**No external dependencies required** - certificate generation is built into the binary using native Rust cryptography.

### 2. Start the Server

```bash
# Start receiver on default port (0.0.0.0:4443)
sftpx recv

# Or specify custom bind address and upload directory
sftpx recv --bind 192.168.1.100:4443 --upload-dir ./uploads
```

### 3. Send a File

```bash
# Send to localhost
sftpx send myfile.dat

# Send to remote server
sftpx send myfile.dat 192.168.1.100

```

## CLI Commands

### `sftpx init`
Initialize TLS certificates for QUIC connections.

```bash
sftpx init --ip <server_ip>
```

**Options:**
- `--ip <IP>` - Server IP address for certificate SAN (default: 127.0.0.1)

**Example:**
```bash
sftpx init --ip 192.168.1.100
```

### `sftpx send`

```bash
sftpx send <file> [server]
```

**Arguments:**
- `<file>` - File to send (required)
- `[server]` - Server IP address (default: 127.0.0.1)

**Features:**
- Automatically detects interrupted transfers
- Session ID based on file path (deterministic)
- Progress reporting during transfer

**Example:**
```bash
sftpx send large_file.bin 192.168.1.100
```

### `sftpx recv`
Start server to receive files.

```bash
sftpx recv [OPTIONS]
```

**Options:**
- `--bind <ADDRESS>` - Bind address (default: 0.0.0.0:4443)
- `--upload-dir <PATH>` - Upload directory (default: ./uploads)

**Example:**
```bash
sftpx recv --bind 192.168.1.100:4443 --upload-dir /var/uploads
```

## Features in Detail

### BLAKE3 Integrity

Every chunk is verified with BLAKE3:
- Computed during chunking
- Transmitted in manifest
- Verified on server before storage
- Automatic retransmission on corruption

### Migration Handling

Server detects peer address changes and handles gracefully:
- Closes old connection
- Resets socket to blocking mode  
- Loops back to accept new connection
- Client reconnects with same session ID

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
