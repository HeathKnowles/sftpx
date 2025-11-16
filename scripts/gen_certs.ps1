# Generate self-signed certificates with IP address support for Windows
# Usage: .\gen_certs.ps1 [server_ip]

param(
    [string]$ServerIP = "127.0.0.1"
)

Write-Host "Generating certificates for IP: $ServerIP" -ForegroundColor Cyan

# Create certs directory if it doesn't exist
$certsDir = "certs"
if (-not (Test-Path $certsDir)) {
    New-Item -ItemType Directory -Path $certsDir | Out-Null
}

# Create OpenSSL config with IP SAN
$opensslConfig = @"
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
CN = $ServerIP

[v3_req]
subjectAltName = @alt_names

[alt_names]
DNS.1 = localhost
DNS.2 = *.local
IP.1 = 127.0.0.1
IP.2 = $ServerIP
"@

$opensslConfig | Out-File -FilePath "$certsDir\openssl.cnf" -Encoding ASCII

# Check if OpenSSL is available
$opensslPath = Get-Command openssl -ErrorAction SilentlyContinue
if (-not $opensslPath) {
    Write-Host "Error: OpenSSL not found in PATH" -ForegroundColor Red
    Write-Host "Please install OpenSSL:" -ForegroundColor Yellow
    Write-Host "  - Download from: https://slproweb.com/products/Win32OpenSSL.html" -ForegroundColor Yellow
    Write-Host "  - Or install via: winget install -e --id ShiningLight.OpenSSL" -ForegroundColor Yellow
    exit 1
}

try {
    # Generate private key
    Write-Host "Generating private key..." -ForegroundColor Yellow
    & openssl genrsa -out "$certsDir\key.pem" 2048 2>&1 | Out-Null
    
    if ($LASTEXITCODE -ne 0) {
        throw "Failed to generate private key"
    }

    # Generate certificate with IP SAN
    Write-Host "Generating certificate..." -ForegroundColor Yellow
    & openssl req -new -x509 -key "$certsDir\key.pem" -out "$certsDir\cert.pem" -days 365 `
        -config "$certsDir\openssl.cnf" -extensions v3_req 2>&1 | Out-Null
    
    if ($LASTEXITCODE -ne 0) {
        throw "Failed to generate certificate"
    }

    Write-Host ""
    Write-Host "✅ Certificates generated successfully:" -ForegroundColor Green
    Write-Host "   $certsDir\cert.pem - Certificate (includes localhost + $ServerIP)" -ForegroundColor Green
    Write-Host "   $certsDir\key.pem  - Private key" -ForegroundColor Green
    Write-Host ""
    Write-Host "Verify with: openssl x509 -in $certsDir\cert.pem -text -noout | Select-String -Pattern 'Subject Alternative Name' -Context 0,1" -ForegroundColor Cyan
}
catch {
    Write-Host "Error: $_" -ForegroundColor Red
    exit 1
}
