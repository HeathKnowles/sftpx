# Scripts Directory

This directory contains utility scripts for SFTPX development and deployment.

## Certificate Generation

### Recommended: Use CLI (Built-in, No Dependencies)

The easiest and most portable way to generate certificates is through the SFTPX CLI:

```bash
sftpx init --ip <server_ip>
```

**Advantages:**
- ✅ No external dependencies (OpenSSL not required)
- ✅ Works on all platforms (Linux, macOS, Windows)
- ✅ Self-contained in the binary
- ✅ Uses native Rust cryptography (rcgen)
- ✅ Works even when installed via `cargo install sftpx`

### Alternative: Shell Scripts (Requires OpenSSL)

For legacy compatibility or if you prefer using OpenSSL directly:

### gen_certs.sh (Linux/macOS)
Generates self-signed TLS certificates for QUIC connections on Unix-like systems.

**Usage:**
```bash
./scripts/gen_certs.sh [server_ip]
```

**Default IP:** 127.0.0.1

**Example:**
```bash
./scripts/gen_certs.sh 192.168.1.100
```

### gen_certs.ps1 (Windows)
PowerShell script for generating self-signed TLS certificates on Windows.

**Usage:**
```powershell
.\scripts\gen_certs.ps1 [-ServerIP <ip>]
```

**Example:**
```powershell
.\scripts\gen_certs.ps1 -ServerIP 192.168.1.100
```

**Requirements:**
- OpenSSL installed and available in PATH
- On Windows: Install via [Win32 OpenSSL](https://slproweb.com/products/Win32OpenSSL.html) or `winget install -e --id ShiningLight.OpenSSL`
- On Linux: `sudo apt install openssl` or `sudo yum install openssl`
- On macOS: Pre-installed or `brew install openssl`

**Note:** These scripts are deprecated in favor of the built-in `sftpx init` command.

## Generated Files

Both scripts create the following files in the `certs/` directory:

- **cert.pem** - Self-signed certificate (valid for 365 days)
- **key.pem** - Private key (2048-bit RSA)
- **openssl.cnf** - OpenSSL configuration with Subject Alternative Names (SANs)

### Certificate SANs

The generated certificates include the following Subject Alternative Names:
- DNS: localhost
- DNS: *.local
- IP: 127.0.0.1
- IP: <server_ip> (the IP you specified)

This allows the server to accept connections from localhost and the specified IP address.

## Other Scripts

### bench.sh
Runs performance benchmarks for chunking, hashing, and transfer operations.

### run_tests.sh
Executes the full test suite including integration tests.
