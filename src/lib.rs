pub mod pb {
    include!(concat!(env!("OUT_DIR"), "/chunk.rs"));
}

pub mod chunk;
pub mod compression;

// Export functions so tests can call them
pub use crate::chunk::chunk_file_to_pb;
pub use crate::chunk::serialize_chunk_table;
pub use crate::chunk::deserialize_chunk_table;
pub use crate::chunk::reconstruct_file;
