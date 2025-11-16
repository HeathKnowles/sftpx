# Connection Migration & Heartbeat Implementation

## Summary

Successfully implemented **connection migration support** and **keepalive/heartbeat** functionality for both client and server QUIC connections.

## Features Implemented

### 1. Connection Migration Support

#### Client (`src/client/connection.rs`)
- **Migration State Tracking**:
  - `migration_enabled: bool` - Enable/disable migration (default: enabled)
  - `original_peer_addr: SocketAddr` - Track original server address
  
- **Migration Methods**:
  - `is_migration_enabled()` - Check if migration is enabled
  - `set_migration_enabled(bool)` - Enable/disable migration
  - `migrate_to_address(new_local_addr)` - Migrate to new local address (e.g., WiFi → cellular)
  - `has_peer_migrated(current_peer)` - Detect if server has migrated
  - `update_peer_address(new_peer)` - Update peer address after migration
  - `original_peer_addr()` - Get original peer address

#### Server (`src/server/connection.rs`)
- **Migration Detection**:
  - `original_peer_addr: SocketAddr` - Original client address
  - `migration_count: usize` - Number of times client has migrated
  - Automatic detection in `process_packet()` when client address changes
  
- **Migration Methods**:
  - `original_peer_addr()` - Get original peer address
  - `migration_count()` - Get number of migrations
  - `has_migrated()` - Check if peer has migrated

### 2. Keepalive/Heartbeat Support

#### Constants (`src/common/types.rs`)
- `HEARTBEAT_INTERVAL` = 30 seconds - Send heartbeat every 30s
- `KEEPALIVE_IDLE_THRESHOLD` = 60 seconds - Consider connection idle after 60s

#### Client (`src/client/connection.rs`)
- **Heartbeat State**:
  - `last_heartbeat: Instant` - Track last heartbeat time
  - `last_activity: Instant` - Track last activity time
  
- **Heartbeat Methods**:
  - `should_send_heartbeat()` - Returns true if 30s elapsed since last heartbeat
  - `send_heartbeat()` - Send PING on control stream (stream 0)
  - `is_idle()` - Check if connection idle for 60s+
  - `idle_duration()` - Get time since last activity
  - `time_since_heartbeat()` - Get time since last heartbeat
  - `handle_heartbeat(data)` - Process PING/PONG messages

#### Server (`src/server/connection.rs`)
- **Heartbeat State**:
  - `last_activity: Instant` - Automatically updated on send/recv
  - `last_heartbeat: Instant` - Track heartbeat timing
  
- **Heartbeat Methods**:
  - `should_send_heartbeat()` - Check if heartbeat needed
  - `send_heartbeat()` - Send PING on control stream
  - `is_idle()` - Check if connection idle for 60s+
  - `idle_duration()` - Get idle duration
  - `time_since_heartbeat()` - Get time since last heartbeat
  - `handle_heartbeat(data)` - Auto-respond to PING with PONG

## Protocol Details

### Heartbeat Messages
- **PING**: 4-byte message `b"PING"` sent on stream 0 (control)
- **PONG**: 4-byte message `b"PONG"` sent as response
- Messages sent with `fin=false` to keep stream open

### Activity Tracking
- Both client and server update `last_activity` on every packet send/receive
- Heartbeats update `last_heartbeat` timestamp
- Applications can use `is_idle()` to detect stale connections

### Migration Flow

**Client Migration** (network interface change):
1. Client detects network change (WiFi → cellular)
2. Calls `migrate_to_address(new_local_addr)`
3. QUIC handles path validation automatically
4. Continue sending/receiving from new address

**Server Detection**:
1. Server receives packet from different address
2. `process_packet()` detects address change
3. Increments `migration_count`
4. Updates `peer_addr` to new address
5. Logs migration event

## Usage Examples

### Client Heartbeat
```rust
// In main transfer loop
if connection.should_send_heartbeat() {
    if let Err(e) = connection.send_heartbeat() {
        log::warn!("Heartbeat failed: {:?}", e);
    }
}

// Check for idle connection
if connection.is_idle() {
    log::warn!("Connection idle for {:?}", connection.idle_duration());
}
```

### Client Migration
```rust
// When network interface changes
let new_local_addr = "192.168.2.100:0".parse()?;
connection.migrate_to_address(new_local_addr)?;
```

### Server Heartbeat Response
```rust
// When receiving data on control stream
let mut buf = [0u8; 1024];
if let Ok((len, _fin)) = connection.stream_recv(0, &mut buf) {
    if connection.handle_heartbeat(&buf[..len]) {
        // Heartbeat handled, continue
    } else {
        // Process as regular control message
    }
}
```

### Server Migration Detection
```rust
// After processing packets
if connection.has_migrated() {
    log::info!("Client migrated {} times", connection.migration_count());
    log::info!("Original: {:?}, Current: {:?}", 
        connection.original_peer_addr(),
        connection.peer_addr());
}
```

## Testing

Added unit tests in both modules:
- `client::connection::tests::test_heartbeat_timing` - Verify constants
- `client::connection::tests::test_heartbeat_messages` - Verify message format
- `server::connection::tests::test_migration_tracking` - Placeholder for migration tests
- `server::connection::tests::test_heartbeat_methods` - Verify message handling

## Implementation Status

✅ **Fully Implemented**:
1. QUIC Client - Full connection management
2. QUIC Server - Full connection management
3. Stream 0 (Control) - Highest priority
4. Stream 1 (Manifest) - File metadata
5. Stream 2 (Data) - File chunks
6. Stream 3 (Status) - Transfer status
7. Session ID Generator - Cryptographic random IDs
8. **Connection Migration** - Enable/disable, detection, tracking
9. **Keepalive/Heartbeat** - PING/PONG, idle detection, auto-response

## Notes

- QUIC's connection migration is enabled by default (`set_disable_active_migration(false)`)
- Path validation is handled automatically by the QUIC implementation (quiche)
- Heartbeats use the control stream (stream 0) to avoid interfering with data streams
- The 30-second heartbeat interval keeps connections alive well within the 300-second idle timeout
- Server automatically responds to PING with PONG when using `handle_heartbeat()`
- Migration detection is automatic on the server side
- Activity timestamps are updated on every packet, not just heartbeats
