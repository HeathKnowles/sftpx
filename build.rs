// build.rs - Build script for protobuf and flatbuffer compilation

fn main() {
    // Protobuf compilation
    // Uncomment when proto files are ready
    /*
    let proto_files = [
        "proto/session.proto",
        "proto/manifest.proto",
        "proto/chunk.proto",
        "proto/resume.proto",
        "proto/status.proto",
        "proto/transfer.proto",
        "proto/control.proto",
        "proto/bitmap.proto",
        "proto/common.proto",
    ];

    let mut config = prost_build::Config::new();
    config.out_dir("src/protocol/generated");
    
    config
        .compile_protos(&proto_files, &["proto/"])
        .expect("Failed to compile protobuf files");
    */

    // FlatBuffers compilation
    // Uncomment when flatbuffer schemas are ready
    /*
    let flatbuffer_files = [
        "flatbuffers/chunk_data.fbs",
        "flatbuffers/manifest_table.fbs",
        "flatbuffers/bitmap.fbs",
    ];

    for file in &flatbuffer_files {
        println!("cargo:rerun-if-changed={}", file);
    }
    */

    println!("cargo:rerun-if-changed=build.rs");
}
