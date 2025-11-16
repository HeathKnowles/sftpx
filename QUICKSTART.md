# SFTPX Quick Start Guide

## Installation

```bash
# Clone the repository
git clone <repository-url>
cd sftpx

# Build the release binary
cargo build --release

# The binary is located at: ./target/release/sftpx
```

## First-Time Setup

### 1. Generate Certificates

SFTPX uses QUIC which requires TLS certificates. Generate them with:

```bash
# For localhost development
./target/release/sftpx init

# For remote server (replace with your server IP)
./target/release/sftpx init --ip 192.168.1.100
```

**What this does:**
- Creates `certs/` directory
- Generates `cert.pem` (self-signed certificate, valid 365 days)
- Generates `key.pem` (ECDSA private key)
- Includes SANs for localhost + your IP

**No external dependencies required** - uses native Rust cryptography (rcgen crate).

## Basic Usage

### Scenario 1: Local File Transfer (Same Machine)

**Terminal 1 - Start receiver:**
```bash
./target/release/sftpx recv
```

**Terminal 2 - Send file:**
```bash
./target/release/sftpx send myfile.dat
```

### Scenario 2: Remote File Transfer

**On Server (192.168.1.100):**
```bash
# Generate certs with server IP
./target/release/sftpx init --ip 192.168.1.100

# Start receiver
./target/release/sftpx recv --bind 192.168.1.100:4443
```

**On Client:**
```bash
# Copy server's cert.pem to local certs/ directory (or use --disable-cert-verification)
# Send file to server
./target/release/sftpx send myfile.dat 192.168.1.100
```

### Scenario 3: Resume Interrupted Transfer

**Start transfer:**
```bash
./target/release/sftpx send large_file.bin 192.168.1.100
```

**Interrupt with Ctrl+C** - Transfer state is saved

**Resume automatically:**
```bash
./target/release/sftpx send large_file.bin 192.168.1.100
```

The client automatically detects the previous session and resumes from where it left off.

## Advanced Options

### Custom Server Port

**Server:**
```bash
./target/release/sftpx recv --bind 0.0.0.0:8443
```

**Client:**
```bash
# Specify port after IP
./target/release/sftpx send myfile.dat 192.168.1.100:8443
```

### Custom Upload Directory

```bash
./target/release/sftpx recv --upload-dir /var/uploads
```

### View Resume State

Resume bitmaps are stored in `sftpx_resume/` directory:
```bash
ls -lh sftpx_resume/
# Shows: upload_<filename>_<hash>.bitmap files
```

## Configuration

### Chunk Size
Default: 2 MB (hardcoded in `main.rs`)

To change, edit `src/main.rs`:
```rust
let config = ClientConfig::new(server_addr, server_name)
    .with_chunk_size(4 * 1024 * 1024)?  // 4 MB chunks
```

### Compression
Default: None

To enable, edit `src/main.rs`:
```rust
.with_compression(CompressionType::Zstd)  // or Gzip, Lz4
```

### Connection Limits
Default: 10GB connection window, 1GB per stream

To change, edit `src/client/connection.rs` and `src/server/connection.rs`

## Troubleshooting

### "Certificate verification failed"
- Ensure certificates are generated with correct IP
- For testing, client uses `disable_cert_verification()` by default

### "Connection timeout"
- Check firewall allows UDP port 4443
- Verify server is running and listening
- Check IP address is correct

### "Peer migration detected"
- Server detected IP change during transfer
- Normal during resume after Ctrl+C
- Server automatically handles reconnection

### "OpenSSL not found" (during init)
- This error should no longer occur - certificate generation is now built-in
- If you see this, you may be using an old version - rebuild with `cargo build --release`

### Resume not working
- Check `sftpx_resume/` directory exists
- Ensure file path is the same (session ID is path-based)
- Server must have write access to upload directory

## Performance Tips

1. **Increase chunk size** for large files (reduces overhead)
2. **Disable compression** for pre-compressed files (jpg, mp4, zip)
3. **Use gigabit network** - QUIC can saturate the connection
4. **Monitor with** `--log-level trace` for detailed diagnostics

## Examples

### Transfer a 10GB file with resume
```bash
# Server
./target/release/sftpx recv --upload-dir /mnt/storage

# Client
./target/release/sftpx send /path/to/10GB_file.iso 192.168.1.100
# Press Ctrl+C after 5GB transferred
# Resume:
./target/release/sftpx send /path/to/10GB_file.iso 192.168.1.100
# Continues from 5GB
```

### Batch transfer multiple files
```bash
for file in *.dat; do
    ./target/release/sftpx send "$file" 192.168.1.100
done
```

## Directory Structure

```
sftpx/
├── certs/              # TLS certificates (generated)
│   ├── cert.pem
│   ├── key.pem
│   └── openssl.cnf
├── sftpx_resume/       # Resume bitmaps (auto-created)
│   └── upload_*.bitmap
├── uploads/            # Received files (server-side)
├── target/release/
│   └── sftpx          # Compiled binary
└── scripts/
    ├── gen_certs.sh   # Linux/macOS cert generation
    └── gen_certs.ps1  # Windows cert generation
```

## Next Steps

- Read [USAGE.md](USAGE.md) for detailed protocol information
- Check [ARCHITECTURE.md](ARCHITECTURE.md) for implementation details
- See [docs/](docs/) for module documentation
