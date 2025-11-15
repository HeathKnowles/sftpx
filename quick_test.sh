#!/bin/bash
# Quick test: Runs server and client automatically

set -e

echo "=== Quick QUIC Server Test ==="
echo ""

# Check certificates
if [ ! -f "certs/cert.pem" ] || [ ! -f "certs/key.pem" ]; then
    echo "Generating certificates..."
    mkdir -p certs
    cd certs
    openssl req -x509 -newkey rsa:4096 -keyout key.pem -out cert.pem -days 365 -nodes -subj "/CN=localhost" 2>/dev/null
    cd ..
fi

# Build first
echo "Building..."
cargo build --examples --quiet 2>&1

# Start server in background
echo "Starting server..."
cargo run --example test_server > /tmp/sftpx_server.log 2>&1 &
SERVER_PID=$!

# Wait for server to start
sleep 2

# Check if server is running
if ! ps -p $SERVER_PID > /dev/null; then
    echo "❌ Server failed to start"
    cat /tmp/sftpx_server.log
    exit 1
fi

echo "Server running (PID: $SERVER_PID)"
echo ""
echo "Running client..."
echo ""

# Run client
if cargo run --example test_client 2>&1; then
    echo ""
    echo "✅ Test completed successfully!"
    EXIT_CODE=0
else
    echo ""
    echo "❌ Test failed"
    EXIT_CODE=1
fi

# Cleanup
echo ""
echo "Stopping server..."
kill $SERVER_PID 2>/dev/null || true
wait $SERVER_PID 2>/dev/null || true

# Show server logs
echo ""
echo "=== Server Logs ==="
cat /tmp/sftpx_server.log

exit $EXIT_CODE
