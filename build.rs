fn main() {
    prost_build::compile_protos(&["proto/chunk.proto"], &["proto"])
        .expect("Failed to compile protobufs");
}
