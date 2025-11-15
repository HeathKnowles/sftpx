fn main() {
    prost_build::compile_protos(
        &["src/proto/user.proto"],
        &["src/proto"],
    ).unwrap();
}
