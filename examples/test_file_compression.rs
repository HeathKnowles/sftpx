// Test compression on actual files (MP4, images, etc.)
// Usage: cargo run --example test_file_compression <file_path>

use sftpx::chunking::{ChunkCompressor, CompressionAlgorithm};
use std::env;
use std::fs::File;
use std::io::Read;
use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    
    if args.len() < 2 {
        eprintln!("Usage: {} <file_path>", args[0]);
        eprintln!("\nExample:");
        eprintln!("  cargo run --example test_file_compression video.mp4");
        eprintln!("  cargo run --example test_file_compression image.jpg");
        eprintln!("  cargo run --example test_file_compression document.pdf");
        std::process::exit(1);
    }
    
    let file_path = &args[1];
    test_file_compression(Path::new(file_path))?;
    
    Ok(())
}

fn test_file_compression(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    // Check if file exists
    if !path.exists() {
        return Err(format!("File not found: {}", path.display()).into());
    }
    
    // Get file info
    let metadata = std::fs::metadata(path)?;
    let file_size = metadata.len();
    let extension = path.extension()
        .and_then(|e| e.to_str())
        .unwrap_or("unknown")
        .to_lowercase();
    
    println!("╔════════════════════════════════════════════════════════════╗");
    println!("║              File Compression Analysis                     ║");
    println!("╚════════════════════════════════════════════════════════════╝\n");
    
    println!("File: {}", path.display());
    println!("Size: {} bytes ({:.2} MB)", file_size, file_size as f64 / 1_048_576.0);
    println!("Type: {}", extension.to_uppercase());
    println!();
    
    // Detect file type and recommend compression
    let (should_compress, reason) = should_compress_file_type(&extension);
    
    if !should_compress {
        println!("⚠️  Recommendation: SKIP COMPRESSION");
        println!("   Reason: {}", reason);
        println!("\n   This file type is already compressed or won't benefit");
        println!("   from additional compression. Transmit as-is.\n");
    } else {
        println!("✓  Recommendation: COMPRESS");
        println!("   Reason: {}", reason);
        println!();
    }
    
    // Read file (limit to 10MB for demo)
    let max_read = (file_size as usize).min(10 * 1024 * 1024);
    let mut file = File::open(path)?;
    let mut data = vec![0u8; max_read];
    file.read_exact(&mut data)?;
    
    if file_size > max_read as u64 {
        println!("Note: Testing on first {} bytes (10MB sample)\n", max_read);
    }
    
    // Test different compression algorithms
    println!("─────────────────────────────────────────────────────────────");
    println!("Testing Compression Algorithms:");
    println!("─────────────────────────────────────────────────────────────\n");
    
    test_algorithm(&data, "No Compression", CompressionAlgorithm::None)?;
    test_algorithm(&data, "LZ4", CompressionAlgorithm::Lz4)?;
    test_algorithm(&data, "Zstd (level 1)", CompressionAlgorithm::Zstd(1))?;
    test_algorithm(&data, "Zstd (level 3)", CompressionAlgorithm::Zstd(3))?;
    test_algorithm(&data, "Zstd (level 5)", CompressionAlgorithm::Zstd(5))?;
    test_algorithm(&data, "Zstd (level 10)", CompressionAlgorithm::Zstd(10))?;
    test_algorithm(&data, "LZMA2 (level 3)", CompressionAlgorithm::Lzma2(3))?;
    test_algorithm(&data, "LZMA2 (level 6)", CompressionAlgorithm::Lzma2(6))?;
    test_algorithm(&data, "LZMA2 (level 9)", CompressionAlgorithm::Lzma2(9))?;
    
    println!("\n─────────────────────────────────────────────────────────────");
    println!("Recommendation:");
    println!("─────────────────────────────────────────────────────────────\n");
    
    // Analyze results and recommend
    let lz4_result = ChunkCompressor::compress(&data, CompressionAlgorithm::Lz4)?;
    let zstd3_result = ChunkCompressor::compress(&data, CompressionAlgorithm::Zstd(3))?;
    let lzma2_result = ChunkCompressor::compress(&data, CompressionAlgorithm::Lzma2(6))?;
    
    if lz4_result.ratio > 0.95 && zstd3_result.ratio > 0.95 {
        println!("❌ SKIP COMPRESSION for this file");
        println!("   Both LZ4 and Zstd achieved <5% reduction");
        println!("   Compression overhead not worth it\n");
        println!("   → Use CompressionAlgorithm::None");
    } else if lz4_result.ratio < 0.90 {
        println!("✅ USE LZ4");
        println!("   Fast compression with good ratio ({:.1}% saved)", 
            (1.0 - lz4_result.ratio) * 100.0);
        println!("   Best for: Real-time transfer, interactive applications\n");
        println!("   → Use CompressionAlgorithm::Lz4");
    } else if zstd3_result.ratio < 0.85 {
        println!("✅ USE ZSTD (level 3)");
        println!("   Balanced compression ({:.1}% saved)", 
            (1.0 - zstd3_result.ratio) * 100.0);
        println!("   Best for: Most file transfers\n");
        println!("   → Use CompressionAlgorithm::Zstd(3)");
    } else if lzma2_result.ratio < 0.70 {
        println!("✅ USE LZMA2 (level 6-9)");
        println!("   Maximum compression ({:.1}% saved)", 
            (1.0 - lzma2_result.ratio) * 100.0);
        println!("   Best for: Archival, large files where compression ratio matters most\n");
        println!("   → Use CompressionAlgorithm::Lzma2(6)");
    } else {
        println!("✅ USE ZSTD (level 5-10)");
        println!("   High compression for larger files");
        println!("   Best for: Large files, batch transfers\n");
        println!("   → Use CompressionAlgorithm::Zstd(5)");
    }
    
    Ok(())
}

