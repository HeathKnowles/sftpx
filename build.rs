// build.rs - Build script for protobuf and flatbuffer compilation

fn main() {
    // Protobuf compilation
    let proto_files = [
        "proto/chunk.proto",
    ];

    // Create output directory for generated code
    std::fs::create_dir_all("src/proto").expect("Failed to create output directory");

    let mut config = prost_build::Config::new();
    config.out_dir("src/proto");
    
    config
        .compile_protos(&proto_files, &["proto/"])
        .expect("Failed to compile protobuf files");

    // FlatBuffers compilation
    use std::process::Command;
    
    let flatbuffer_files = [
        "flatbuffers/chunk_data.fbs",
    ];

    // Create output directory for generated code
    let out_dir = "src/proto";
    std::fs::create_dir_all(out_dir).expect("Failed to create output directory");

    // Compile each FlatBuffer schema
    for file in &flatbuffer_files {
        println!("cargo:rerun-if-changed={}", file);
        
        let status = Command::new("flatc")
            .args(&["--rust", "-o", out_dir, file])
            .status();
        
        match status {
            Ok(s) if s.success() => {
                println!("Successfully compiled {}", file);
            }
            Ok(s) => {
                eprintln!("flatc failed with status: {}", s);
                eprintln!("Note: Install flatc with 'sudo apt install flatbuffers-compiler' or similar");
            }
            Err(e) => {
                eprintln!("Failed to run flatc: {}", e);
                eprintln!("Note: Install flatc with 'sudo apt install flatbuffers-compiler' or similar");
            }
        }
    }

    println!("cargo:rerun-if-changed=build.rs");
}
