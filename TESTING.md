# Testing Guide for QUIC Server

This guide explains how to verify that the QUIC server implementation works correctly.

## Prerequisites

1. **Install Rust** (if not already installed):
   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   ```

2. **Install Dependencies**:
   ```bash
   cd /home/dwhoik/sftpx
   cargo fetch
   ```

3. **Generate Test Certificates**:
   ```bash
   cd certs
   openssl req -x509 -newkey rsa:4096 -keyout key.pem -out cert.pem -days 365 -nodes -subj "/CN=localhost"
   cd ..
   ```

## Testing Methods

### Method 1: Quick Compilation Check

Verify the code compiles without errors:

```bash
cargo check
```

Expected output:
```
    Checking sftpx v0.1.0 (/home/dwhoik/sftpx)
    Finished dev [unoptimized + debuginfo] target(s) in X.XXs
```

### Method 2: Run Unit Tests

Run all unit tests in the server modules:

```bash
cargo test --lib
```

This tests:
- Server configuration defaults
- Stream manager functionality
- Data sender initialization
- Transfer manager setup

Expected output:
```
running 6 tests
test server::tests::test_server_config_default ... ok
test server::streams::tests::test_stream_ids ... ok
test server::streams::tests::test_stream_manager_creation ... ok
test server::sender::tests::test_data_sender_creation ... ok
test server::sender::tests::test_counter_reset ... ok
test server::transfer::tests::test_transfer_manager_creation ... ok

test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

### Method 3: Run Integration Tests

Run the integration tests:

```bash
cargo test --test server_test
```

### Method 4: Manual Server-Client Test

**Step 1: Start the Server** (Terminal 1)
```bash
cargo run --example test_server
```

Expected output:
```
=== QUIC Server Test ===

Server Configuration:
  - Address: 127.0.0.1:4443
  - Certificate: certs/cert.pem
  - Private Key: certs/key.pem
  - Max Idle Timeout: 5000ms
  - Max Data: 10000000 bytes
  - Max Streams: 100

Starting QUIC server...
✓ Server initialized successfully
✓ Listening for connections...
```

**Step 2: Run the Client** (Terminal 2)
```bash
cargo run --example test_client
```

Expected output:
```
=== QUIC Client Test ===

Client Configuration:
  - Local Address: 127.0.0.1:xxxxx
  - Server Address: 127.0.0.1:4443
  - Max Datagram Size: 1350 bytes

✓ QUIC connection initialized
✓ Sent initial packet (1200 bytes)

Completing handshake...
  - Received 1200 bytes from 127.0.0.1:4443
✓ Connection established!

Sending application data...
✓ Sent 23 bytes on stream 0

Waiting for server response...
✓ Stream 0 is readable
✓ Received from server: "Hello from QUIC server!"
✓ Stream 0 finished

✓ Test PASSED: Communication successful!
✓ Connection closed
```

**Server output should show:**
```
Server listening on 127.0.0.1:4443
Connection established, initializing streams...
Initialized stream: Control with ID 0
Initialized stream: Data1 with ID 4
Initialized stream: Data2 with ID 8
Initialized stream: Data3 with ID 12
Server: recv 1200 bytes from 127.0.0.1:xxxxx
Server: conn.readable() -> [0]
Server received on stream 0: Hello from QUIC client!
DataSender: sent 23 bytes on stream 0 (total: 23)
Message sent, closing server.
```

### Method 5: Build All Examples

Build all examples to ensure they compile:

```bash
cargo build --examples
```

### Method 6: Run with Verbose Output

For detailed debugging output:

```bash
RUST_LOG=debug cargo run --example test_server
```

## What to Look For

### ✅ Success Indicators

1. **Compilation**:
   - No compiler errors
   - No warnings about unused code
   - All dependencies resolved

2. **Server Startup**:
   - Server binds to port successfully
   - Certificates loaded without errors
   - "Listening for connections" message appears

3. **Client Connection**:
   - Handshake completes successfully
   - "Connection established" appears
   - 4 streams initialized (IDs: 0, 4, 8, 12)

4. **Data Exchange**:
   - Client message received by server
   - Server response received by client
   - Correct message content on both sides

5. **Stream Management**:
   - All 4 streams initialized
   - Stream IDs are correct (0, 4, 8, 12)
   - Bytes sent/received tracked properly

