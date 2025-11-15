// Realistic compression test with various data types

use sftpx::chunking::{ChunkCompressor, CompressionAlgorithm, CompressionStats};
use std::fs::File;
use std::io::{Write, Read};
use tempfile::tempdir;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Realistic Compression Benchmarks ===\n");

    let temp_dir = tempdir()?;

    // Test 1: Text file (source code)
    println!("Test 1: Source Code (Rust file)");
    test_compression_on_file("src/chunking/compress.rs", "LZ4", CompressionAlgorithm::Lz4)?;
    test_compression_on_file("src/chunking/compress.rs", "Zstd-3", CompressionAlgorithm::Zstd(3))?;
    test_compression_on_file("src/chunking/compress.rs", "Zstd-10", CompressionAlgorithm::Zstd(10))?;
    test_compression_on_file("src/chunking/compress.rs", "LZMA2-6", CompressionAlgorithm::Lzma2(6))?;
    println!();

    // Test 2: JSON-like data
    println!("Test 2: JSON Data");
    let json_data = generate_json_data(10000);
    test_compression_on_data(&json_data, "LZ4", CompressionAlgorithm::Lz4)?;
    test_compression_on_data(&json_data, "Zstd-3", CompressionAlgorithm::Zstd(3))?;
    test_compression_on_data(&json_data, "Zstd-10", CompressionAlgorithm::Zstd(10))?;
    test_compression_on_data(&json_data, "LZMA2-6", CompressionAlgorithm::Lzma2(6))?;
    println!();

    // Test 3: Binary data (pseudo-random)
    println!("Test 3: Binary/Random Data");
    let binary_data = generate_binary_data(50000);
    test_compression_on_data(&binary_data, "LZ4", CompressionAlgorithm::Lz4)?;
    test_compression_on_data(&binary_data, "Zstd-3", CompressionAlgorithm::Zstd(3))?;
    test_compression_on_data(&binary_data, "Zstd-10", CompressionAlgorithm::Zstd(10))?;
    println!();

    // Test 4: Mixed content (realistic file)
    println!("Test 4: Mixed Content");
    let mixed_data = generate_mixed_data();
    test_compression_on_data(&mixed_data, "LZ4", CompressionAlgorithm::Lz4)?;
    test_compression_on_data(&mixed_data, "Zstd-3", CompressionAlgorithm::Zstd(3))?;
    test_compression_on_data(&mixed_data, "Zstd-10", CompressionAlgorithm::Zstd(10))?;
    println!();

    // Test 5: Chunked compression (simulating file transfer)
    println!("Test 5: Chunked Transfer (64KB chunks)");
    test_chunked_compression()?;

    Ok(())
}

fn test_compression_on_file(
    path: &str,
    label: &str,
    algorithm: CompressionAlgorithm,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut file = File::open(path)?;
    let mut data = Vec::new();
    file.read_to_end(&mut data)?;
    
    test_compression_on_data(&data, label, algorithm)
}

fn test_compression_on_data(
    data: &[u8],
    label: &str,
    algorithm: CompressionAlgorithm,
) -> Result<(), Box<dyn std::error::Error>> {
    let original_size = data.len();
    
    let start = std::time::Instant::now();
    let compressed = ChunkCompressor::compress(data, algorithm)?;
    let compress_time = start.elapsed();
    
    let start = std::time::Instant::now();
    let decompressed = ChunkCompressor::decompress(
        &compressed.compressed_data,
        algorithm,
        Some(original_size),
    )?;
    let decompress_time = start.elapsed();
    
    assert_eq!(decompressed.len(), original_size);
    
    let ratio = compressed.compressed_size as f64 / original_size as f64;
    let savings = ((1.0 - ratio) * 100.0).max(0.0);
    
    println!("  {:<10} | {:>8} → {:>8} bytes | Ratio: {:>5.1}% | Saved: {:>5.1}% | Compress: {:>6.2}ms | Decompress: {:>6.2}ms",
        label,
        original_size,
        compressed.compressed_size,
        ratio * 100.0,
        savings,
        compress_time.as_secs_f64() * 1000.0,
        decompress_time.as_secs_f64() * 1000.0,
    );
    
    Ok(())
}

