pub mod chunker;
pub mod hasher;
pub mod bitmap;
pub mod table;
pub mod compress;
pub mod dedup;

pub use chunker::{FileChunker, ChunkIterator};
pub use hasher::ChunkHasher;
pub use bitmap::ChunkBitmap;
pub use table::{ChunkTable, ChunkMetadata};
pub use compress::{
    CompressionType, ChunkCompressor, NoneCompressor, ZstdCompressor, 
    create_compressor, compress_chunk, decompress_chunk
};
pub use dedup::{ChunkHashIndex, ChunkLocation, DedupStats};