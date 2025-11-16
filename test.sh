#!/bin/bash

# Test script for running QUIC client and server together
# This script demonstrates proper testing of the QUIC connection

echo "==================================="
echo "QUIC Client/Server Test"
echo "==================================="
echo ""

# Build the examples first
echo "Building examples..."
cargo build --example test_server --example test_client 2>&1 | grep -E "(Compiling|Finished)"
echo ""

# Check if build succeeded
if [ ! -f "target/debug/examples/test_server" ] || [ ! -f "target/debug/examples/test_client" ]; then
    echo "❌ Build failed! Examples not found."
    exit 1
fi

echo "✓ Build successful"
echo ""

# Start server in background
echo "Starting server..."
./target/debug/examples/test_server &
SERVER_PID=$!

# Give server time to start
sleep 1

# Check if server is running
if ! kill -0 $SERVER_PID 2>/dev/null; then
    echo "❌ Server failed to start"
    exit 1
fi

echo "✓ Server started (PID: $SERVER_PID)"
echo ""

# Run client
echo "Running client..."
echo "-----------------------------------"
./target/debug/examples/test_client
CLIENT_EXIT=$?
echo "-----------------------------------"
echo ""

# Wait a moment for server to finish
sleep 1

# Clean up server if still running
if kill -0 $SERVER_PID 2>/dev/null; then
    echo "Stopping server..."
    kill $SERVER_PID 2>/dev/null
    wait $SERVER_PID 2>/dev/null
fi

# Report results
echo ""
echo "==================================="
if [ $CLIENT_EXIT -eq 0 ]; then
    echo "✓ Test PASSED"
    echo "==================================="
    exit 0
else
    echo "❌ Test FAILED (exit code: $CLIENT_EXIT)"
    echo "==================================="
    exit 1
fi
