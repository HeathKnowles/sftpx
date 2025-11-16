// QUIC 4-Stream Architecture Documentation

/*
 * STREAM ARCHITECTURE
 * ===================
 * 
 * This implementation uses 4 bidirectional QUIC streams for parallel data transfer.
 * Each stream is assigned a unique ID and priority level.
 * 
 * Stream IDs follow QUIC convention:
 * - Client-initiated bidirectional streams: 0, 4, 8, 12, 16...
 * - Formula: stream_id = 4 * n (where n = 0, 1, 2, 3...)
 * 
 * STREAM DEFINITIONS
 * ==================
 */

// STREAM 0: Control Stream (Highest Priority)
// --------------------------------------------
// Purpose: Protocol control messages, metadata, handshake
// Priority: urgency=0, incremental=false
// Data Types:
//   - TransferRequest (file info, chunk size, etc.)
//   - TransferResponse (session ID, acceptance)
//   - ControlCommand (pause, resume, cancel)
//   - StatusUpdate (progress, errors)
pub const STREAM_CONTROL: u64 = 0;

// STREAM 4: Data Stream 1 (Medium Priority)
// ------------------------------------------
// Purpose: Primary data transfer channel
// Priority: urgency=3, incremental=true
// Data Types:
//   - ChunkData (chunk_id, offset, data bytes)
//   - ChunkAck (acknowledgment)
pub const STREAM_DATA1: u64 = 4;

// STREAM 8: Data Stream 2 (Medium Priority)
// ------------------------------------------
// Purpose: Secondary data transfer channel
// Priority: urgency=3, incremental=true
// Data Types:
//   - ChunkData (parallel to stream 4)
//   - ChunkAck
pub const STREAM_DATA2: u64 = 8;

// STREAM 12: Data Stream 3 (Medium Priority)
// -------------------------------------------
// Purpose: Tertiary data transfer channel
// Priority: urgency=3, incremental=true
// Data Types:
//   - ChunkData (parallel to streams 4 & 8)
//   - ChunkAck
pub const STREAM_DATA3: u64 = 12;

/*
 * PRIORITY EXPLANATION
 * ====================
 * 
 * QUIC HTTP/3 Priority (RFC 9218):
 * - urgency: 0-7, where 0 is highest priority
 * - incremental: whether data can be processed incrementally
 * 
 * Control Stream (urgency=0, incremental=false):
 * - Must be delivered before data streams
 * - Non-incremental: entire message needed before processing
 * 
 * Data Streams (urgency=3, incremental=true):
 * - Medium priority, after control
 * - Incremental: chunks can be processed as they arrive
 * - Equal priority among data streams for fair bandwidth sharing
 * 
 * TRANSFER FLOW
 * =============
 * 
 * 1. HANDSHAKE PHASE
 *    Client establishes QUIC connection
 *    TLS handshake completes
 *    All 4 streams initialized with priorities
 * 
 * 2. CONTROL PHASE
 *    Client → Server: TransferRequest on STREAM_CONTROL
 *    Server → Client: TransferResponse on STREAM_CONTROL
 *    Session established
 * 
 * 3. DATA TRANSFER PHASE
 *    Client distributes chunks across STREAM_DATA1/2/3:
 *      - Round-robin distribution
 *      - Or: Priority-based (retransmissions on DATA1)
 *      - Or: Size-based (large chunks on DATA1)
 *    
 *    Example chunk distribution:
 *      Chunk 0 → STREAM_DATA1
 *      Chunk 1 → STREAM_DATA2
 *      Chunk 2 → STREAM_DATA3
 *      Chunk 3 → STREAM_DATA1
 *      ...
 * 
 * 4. ACKNOWLEDGMENT PHASE
 *    Server sends ChunkAck on respective data streams
 *    Client tracks acknowledged chunks in session bitmap
 * 
 * 5. COMPLETION PHASE
 *    All chunks acknowledged
 *    Client → Server: TransferComplete on STREAM_CONTROL
 *    Server validates file integrity
 *    Connection closes gracefully
 * 
 * ERROR HANDLING
 * ==============
 * 
 * Stream-level errors:
 *   - STREAM_SEND_STOPPED: peer stopped reading
 *   - STREAM_RECV_STOPPED: peer stopped sending
 *   - Action: Retry on different stream or fail transfer
 * 
 * Connection-level errors:
 *   - IDLE_TIMEOUT: no activity for timeout period
 *   - TLS_ERROR: certificate validation failed
 *   - PROTOCOL_VIOLATION: invalid packet
 *   - Action: Save session state, attempt reconnect
 * 
 * IMPLEMENTATION NOTES
 * ====================
 * 
 * See implementation files:
 * - src/client/connection.rs: QUIC connection wrapper
 * - src/client/streams.rs: Stream manager (this architecture)
 * - src/client/transfer.rs: Event loop implementing the flow
 * 
 * Key implementation details:
 * 1. Streams pre-registered at connection start
 * 2. Priority set before first send on each stream
 * 3. Send operations check stream.is_writable()
 * 4. Receive uses conn.readable() iterator
 * 5. Stats tracked per-stream and aggregated
 * 
 * PERFORMANCE CONSIDERATIONS
 * ==========================
 * 
 * Benefits of 4-stream architecture:
 * - Head-of-line blocking limited to single stream
 * - Parallel transmission utilizes bandwidth better
 * - Lost packets on one stream don't block others
 * - Control messages never blocked by data
 * 
 * Tuning parameters:
 * - chunk_size: 1MB default (tune based on RTT)
 * - max_stream_data: 10MB window per stream
 * - max_data: 100MB total connection window
 * - datagram_size: 1350 bytes (MTU - overhead)
 * 
 * FUTURE ENHANCEMENTS
 * ===================
 * 
 * - Dynamic stream count based on bandwidth
 * - Adaptive chunk size based on RTT
 * - FEC (Forward Error Correction) on data streams
 * - Compression on control stream
 * - Priority boost for retransmissions
 */