### ❌ Common Issues and Solutions

#### Issue: "Address already in use"
```
Error: Address already in use (os error 98)
```
**Solution**: Another process is using port 4443
```bash
# Find the process
lsof -i :4443
# Kill it
kill -9 <PID>
```

#### Issue: Certificate not found
```
Error: No such file or directory (os error 2)
```
**Solution**: Generate certificates
```bash
cd certs
openssl req -x509 -newkey rsa:4096 -keyout key.pem -out cert.pem -days 365 -nodes -subj "/CN=localhost"
```

#### Issue: Connection timeout
```
Error: Operation timed out
```
**Solution**: 
- Ensure server is running
- Check firewall settings
- Verify port 4443 is accessible

#### Issue: Compilation errors
```
error[E0433]: failed to resolve: use of undeclared crate or module
```
**Solution**: Add dependencies to Cargo.toml
```toml
[dependencies]
quiche = "0.20"
```

## Automated Test Script

Create a test script to automate testing:

```bash
#!/bin/bash
# test_server.sh

echo "=== SFTPX Server Test Suite ==="
echo ""

# Check certificates
echo "1. Checking certificates..."
if [ ! -f "certs/cert.pem" ] || [ ! -f "certs/key.pem" ]; then
    echo "   Generating certificates..."
    cd certs
    openssl req -x509 -newkey rsa:4096 -keyout key.pem -out cert.pem -days 365 -nodes -subj "/CN=localhost" 2>/dev/null
    cd ..
fi
echo "   ✓ Certificates ready"

# Compilation check
echo ""
echo "2. Checking compilation..."
cargo check --quiet 2>&1
if [ $? -eq 0 ]; then
    echo "   ✓ Code compiles successfully"
else
    echo "   ✗ Compilation failed"
    exit 1
fi

# Unit tests
echo ""
echo "3. Running unit tests..."
cargo test --lib --quiet 2>&1
if [ $? -eq 0 ]; then
    echo "   ✓ All unit tests passed"
else
    echo "   ✗ Some unit tests failed"
    exit 1
fi

# Build examples
echo ""
echo "4. Building examples..."
cargo build --examples --quiet 2>&1
if [ $? -eq 0 ]; then
    echo "   ✓ All examples built successfully"
else
    echo "   ✗ Example build failed"
    exit 1
fi

echo ""
echo "=== All Tests Passed! ==="
echo ""
echo "To run manual test:"
echo "  Terminal 1: cargo run --example test_server"
echo "  Terminal 2: cargo run --example test_client"
```

Make it executable:
```bash
chmod +x test_server.sh
./test_server.sh
```

## Performance Testing

Test with larger data transfers:

```bash
# Create test data
dd if=/dev/urandom of=test_data.bin bs=1M count=10

# Modify test_client.rs to read and send this file
# Then run the test
```

## Continuous Monitoring

While the server is running, monitor in another terminal:

```bash
# Monitor network traffic
watch -n 1 'netstat -an | grep 4443'

# Monitor server process
watch -n 1 'ps aux | grep test_server'

# Monitor system resources
htop
```

## Summary Checklist

- [ ] Code compiles without errors (`cargo check`)
- [ ] Unit tests pass (`cargo test --lib`)
- [ ] Certificates generated and in place
- [ ] Server starts successfully
- [ ] Client connects to server
- [ ] Handshake completes
- [ ] 4 streams initialized (IDs: 0, 4, 8, 12)
- [ ] Client sends message to server
- [ ] Server receives message
- [ ] Server sends response
- [ ] Client receives response
- [ ] Connection closes cleanly
- [ ] No memory leaks or panics

## Next Steps

Once basic tests pass:

1. **Stress Testing**: Run multiple clients simultaneously
2. **Large File Transfer**: Test with multi-GB files
3. **Network Simulation**: Test with packet loss and latency
4. **Security Testing**: Verify certificate validation
5. **Performance Profiling**: Use `cargo flamegraph`

## Getting Help

If tests fail:
1. Check error messages carefully
2. Review the output in both server and client terminals
3. Verify certificates are valid
4. Ensure port 4443 is available
5. Check file permissions
6. Review logs with `RUST_LOG=debug`
