use anyhow::{Context, Result};
use blake3::{Hasher};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{BufReader, Read};

pub const CHUNK_SIZE: usize = 100 * 1024 * 1024;
pub const KEYED_HASH_LEN: usize = 32;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkEntry {
    pub index: u64,
    pub offset: u64,
    pub len: usize,
    pub hash_hex: String,
    pub metro_hash_hex: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkTable {
    pub file_size: u64,
    pub chunk_size: usize,
    pub entries: Vec<ChunkEntry>,
    pub root_hash_hex: String,
}

impl ChunkTable {
    pub fn to_json_pretty(&self) -> Result<String> {
        Ok(serde_json::to_string_pretty(self)?)
    }
}

#[derive(Debug, Clone, Copy)]
pub enum Mode {
    Default,
    Metros,
}

impl Mode {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "metros" => Mode::Metros,
            _ => Mode::Default,
        }
    }
}

pub fn chunk_file(path: &str, mode: Mode) -> Result<ChunkTable> {
    let file = File::open(path).with_context(|| format!("opening file: {}", path))?;
    let metadata = file.metadata().with_context(|| "getting file metadata")?;
    let file_size = metadata.len();

    let mut reader = BufReader::new(file);
    let mut entries = Vec::new();
    let mut offset: u64 = 0;
    let mut index: u64 = 0;

    let mut buffer = vec![0u8; CHUNK_SIZE];

    let mut chunk_hash_bytes_concat = Vec::new();

    while offset < file_size {
        let to_read = std::cmp::min(CHUNK_SIZE as u64, file_size - offset) as usize;

        let mut read_total = 0usize;
        while read_total < to_read {
            let n = reader.read(&mut buffer[read_total..to_read])?;
            if n == 0 {
                break;
            }
            read_total += n;
        }

        let chunk_bytes = &buffer[..read_total];

        // Compute normal blake3 hash
        let mut hasher = Hasher::new();
        hasher.update(chunk_bytes);
        let hash = hasher.finalize();
        let hash_hex = hex::encode(hash.as_bytes());
        chunk_hash_bytes_concat.extend_from_slice(hash.as_bytes());

        // Optional metro hash (keyed blake3)
        let metro_hash_hex = if let Mode::Metros = mode {
            let label = b"metros-mode-key-v1-change-me";
            let mut key_hasher = Hasher::new();
            key_hasher.update(label);
            let derived_key = key_hasher.finalize();

            let keyed = blake3::keyed_hash(
                derived_key.as_bytes()[..KEYED_HASH_LEN].try_into().unwrap(),
                chunk_bytes,
            );

            Some(hex::encode(keyed.as_bytes()))
        } else {
            None
        };

        entries.push(ChunkEntry {
            index,
            offset,
            len: read_total,
            hash_hex,
            metro_hash_hex,
        });

        offset += read_total as u64;
        index += 1;
    }

    // Root hash = blake3(hash1 || hash2 || ...)
    let mut root_hasher = Hasher::new();
    root_hasher.update(&chunk_hash_bytes_concat);
    let root_hash = root_hasher.finalize();

    Ok(ChunkTable {
        file_size,
        chunk_size: CHUNK_SIZE,
        entries,
        root_hash_hex: hex::encode(root_hash.as_bytes()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_default_chunking() {
        let mut f = NamedTempFile::new().unwrap();
        let data = vec![42u8; 1024 * 1024];
        f.write_all(&data).unwrap();

        let table = chunk_file(f.path().to_str().unwrap(), Mode::Default).unwrap();
        assert_eq!(table.entries.len(), 1);
        assert!(table.entries[0].metro_hash_hex.is_none());
    }

    #[test]
    fn test_metros_chunking() {
        let mut f = NamedTempFile::new().unwrap();
        let data = vec![1u8; 5 * 1024 * 1024];
        f.write_all(&data).unwrap();

        let table = chunk_file(f.path().to_str().unwrap(), Mode::Metros).unwrap();
        assert_eq!(table.entries.len(), 1);
        assert!(table.entries[0].metro_hash_hex.is_some());
    }
}
