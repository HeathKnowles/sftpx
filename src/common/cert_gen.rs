// Certificate generation module
// Generates self-signed TLS certificates for QUIC connections

use rcgen::{CertificateParams, DistinguishedName, DnType, SanType};
use std::fs;
use std::path::Path;

use crate::common::error::{Error, Result};

/// Generate self-signed TLS certificates for QUIC
///
/// Creates a certificate with Subject Alternative Names (SANs) for:
/// - localhost
/// - *.local
/// - 127.0.0.1
/// - The specified server IP
///
/// # Arguments
/// * `server_ip` - Server IP address to include in certificate SANs
/// * `output_dir` - Directory to save cert.pem and key.pem (default: "certs")
///
/// # Returns
/// Ok(()) if certificates generated successfully
pub fn generate_self_signed_cert(server_ip: &str, output_dir: Option<&str>) -> Result<()> {
    let cert_dir = output_dir.unwrap_or("certs");
    
    // Create output directory
    fs::create_dir_all(cert_dir).map_err(|e| {
        Error::Io(std::io::Error::new(
            e.kind(),
            format!("Failed to create directory {}: {}", cert_dir, e),
        ))
    })?;

    // Set up certificate parameters with DNS SANs
    let mut params = CertificateParams::new(vec![
        "localhost".to_string(),
        "*.local".to_string(),
    ]).map_err(|e| {
        Error::Io(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("Failed to create certificate params: {}", e),
        ))
    })?;
    
    // Set distinguished name
    let mut dn = DistinguishedName::new();
    dn.push(DnType::CountryName, "US");
    dn.push(DnType::StateOrProvinceName, "State");
    dn.push(DnType::LocalityName, "City");
    dn.push(DnType::OrganizationName, "SFTPX");
    dn.push(DnType::CommonName, server_ip);
    params.distinguished_name = dn;

    // Add IP addresses as SANs
    params.subject_alt_names.push(SanType::IpAddress(
        "127.0.0.1".parse().unwrap()
    ));
    
    // Add the server IP if it's different from localhost
    if server_ip != "127.0.0.1" && server_ip != "localhost" {
        if let Ok(ip) = server_ip.parse() {
            params.subject_alt_names.push(SanType::IpAddress(ip));
        } else {
            // Try as DNS name if not a valid IP
            if let Ok(dns) = rcgen::Ia5String::try_from(server_ip.to_string()) {
                params.subject_alt_names.push(SanType::DnsName(dns));
            }
        }
    }

    // Generate key pair and self-signed certificate
    let key_pair = rcgen::KeyPair::generate().map_err(|e| {
        Error::Io(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("Failed to generate key pair: {}", e),
        ))
    })?;

    let cert = params.self_signed(&key_pair).map_err(|e| {
        Error::Io(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("Failed to generate certificate: {}", e),
        ))
    })?;

    // Get PEM-encoded certificate and private key
    let cert_pem = cert.pem();
    let key_pem = key_pair.serialize_pem();

    // Write to files
    let cert_path = Path::new(cert_dir).join("cert.pem");
    let key_path = Path::new(cert_dir).join("key.pem");

    fs::write(&cert_path, cert_pem.as_bytes()).map_err(|e| {
        Error::Io(std::io::Error::new(
            e.kind(),
            format!("Failed to write certificate to {:?}: {}", cert_path, e),
        ))
    })?;

    fs::write(&key_path, key_pem.as_bytes()).map_err(|e| {
        Error::Io(std::io::Error::new(
            e.kind(),
            format!("Failed to write private key to {:?}: {}", key_path, e),
        ))
    })?;

    println!("\n✅ Certificates generated successfully:");
    println!("   {:?} - Certificate (includes localhost + {})", cert_path, server_ip);
    println!("   {:?} - Private key", key_path);
    println!("\nCertificate SANs:");
    println!("   - DNS: localhost");
    println!("   - DNS: *.local");
    println!("   - IP: 127.0.0.1");
    if server_ip != "127.0.0.1" && server_ip != "localhost" {
        println!("   - IP/DNS: {}", server_ip);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_generate_cert_localhost() {
        let test_dir = "test_certs_localhost";
        let _ = fs::remove_dir_all(test_dir);
        
        let result = generate_self_signed_cert("127.0.0.1", Some(test_dir));
        assert!(result.is_ok());
        
        // Verify files exist
        assert!(Path::new(test_dir).join("cert.pem").exists());
        assert!(Path::new(test_dir).join("key.pem").exists());
        
        // Cleanup
        let _ = fs::remove_dir_all(test_dir);
    }

    #[test]
    fn test_generate_cert_custom_ip() {
        let test_dir = "test_certs_custom";
        let _ = fs::remove_dir_all(test_dir);
        
        let result = generate_self_signed_cert("192.168.1.100", Some(test_dir));
        assert!(result.is_ok());
        
        // Verify files exist
        assert!(Path::new(test_dir).join("cert.pem").exists());
        assert!(Path::new(test_dir).join("key.pem").exists());
        
        // Cleanup
        let _ = fs::remove_dir_all(test_dir);
    }
}
