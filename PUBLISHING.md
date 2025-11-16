# SFTPX - Publishing to crates.io Checklist

## ✅ Completed Tasks

### 1. Certificate Generation (Native Rust)
- ✅ Replaced OpenSSL shell scripts with native Rust implementation
- ✅ Uses `rcgen` crate for certificate generation
- ✅ No external dependencies (OpenSSL not required)
- ✅ Works on all platforms (Linux, macOS, Windows)
- ✅ Self-contained in binary - works even when installed via `cargo install`

### 2. Cargo.toml Metadata
- ✅ Added package metadata:
  - `authors = ["Heath Knowles"]`
  - `description = "QUIC-based file transfer tool with auto-resume capability"`
  - `license = "MIT"`
  - `repository = "https://github.com/HeathKnowles/sftpx"`
  - `readme = "README.md"`
  - `keywords = ["quic", "file-transfer", "networking", "resume", "chunking"]`
  - `categories = ["command-line-utilities", "network-programming"]`
- ✅ Added version flag support

### 3. Dependencies Added
- `rcgen = "0.13"` - Certificate generation
- `rustls-pemfile = "2.2"` - PEM file handling
- `time = { version = "0.3", features = ["std"] }` - Time utilities for rcgen

### 4. Documentation Updates
- ✅ README.md - Updated with no-dependency installation
- ✅ QUICKSTART.md - Removed OpenSSL requirements
- ✅ scripts/README.md - Marked shell scripts as deprecated
- ✅ INSTALL.md - Created comprehensive installation guide for crates.io users

### 5. Testing
- ✅ Built release binary
- ✅ Tested `sftpx init` command in clean directory
- ✅ Verified certificate generation with custom IPs
- ✅ Validated certificate SANs with OpenSSL
- ✅ Confirmed version flag works: `sftpx --version`

## Publishing to crates.io

### Prerequisites
1. Create account at https://crates.io
2. Get API token from https://crates.io/settings/tokens
3. Login: `cargo login <token>`

### Pre-publish Checklist

```bash
# 1. Ensure all tests pass
cargo test

# 2. Check package
cargo package --list

# 3. Build release
cargo build --release

# 4. Test locally
./target/release/sftpx --version
./target/release/sftpx init
./target/release/sftpx --help

# 5. Dry run publish
cargo publish --dry-run

# 6. Actual publish
cargo publish
```

### After Publishing

Users can install with:
```bash
cargo install sftpx
```

## Key Features for crates.io Description

When publishing, highlight:

1. **Zero External Dependencies** - Certificate generation built-in (no OpenSSL required)
2. **Auto-Resume** - Interrupt and resume transfers automatically
3. **QUIC Protocol** - Modern transport with congestion control
4. **Cross-Platform** - Linux, macOS, Windows support
5. **Fast & Reliable** - BLAKE3 integrity checks, multi-stream transfers

## Version Bump Workflow

For future releases:

```bash
# 1. Update version in Cargo.toml
# [package]
# version = "0.2.0"

# 2. Create git tag
git tag -a v0.2.0 -m "Release version 0.2.0"
git push origin v0.2.0

# 3. Publish
cargo publish
```

## File Structure for Publication

Files included in crate (via `cargo package`):
```
sftpx/
├── Cargo.toml          # Package metadata
├── README.md           # Main documentation
├── LICENSE             # MIT license
├── INSTALL.md          # Installation guide
├── QUICKSTART.md       # Quick start guide
├── src/                # Source code
│   ├── main.rs         # CLI entry point
│   ├── lib.rs          # Library
│   ├── client/         # Client implementation
│   ├── server/         # Server implementation
│   ├── common/         # Common utilities
│   │   └── cert_gen.rs # Built-in cert generation ✨
│   └── ...
├── benches/            # Benchmarks
└── tests/              # Integration tests
```

## Maintenance Notes

### Shell Scripts (Deprecated)
- `scripts/gen_certs.sh` - Keep for legacy/development use
- `scripts/gen_certs.ps1` - Keep for legacy/development use
- These are NOT included in published crate
- Users should use `sftpx init` instead

### Certificate Generation
- Built-in via `src/common/cert_gen.rs`
- Uses ECDSA (faster than RSA)
- 365-day validity
- SANs: localhost, *.local, 127.0.0.1, custom IP

## Support & Issues

After publishing:
1. Monitor https://crates.io/crates/sftpx for downloads
2. Watch GitHub issues
3. Update documentation as needed
4. Release patches for critical bugs

## Next Steps

Before publishing:
1. ✅ Verify all compiler warnings resolved
2. ✅ Test certificate generation on all platforms
3. ⚠️  Add more integration tests (optional)
4. ⚠️  Add CI/CD pipeline (optional but recommended)
5. ⚠️  Create GitHub releases (optional)

Ready to publish when you run:
```bash
cargo publish
```

## Notes

- Current version: 0.1.0
- License: MIT
- Repository: https://github.com/HeathKnowles/sftpx
- Categories: command-line-utilities, network-programming
- Keywords: quic, file-transfer, networking, resume, chunking
