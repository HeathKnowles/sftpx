# Client Default Configuration Guide

## Overview

The SFTPX client now provides convenient default configurations that automatically use the certificates from the `certs/` directory, making it easier to get started with secure QUIC connections.

## Default Configuration

When you use `Client::default()`, the following defaults are applied:

```rust
ClientConfig {
    server_addr: "127.0.0.1:4433",
    server_name: "localhost",
    chunk_size: 1048576,              // 1MB
    max_retries: 3,
    timeout: 30 seconds,
    session_dir: ".sftpx/sessions",
    verify_cert: true,               // Disabled by default for easier testing
    ca_cert_path: Some("certs/cert.pem"),  // Automatically points to cert file
}
```

## Usage Examples

### Example 1: Default Configuration

```rust
use sftpx::client::Client;

let client = Client::default();
// Connects to localhost:4433 using certs/cert.pem
```

### Example 2: Custom Server Address with Defaults

```rust
use sftpx::client::Client;

let client = Client::with_defaults("127.0.0.1:8443")?;
// Uses certs/cert.pem for TLS verification
```

### Example 3: Custom Configuration

```rust
use sftpx::common::ClientConfig;
use sftpx::client::Client;
use std::path::PathBuf;

let config = ClientConfig::default()
    .with_chunk_size(2 * 1024 * 1024)?     // 2MB chunks
    .with_max_retries(5)
    .enable_cert_verification()
    .with_ca_cert(PathBuf::from("certs/cert.pem"));

let client = Client::new(config);
```

### Example 4: Disable Certificate Verification

```rust
use sftpx::common::ClientConfig;
use sftpx::client::Client;

let config = ClientConfig::default()
    .disable_cert_verification();

let client = Client::new(config);
```

## Configuration Methods

### ClientConfig Methods

- `ClientConfig::default()` - Creates config with default values
- `ClientConfig::new(addr, server_name)` - Creates config with custom server
- `.with_chunk_size(size)` - Set chunk size (returns Result)
- `.with_timeout(duration)` - Set connection timeout
- `.with_session_dir(path)` - Set session directory
- `.with_max_retries(n)` - Set maximum retry attempts
- `.with_ca_cert(path)` - Set CA certificate path and enable verification
- `.enable_cert_verification()` - Enable TLS certificate verification
- `.disable_cert_verification()` - Disable TLS certificate verification

### Client Methods

- `Client::default()` - Create client with default configuration
- `Client::with_defaults(addr)` - Create client for specific server with defaults
- `Client::new(config)` - Create client with custom configuration
- `.send_file(path, dest)` - Send a file to the server
- `.receive_file(session_id)` - Receive a file from the server
- `.resume_transfer(session_id)` - Resume a previous transfer
- `.config()` - Get reference to current configuration

## Certificate Setup

The default configuration expects certificates in the `certs/` directory:

```
project_root/
├── certs/
│   ├── cert.pem    # Server certificate (used as CA cert by default)
│   └── key.pem     # Private key (used by server)
```

### Generate Self-Signed Certificates for Testing

```bash
cd certs
openssl req -x509 -newkey rsa:2048 -nodes \
  -keyout key.pem -out cert.pem \
  -subj "/CN=localhost" -days 365
```

## Security Considerations

### Development vs Production

**Default Configuration (Development):**
- Certificate verification is **disabled** by default
- Uses localhost and port 4433
- Suitable for local testing

**Production Configuration:**
- Always **enable certificate verification**:
  ```rust
  let config = ClientConfig::default()
      .enable_cert_verification()
      .with_ca_cert(PathBuf::from("path/to/ca-cert.pem"));
  ```
- Use proper CA-signed certificates
- Verify the server name matches the certificate

### Best Practices

1. **Development**: Use `Client::default()` with self-signed certs
2. **Testing**: Use `Client::with_defaults()` with `disable_cert_verification()`
3. **Production**: Always enable verification with proper CA certificates:

```rust
use sftpx::common::ClientConfig;
use sftpx::client::Client;
use std::path::PathBuf;

let config = ClientConfig::new(
    "production-server.example.com:4433".parse()?,
    "production-server.example.com".to_string()
)
.enable_cert_verification()
.with_ca_cert(PathBuf::from("/etc/ssl/certs/ca-bundle.crt"))
.with_chunk_size(4 * 1024 * 1024)?;  // 4MB chunks for production

let client = Client::new(config);
```

## Full Example

```rust
use sftpx::client::Client;
use sftpx::common::Result;

fn main() -> Result<()> {
    env_logger::init();
    
    // Simple usage with defaults
    let client = Client::default();
    
    // Send a file
    let mut transfer = client.send_file("myfile.dat", "remote/path/")?;
    transfer.run()?;
    
    println!("Transfer complete! Progress: {:.2}%", transfer.progress());
    
    Ok(())
}
```

## Troubleshooting

### Certificate Not Found

If you get a certificate error:

1. Ensure `certs/cert.pem` exists in the project root
2. Generate certificates using the command above
3. Or specify a custom path:
   ```rust
   .with_ca_cert(PathBuf::from("path/to/cert.pem"))
   ```

### Connection Refused

If the connection is refused:

1. Ensure the server is running on the configured port
2. Check firewall settings
3. Verify the server address is correct

### Certificate Verification Failed

If verification fails:

1. For testing, disable verification: `.disable_cert_verification()`
2. For production, ensure:
   - The CA cert is valid
   - The server name matches the certificate CN
   - The certificate is not expired

## See Also

- [Server Usage Guide](server_usage.md)
- [Architecture Documentation](../ARCHITECTURE.md)
- [Examples](../examples/)