fn generate_json_data(entries: usize) -> Vec<u8> {
    let mut data = String::from("[\n");
    for i in 0..entries {
        data.push_str(&format!(
            r#"  {{"id": {}, "name": "User{}", "email": "user{}@example.com", "active": {}}},{}"#,
            i, i, i, i % 2 == 0, "\n"
        ));
    }
    data.push_str("]\n");
    data.into_bytes()
}

fn generate_binary_data(size: usize) -> Vec<u8> {
    // Pseudo-random but deterministic
    (0..size).map(|i| {
        let x = i.wrapping_mul(1103515245).wrapping_add(12345);
        ((x / 65536) % 256) as u8
    }).collect()
}

fn generate_mixed_data() -> Vec<u8> {
    let mut data = Vec::new();
    
    // Some text
    data.extend_from_slice(b"# Configuration File\n\n");
    data.extend_from_slice(b"server_name = \"example.com\"\n");
    data.extend_from_slice(b"port = 8080\n");
    data.extend_from_slice(b"timeout = 30\n\n");
    
    // Some binary
    data.extend_from_slice(&[0x89, 0x50, 0x4E, 0x47]); // PNG header
    data.extend_from_slice(&(0..1024).map(|i| (i % 256) as u8).collect::<Vec<_>>());
    
    // More text
    for i in 0..100 {
        data.extend_from_slice(format!("log entry {}: some event occurred\n", i).as_bytes());
    }
    
    // Some repeated patterns (somewhat compressible)
    data.extend_from_slice(&[0xAB; 512]);
    
    // Random-ish data
    data.extend_from_slice(&generate_binary_data(2048));
    
    data
}

fn test_chunked_compression() -> Result<(), Box<dyn std::error::Error>> {
    // Simulate transferring a large file in chunks
    let file_data = generate_mixed_data().repeat(50); // ~200KB
    let chunk_size = 65536; // 64KB
    
    let mut stats_lz4 = CompressionStats::new();
    let mut stats_zstd3 = CompressionStats::new();
    let mut stats_auto = CompressionStats::new();
    
    for chunk_start in (0..file_data.len()).step_by(chunk_size) {
        let chunk_end = (chunk_start + chunk_size).min(file_data.len());
        let chunk = &file_data[chunk_start..chunk_end];
        
        let c_lz4 = ChunkCompressor::compress(chunk, CompressionAlgorithm::Lz4)?;
        stats_lz4.add_chunk(&c_lz4);
        
        let c_zstd = ChunkCompressor::compress(chunk, CompressionAlgorithm::Zstd(3))?;
        stats_zstd3.add_chunk(&c_zstd);
        
        let c_auto = ChunkCompressor::compress_auto(chunk)?;
        stats_auto.add_chunk(&c_auto);
    }
    
    println!("  File size: {} bytes, {} chunks", file_data.len(), stats_lz4.total_chunks);
    println!();
    println!("  LZ4:      {} → {} bytes ({:.1}% ratio, {:.1}% saved)",
        stats_lz4.original_bytes,
        stats_lz4.compressed_bytes,
        stats_lz4.overall_ratio() * 100.0,
        (1.0 - stats_lz4.overall_ratio()) * 100.0,
    );
    println!("  Zstd-3:   {} → {} bytes ({:.1}% ratio, {:.1}% saved)",
        stats_zstd3.original_bytes,
        stats_zstd3.compressed_bytes,
        stats_zstd3.overall_ratio() * 100.0,
        (1.0 - stats_zstd3.overall_ratio()) * 100.0,
    );
    println!("  Auto:     {} → {} bytes ({:.1}% ratio, {:.1}% saved)",
        stats_auto.original_bytes,
        stats_auto.compressed_bytes,
        stats_auto.overall_ratio() * 100.0,
        (1.0 - stats_auto.overall_ratio()) * 100.0,
    );
    println!("           {} LZ4, {} Zstd, {} uncompressed",
        stats_auto.lz4_count,
        stats_auto.zstd_count,
        stats_auto.none_count,
    );
    
    Ok(())
}
