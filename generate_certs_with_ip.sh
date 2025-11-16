#!/bin/bash
# Generate self-signed certificates with IP address support
# Usage: ./generate_certs_with_ip.sh <server_ip>

SERVER_IP=${1:-"10.149.129.191"}
echo "Generating certificates for IP: $SERVER_IP"

mkdir -p certs

# Create OpenSSL config with IP SAN
cat > certs/openssl.cnf <<EOF
[req]
default_bits = 2048
prompt = no
default_md = sha256
distinguished_name = dn
req_extensions = v3_req

[dn]
C = US
ST = State
L = City
O = Organization
CN = $SERVER_IP

[v3_req]
subjectAltName = @alt_names

[alt_names]
DNS.1 = localhost
DNS.2 = *.local
IP.1 = 127.0.0.1
IP.2 = $SERVER_IP
EOF

# Generate private key
openssl genrsa -out certs/key.pem 2048

# Generate certificate with IP SAN
openssl req -new -x509 -key certs/key.pem -out certs/cert.pem -days 365 \
    -config certs/openssl.cnf -extensions v3_req

echo ""
echo "✅ Certificates generated:"
echo "   certs/cert.pem - Certificate (includes localhost + $SERVER_IP)"
echo "   certs/key.pem  - Private key"
echo ""
echo "Verify with: openssl x509 -in certs/cert.pem -text -noout | grep -A1 'Subject Alternative Name'"