fn test_algorithm(
    data: &[u8],
    name: &str,
    algorithm: CompressionAlgorithm,
) -> Result<(), Box<dyn std::error::Error>> {
    let original_size = data.len();
    
    // Compress
    let start = std::time::Instant::now();
    let compressed = ChunkCompressor::compress(data, algorithm)?;
    let compress_time = start.elapsed();
    
    // Decompress
    let start = std::time::Instant::now();
    let decompressed = ChunkCompressor::decompress(
        &compressed.compressed_data,
        algorithm,
        Some(original_size),
    )?;
    let decompress_time = start.elapsed();
    
    // Verify
    assert_eq!(decompressed.len(), original_size);
    
    let ratio = compressed.compressed_size as f64 / original_size as f64;
    let savings = ((1.0 - ratio) * 100.0).max(0.0);
    let compress_speed = (original_size as f64 / compress_time.as_secs_f64()) / 1_048_576.0;
    let decompress_speed = (original_size as f64 / decompress_time.as_secs_f64()) / 1_048_576.0;
    
    let indicator = if ratio < 0.90 {
        "✓ Good"
    } else if ratio < 0.95 {
        "○ Okay"
    } else {
        "✗ Poor"
    };
    
    println!("{:<18} │ {:>8} → {:>8} bytes │ {:>6.1}% size │ {:>5.1}% saved │ {:>7.1} MB/s ↑ │ {:>7.1} MB/s ↓ │ {}",
        name,
        original_size,
        compressed.compressed_size,
        ratio * 100.0,
        savings,
        compress_speed,
        decompress_speed,
        indicator,
    );
    
    Ok(())
}

fn should_compress_file_type(extension: &str) -> (bool, &'static str) {
    match extension {
        // Already compressed - DO NOT compress
        "mp4" | "mkv" | "avi" | "mov" | "webm" => (false, "Video files are already heavily compressed (H.264/H.265)"),
        "mp3" | "aac" | "ogg" | "flac" | "m4a" => (false, "Audio files are already compressed"),
        "jpg" | "jpeg" | "png" | "gif" | "webp" => (false, "Images are already compressed"),
        "zip" | "gz" | "bz2" | "7z" | "rar" | "xz" => (false, "Archive files are already compressed"),
        "pdf" => (false, "PDFs typically contain compressed images and data"),
        
        // Good compression candidates
        "txt" | "log" => (true, "Text files compress very well (70-90% reduction)"),
        "json" | "xml" | "yaml" | "toml" => (true, "Structured text compresses extremely well (80-95% reduction)"),
        "csv" | "tsv" => (true, "Tabular data compresses well (70-85% reduction)"),
        "html" | "htm" | "css" | "js" => (true, "Web files compress well (60-80% reduction)"),
        "rs" | "c" | "cpp" | "h" | "java" | "py" => (true, "Source code compresses well (65-80% reduction)"),
        "md" | "rst" => (true, "Markdown compresses well (70-85% reduction)"),
        "sql" => (true, "SQL files compress well (70-85% reduction)"),
        
        // Binary - may compress
        "bin" | "dat" => (true, "Binary files may compress moderately (30-60% reduction)"),
        "exe" | "dll" | "so" => (true, "Executables may compress (20-50% reduction)"),
        
        _ => (true, "Unknown type - test compression to determine benefit"),
    }
}
