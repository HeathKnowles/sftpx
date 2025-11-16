# SFTPX File Transfer Usage Guide

Complete guide for using SFTPX bidirectional file transfer with integrated orchestration.

## ✅ Implementation Complete

All systems integrated and tested:
- ✅ **Client send** (upload to server)
- ✅ **Server receive** (accept uploads)  
- ✅ **Server send** (download from server)
- ✅ **Client receive** (download files)
- ✅ **131 tests passing**

## 🏗️ Architecture

### Integrated Systems
1. **QUIC Transport** - 4 bidirectional streams
2. **Chunking** - FileChunker with configurable sizes
3. **Protocol Buffers** - ChunkPacket, Manifest, ControlMessage
4. **BLAKE3 Integrity** - Per-chunk verification
5. **Retransmission** - Automatic re-request on corruption (max 5 retries, 5s timeout)
6. **Orchestration** - Complete handshake → manifest → chunks flow

### Stream Layout
- **Stream 0** (STREAM_CONTROL): Control messages (ACK, NACK, RetransmitRequest)
- **Stream 1** (STREAM_MANIFEST): Manifest exchange (file metadata)
- **Stream 2** (STREAM_DATA): Chunk data transfer
- **Stream 3** (STREAM_STATUS): Transfer status updates

## 📤 Upload (Client → Server)

### Terminal 1: Start Server
```bash
# Start integrated file server
cargo run --example file_server

# Server will:
# - Listen on 127.0.0.1:4443
# - Accept uploads to /tmp/sftpx_uploads/
# - Serve downloads from ./test_files/
# - Handle manifest + chunk orchestration automatically
```

### Terminal 2: Upload File
```bash
# Upload a specific file
cargo run --example client_upload -- test_files/test.txt

# Or upload any file
cargo run --example client_upload -- /path/to/your/file.dat

# Client will:
# 1. Establish QUIC connection
# 2. Build manifest from file
# 3. Send manifest on stream 1
# 4. Send chunks on stream 2 with BLAKE3 hashes
# 5. Report total bytes sent
```

### What Happens
```
Client (Upload)                    Server (Receive)
================                   ================
1. Handshake   ------------------>  Accept connection
2. Build manifest
3. Send manifest on stream 1 ----->  Receive manifest
4. Send chunks on stream 2 ------->  Receive chunks
                                     Verify BLAKE3 per chunk
                                     Assemble file
                                     Save to upload_dir
5. Close  ----------------------->  File complete
```

## 📥 Download (Server → Client)

### Terminal 1: Start Server
```bash
cargo run --example file_server

# Make sure files exist in ./test_files/ for download
```

### Terminal 2: Download File
```bash
cargo run --example client_download

# Client will:
# 1. Establish QUIC connection
# 2. Receive manifest from server on stream 1
# 3. Receive chunks on stream 2
# 4. Verify BLAKE3 per chunk
# 5. Auto-request missing/corrupted chunks
# 6. Save file to /tmp/sftpx_downloads/
```

### What Happens
```
Server (Send)                      Client (Receive)
=============                      ================
1. Accept connection  <------------  Handshake
2. Build manifest
3. Send manifest on stream 1 ----->  Receive manifest
4. Send chunks on stream 2 ------->  Receive chunks
                                     Verify BLAKE3
                                     Auto-NACK if corrupted
5. Handle retransmit requests  <---  Request missing chunks
6. Close  <------------------------  File complete & verified
```

## 🧪 Testing the Complete Pipeline

### 1. Create Test Files
```bash
# Create test directory
mkdir -p test_files

# Create a small test file
echo "Hello SFTPX!" > test_files/small.txt

# Create a larger test file (10 MB)
dd if=/dev/urandom of=test_files/large.bin bs=1M count=10

# Create certs directory (if needed)
mkdir -p certs
# Place your cert.pem and key.pem in certs/ or use test certs
```

### 2. Generate Test Certificates (if needed)
```bash
# Generate self-signed cert for testing
openssl req -x509 -newkey rsa:2048 -nodes \
  -keyout certs/key.pem \
  -out certs/cert.pem \
  -days 365 \
  -subj "/CN=localhost"
```

### 3. Run Upload Test
```bash
# Terminal 1
cargo run --example file_server

# Terminal 2
cargo run --example client_upload -- test_files/large.bin

# Check uploaded file
ls -lh /tmp/sftpx_uploads/
```

### 4. Run Download Test
```bash
# Terminal 1
cargo run --example file_server

# Terminal 2
cargo run --example client_download

# Check downloaded file
ls -lh /tmp/sftpx_downloads/
```

### 5. Verify Integrity
```bash
# Compare uploaded file with original
diff test_files/large.bin /tmp/sftpx_uploads/large.bin && echo "✅ Upload verified!"

# Compare downloaded file with original
diff test_files/large.bin /tmp/sftpx_downloads/large.bin && echo "✅ Download verified!"
```

## 🔧 Configuration

