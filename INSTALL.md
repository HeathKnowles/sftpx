# Installing SFTPX from crates.io

## Quick Install

```bash
cargo install sftpx
```

That's it! No additional dependencies required - certificate generation is built into the binary.

## Verify Installation

```bash
sftpx --help
```

You should see:
```
QUIC-based file transfer tool with auto-resume

Usage: sftpx <COMMAND>

Commands:
  send  Send a file to a remote server
  recv  Start server to receive files
  init  Initialize certificates for QUIC connections
  help  Print this message or the help of the given subcommand(s)
```

## First-Time Setup

After installation, generate TLS certificates:

```bash
sftpx init --ip 127.0.0.1
```

This creates:
- `certs/cert.pem` - Self-signed certificate (365 days validity)
- `certs/key.pem` - ECDSA private key
- Subject Alternative Names for localhost and your specified IP

## Usage Examples

### Start a Server

```bash
# Receive files on default port (4443)
sftpx recv

# Custom bind address and upload directory
sftpx recv --bind 192.168.1.100:4443 --upload-dir ~/incoming
```

### Send a File

```bash
# Send to localhost
sftpx send myfile.dat

# Send to remote server
sftpx send myfile.dat 192.168.1.100
```

### Auto-Resume

Transfers automatically resume if interrupted:

```bash
sftpx send large_file.iso 192.168.1.100
# Press Ctrl+C to interrupt

# Resume automatically
sftpx send large_file.iso 192.168.1.100
# Continues from where it left off
```

## Features

- ✅ **QUIC Protocol** - Built on Google's QUIC with CUBIC congestion control
- ✅ **Auto-Resume** - Interrupt and resume transfers seamlessly
- ✅ **Integrity Verification** - BLAKE3 hashing per 2MB chunk
- ✅ **Zero External Dependencies** - Certificate generation built-in
- ✅ **Cross-Platform** - Works on Linux, macOS, and Windows
- ✅ **Fast** - Multi-stream QUIC for high throughput

## Advanced Options

### Custom Chunk Size

The default chunk size is 2MB. To modify, clone the repository and edit `src/main.rs`:

```rust
let config = ClientConfig::new(server_addr, server_name)
    .with_chunk_size(4 * 1024 * 1024)?  // 4 MB chunks
```

### Enable Compression

```rust
.with_compression(CompressionType::Zstd)  // or Gzip, Lz4, None
```

## Troubleshooting

### Port Already in Use

If port 4443 is in use, specify a different port:

```bash
# Server
sftpx recv --bind 0.0.0.0:8443

# Client
sftpx send file.dat 192.168.1.100:8443
```

### Firewall Issues

Ensure UDP port 4443 (or your custom port) is open:

```bash
# Linux (ufw)
sudo ufw allow 4443/udp

# Linux (firewalld)
sudo firewall-cmd --add-port=4443/udp --permanent
sudo firewall-cmd --reload
```

### Certificate Verification Failed

For production use, copy the server's `cert.pem` to the client machine. For testing, the client uses `disable_cert_verification()` by default.

## Uninstall

```bash
cargo uninstall sftpx
```

## Build from Source

For the latest development version:

```bash
git clone https://github.com/HeathKnowles/sftpx.git
cd sftpx
cargo build --release
./target/release/sftpx --help
```

## Documentation

- [README.md](https://github.com/HeathKnowles/sftpx/blob/main/README.md) - Full feature overview
- [QUICKSTART.md](https://github.com/HeathKnowles/sftpx/blob/main/QUICKSTART.md) - Detailed usage guide
- [ARCHITECTURE.md](https://github.com/HeathKnowles/sftpx/blob/main/ARCHITECTURE.md) - Technical details

## License

MIT License - see [LICENSE](https://github.com/HeathKnowles/sftpx/blob/main/LICENSE) file.

## Support

- Report issues: https://github.com/HeathKnowles/sftpx/issues
- Contribute: Pull requests welcome!
