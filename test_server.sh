#!/bin/bash
# Automated test script for SFTPX QUIC Server

set -e  # Exit on error

echo "=== SFTPX Server Test Suite ==="
echo ""

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Function to print colored output
print_success() {
    echo -e "${GREEN}✓ $1${NC}"
}

print_error() {
    echo -e "${RED}✗ $1${NC}"
}

print_info() {
    echo -e "${YELLOW}➜ $1${NC}"
}

# Check certificates
echo "Step 1: Checking certificates..."
if [ ! -f "certs/cert.pem" ] || [ ! -f "certs/key.pem" ]; then
    print_info "Generating self-signed certificates..."
    mkdir -p certs
    cd certs
    openssl req -x509 -newkey rsa:4096 -keyout key.pem -out cert.pem -days 365 -nodes -subj "/CN=localhost" 2>/dev/null
    cd ..
    print_success "Certificates generated"
else
    print_success "Certificates found"
fi

# Compilation check
echo ""
echo "Step 2: Checking compilation..."
print_info "Running cargo check..."
if cargo check --quiet 2>&1; then
    print_success "Code compiles successfully"
else
    print_error "Compilation failed"
    exit 1
fi

# Unit tests
echo ""
echo "Step 3: Running unit tests..."
print_info "Executing cargo test --lib..."
if cargo test --lib --quiet 2>&1; then
    print_success "All unit tests passed"
else
    print_error "Some unit tests failed"
    exit 1
fi

# Build examples
echo ""
echo "Step 4: Building examples..."
print_info "Building test_server and test_client..."
if cargo build --example test_server --example test_client --quiet 2>&1; then
    print_success "Test examples built successfully"
else
    print_error "Example build failed"
    exit 1
fi

echo ""
echo "=== All Automated Tests Passed! ==="
echo ""
echo "To run manual server-client test:"
echo ""
echo "  Terminal 1: ${GREEN}cargo run --example test_server${NC}"
echo "  Terminal 2: ${GREEN}cargo run --example test_client${NC}"
echo ""
echo "Or use the quick test script:"
echo "  ${GREEN}./quick_test.sh${NC}"
echo ""