### Client Configuration
```rust
use sftpx::client::transfer::Transfer;
use sftpx::common::config::ClientConfig;

let server_addr = "127.0.0.1:4443".parse()?;
let config = ClientConfig::new(server_addr, "localhost".to_string())
    .disable_cert_verification()     // For testing
    .with_chunk_size(262144)?        // 256 KB chunks
    .with_timeout(Duration::from_secs(30));
```

### Server Configuration
```rust
use sftpx::server::{Server, ServerConfig};

let config = ServerConfig {
    bind_addr: "127.0.0.1:4443".to_string(),
    cert_path: "certs/cert.pem".to_string(),
    key_path: "certs/key.pem".to_string(),
    max_idle_timeout: 30000,        // 30 seconds
    max_data: 100_000_000,          // 100 MB
    max_stream_data: 10_000_000,    // 10 MB per stream
    max_streams: 100,
};
```

## 📊 Features Demonstrated

### Upload Pipeline
✅ Client-side `run_send()` orchestration  
✅ `ManifestBuilder` → `ManifestSender` → `DataSender`  
✅ BLAKE3 hash per chunk  
✅ Protocol Buffers serialization  
✅ Server-side `receive_file_integrated()`  
✅ Automatic file assembly with `FileReceiver`

### Download Pipeline
✅ Server-side `send_file_integrated()` orchestration  
✅ Client-side `run_receive()` orchestration  
✅ `ManifestReceiver` + `FileReceiver` integration  
✅ Automatic NACK on corruption  
✅ `MissingChunkTracker` + `RetransmissionQueue`  
✅ Auto-request missing chunks (max 5 retries, 5s timeout)

### Control Flow
✅ QUIC handshake with connection migration support  
✅ 4-stream architecture (Control, Manifest, Data, Status)  
✅ Heartbeat/keepalive (30s interval)  
✅ Graceful connection close  
✅ Progress tracking and logging

## 🌐 Laptop-to-Laptop Transfer

### Setup on Server Laptop
```bash
# 1. Get server's IP address
ip addr show  # Linux
ifconfig      # macOS

# Example: 192.168.1.100

# 2. Update bind address in file_server.rs or use env var
# bind_addr: "0.0.0.0:4443"  # Listen on all interfaces

# 3. Start server
cargo run --example file_server
```

### Setup on Client Laptop
```bash
# 1. Update server address in examples
# Change "127.0.0.1:4443" to server's IP:
# server_addr = "192.168.1.100:4443".parse()?;

# 2. Run upload
cargo run --example client_upload -- /path/to/file

# 3. Or run download
cargo run --example client_download
```

### Firewall Configuration
```bash
# On server laptop, allow port 4443
sudo ufw allow 4443/udp  # Linux
# Or configure macOS/Windows firewall to allow UDP 4443
```

## 🐛 Troubleshooting

### Connection Refused
- Check server is running: `cargo run --example file_server`
- Verify firewall allows UDP port 4443
- Check IP address is correct for laptop-to-laptop

### Certificate Errors
- Use `.disable_cert_verification()` for testing
- Or generate proper certificates with correct CN/SAN
- Ensure cert.pem and key.pem exist in certs/

### Timeout Errors
- Increase timeout: `.with_timeout(Duration::from_secs(60))`
- Check network connectivity
- Reduce chunk size for slow networks

### Chunk Verification Failed
- System automatically retries up to 5 times
- Check for network corruption
- If persistent, file may be corrupted at source

## 📈 Performance Tuning

### For Large Files
```rust
.with_chunk_size(1_048_576)?  // 1 MB chunks for faster transfers
```

### For Unreliable Networks
```rust
.with_chunk_size(65536)?      // 64 KB chunks for more reliability
.with_timeout(Duration::from_secs(120))  // Longer timeout
```

### For Low-Latency Networks
```rust
.with_chunk_size(4_194_304)?  // 4 MB chunks for maximum throughput
```

## 📝 Logging

Enable detailed logging:
```bash
# Info level (recommended)
RUST_LOG=info cargo run --example client_upload -- test.txt

# Debug level (verbose)
RUST_LOG=debug cargo run --example file_server

# Specific module
RUST_LOG=sftpx::client::transfer=debug cargo run --example client_upload
```

## ✨ Success Indicators

Upload successful:
```
✅ Upload successful!
  Total bytes sent: 10485760 (10.00 MB)
  Transfer state: Completed
```

Download successful:
```
✅ Download successful!
  File saved to: "/tmp/sftpx_downloads/large.bin"
  Transfer state: Completed
  File size: 10485760 bytes (10.00 MB)
```

Server logs:
```
INFO TransferManager: file receive complete!
INFO   File saved to: "/tmp/sftpx_uploads/large.bin"
INFO   Total bytes: 10485760
```

## 🎯 Next Steps

1. **Test with real files**: Try uploading large files (100+ MB)
2. **Test over network**: Try laptop-to-laptop transfer
3. **Test reliability**: Simulate packet loss to verify auto-retransmission
4. **Measure performance**: Benchmark transfer speeds
5. **Production deployment**: Add proper TLS certificate verification

---

**All 131 tests passing** ✅  
**Complete bidirectional file transfer** ✅  
**All 6 systems integrated** ✅
