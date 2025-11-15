#!/bin/bash
echo "Testing file-type-based compression:"
echo ""

# Text files - should use Zstd
echo "=== TEXT FILES (Zstd) ==="
cargo run -q --example complete_chunking_pipeline test.txt 2>&1 | grep -E "Extension|Compression:" | head -2
cargo run -q --example complete_chunking_pipeline test.log 2>&1 | grep -E "Extension|Compression:" | head -2
cargo run -q --example complete_chunking_pipeline test.json 2>&1 | grep -E "Extension|Compression:" | head -2
echo ""

# Video files - should use None
echo "=== VIDEO FILES (None - already HEVC/H.264) ==="
cargo run -q --example complete_chunking_pipeline test.mp4 2>&1 | grep -E "Extension|Compression:" | head -2
cargo run -q --example complete_chunking_pipeline test.mkv 2>&1 | grep -E "Extension|Compression:" | head -2
echo ""

# Binary files - should use LZ4HC
echo "=== BINARY FILES (LZ4HC) ==="
cargo run -q --example complete_chunking_pipeline test.bin 2>&1 | grep -E "Extension|Compression:" | head -2
