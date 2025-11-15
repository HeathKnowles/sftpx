// Integration test for resume functionality
// This demonstrates the end-to-end resume protocol

use sftpx::chunking::ChunkBitmap;

#[test]
fn test_resume_bitmap_workflow() {
    // Simulate client-side bitmap tracking during upload
    let temp_dir = std::env::temp_dir();
    let session_id = "test-session-123";
    let bitmap_path = temp_dir.join(format!("{}.bitmap", session_id));
    
    // Phase 1: Initial upload (interrupted at 60%)
    {
        let total_chunks = 500;
        let mut bitmap = ChunkBitmap::with_exact_size(total_chunks);
        
        // Simulate sending first 300 chunks
        for chunk_idx in 0..300 {
            bitmap.mark_received(chunk_idx, chunk_idx == total_chunks - 1);
            
            // Save periodically (every 10 chunks)
            if chunk_idx % 10 == 0 {
                bitmap.save_to_disk(&bitmap_path).unwrap();
            }
        }
        
        // Final save before "crash"
        bitmap.save_to_disk(&bitmap_path).unwrap();
        
        assert_eq!(bitmap.received_count(), 300);
        assert!(!bitmap.is_complete());
        println!("✓ Phase 1: Uploaded 300/500 chunks (60%)");
    }
    
    // Phase 2: Resume detection and continuation
    {
        // Load bitmap from disk
        let bitmap = ChunkBitmap::load_from_disk(&bitmap_path).unwrap();
        
        assert_eq!(bitmap.received_count(), 300);
        println!("✓ Phase 2: Loaded bitmap, {} chunks already received", bitmap.received_count());
        
        // Find missing chunks
        let missing = bitmap.find_missing();
        assert_eq!(missing.len(), 200); // Chunks 300-499
        assert_eq!(missing[0], 300);
        assert_eq!(missing[missing.len() - 1], 499);
        println!("✓ Phase 2: Found {} missing chunks", missing.len());
        
        // Simulate resume: send only missing chunks
        let mut resumed_bitmap = ChunkBitmap::with_exact_size(500);
        // First mark all previously received chunks
        for chunk_idx in 0..300 {
            resumed_bitmap.mark_received(chunk_idx, false);
        }
        // Then mark the missing chunks we're now sending
        for &chunk_idx in &missing {
            resumed_bitmap.mark_received(chunk_idx, chunk_idx == 499);
        }
        
        assert!(resumed_bitmap.is_complete());
        assert_eq!(resumed_bitmap.received_count(), 500);
        println!("✓ Phase 2: All chunks received after resume");
    }
    
    // Phase 3: Cleanup
    std::fs::remove_file(&bitmap_path).unwrap();
    println!("✓ Phase 3: Cleaned up bitmap file");
}

#[test]
fn test_resume_skip_set_generation() {
    // Test the skip set generation logic
    use std::collections::HashSet;
    
    let total_chunks = 100u64;
    let received_chunks = vec![0, 1, 2, 10, 11, 12, 50, 51, 52];
    
    // Build skip set (chunks NOT in missing list)
    let missing_chunks = (0..total_chunks)
        .filter(|idx| !received_chunks.contains(idx))
        .collect::<Vec<_>>();
    
    let missing_set: HashSet<u64> = missing_chunks.iter().copied().collect();
    let mut skip_chunks = HashSet::new();
    
    for chunk_idx in 0..total_chunks {
        if !missing_set.contains(&chunk_idx) {
            skip_chunks.insert(chunk_idx);
        }
    }
    
    assert_eq!(skip_chunks.len(), received_chunks.len());
    assert!(skip_chunks.contains(&0));
    assert!(skip_chunks.contains(&11));
    assert!(skip_chunks.contains(&52));
    assert!(!skip_chunks.contains(&3));
    assert!(!skip_chunks.contains(&99));
    
    println!("✓ Skip set correctly identifies {} chunks to skip", skip_chunks.len());
    println!("✓ Will send {} missing chunks", missing_chunks.len());
}

#[test]
fn test_resume_protocol_messages() {
    use sftpx::protocol::messages::{ResumeRequest, ResumeResponse};
    
    // Test ResumeRequest encoding/decoding
    let request = ResumeRequest {
        session_id: "test-session".to_string(),
        received_chunks: vec![0, 1, 2, 3, 4],
        received_bitmap: Some(vec![0xFF, 0x00]),
        last_chunk_id: Some(4),
    };
    
    let encoded = request.encode_to_vec();
    let decoded = ResumeRequest::decode_from_bytes(&encoded).unwrap();
    
    assert_eq!(decoded.session_id, "test-session");
    assert_eq!(decoded.received_chunks, vec![0, 1, 2, 3, 4]);
    assert_eq!(decoded.last_chunk_id, Some(4));
    println!("✓ ResumeRequest serialization works");
    
    // Test ResumeResponse encoding/decoding
    let response = ResumeResponse {
        session_id: "test-session".to_string(),
        accepted: true,
        missing_chunks: vec![5, 6, 7, 8, 9],
        chunks_remaining: 5,
        error: None,
    };
    
    let encoded = response.encode_to_vec();
    let decoded = ResumeResponse::decode_from_bytes(&encoded).unwrap();
    
    assert_eq!(decoded.session_id, "test-session");
    assert!(decoded.accepted);
    assert_eq!(decoded.missing_chunks, vec![5, 6, 7, 8, 9]);
    assert_eq!(decoded.chunks_remaining, 5);
    println!("✓ ResumeResponse serialization works");
}
