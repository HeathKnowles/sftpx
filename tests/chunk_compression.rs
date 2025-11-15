use sftpx::chunk::*;
use sftpx::pb::*;

use std::fs;

#[test]
fn test_chunk_compress_and_reassemble() {
    let input_path = "test_input.bin";
    let output_path = "test_output.bin";

    // 1. Read original file
    let original_data = fs::read(input_path).expect("failed to read input");

    // 2. Chunk the file -> protobuf ChunkTable
    let table = chunk_file_to_pb(input_path).expect("chunking failed");

    assert!(table.chunks.len() > 0, "no chunks produced");

    // 3. Serialize
    let serialized = serialize_chunk_table(&table);

    assert!(serialized.len() > 0, "protobuf serialization empty");

    // 4. Deserialize
    let deserialized: ChunkTable =
        deserialize_chunk_table(&serialized).expect("deserialize failed");

    // 5. Reassemble file from protobuf chunks
    reconstruct_file(&deserialized, output_path).expect("reconstruction failed");

    // 6. Compare original vs reconstructed
    let reconstructed_data = fs::read(output_path).expect("failed to read reconstructed");

    assert_eq!(
        original_data, reconstructed_data,
        "Reconstructed file does not match original!"
    );

    println!("Chunks produced: {}", table.chunks.len());
    println!("Chunk test passed!");
}
