# gRPC Stream Cancellation vs Network Disconnection Test

A comprehensive Rust server and Python client implementation to **test and distinguish between gRPC stream cancellation and network disconnection** scenarios.

## Purpose

This project demonstrates and tests three key gRPC streaming scenarios:
- **✅ Normal completion**: Server sends all messages and closes stream gracefully
- **🚫 Intentional cancellation**: Client calls `call.cancel()` → RST_STREAM frame
- **🔌 Network disconnection**: Connection lost due to network issues

## Architecture

### Components
- **Rust Server** (`rust-server/`): Streaming gRPC server with message generation
- **Python Client** (`python-client/`): Test client with multiple modes
- **Rust Proxy** (`rust-proxy/`): Optional proxy for network disconnection tests
- **Test Scripts**: Automated test scenarios

### Key Features
- **Real-time message streaming**: Server generates messages at configurable intervals
- **Channel-based buffering**: 10-message buffer with automatic backpressure
- **Graceful completion**: Server closes stream after sending all messages
- **Cancellation detection**: Distinguishes intentional vs accidental disconnections
- **Comprehensive logging**: Detailed status reporting for all scenarios

## Test Scenarios

### 1. Normal Completion Test
```bash
./test_disconnect.sh
```
**Expected behavior**:
- Server sends 10 messages (1 per second)
- Client receives all messages
- Server closes stream gracefully
- Client detects normal completion
- Both server and client terminate cleanly

### 2. Intentional Cancellation Test
```bash
./test_cancel.sh
```
**Expected behavior**:
- Client connects and receives messages
- After 3 seconds, client calls `call.cancel()`
- Server detects RST_STREAM (CANCELLED status)
- Server performs immediate cleanup
- Server terminates automatically

### 3. Network Disconnection Test (with Proxy)
```bash
# Manual network interruption test
# 1. Start server and proxy
# 2. Connect client through proxy
# 3. Kill proxy to simulate network failure
# 4. Observe reconnection behavior
```

## Quick Start

### Prerequisites
```bash
# Rust (for server)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Python (for client)
pip install grpcio grpcio-tools
```

### Running Tests

#### Test Normal Completion
```bash
cd grpc-stream-cancel
./test_disconnect.sh
```

#### Test Intentional Cancellation
```bash
cd grpc-stream-cancel
./test_cancel.sh
```

#### Manual Testing
```bash
# Terminal 1: Start server
cd rust-server
cargo run --release 1  # 1-second intervals

# Terminal 2: Run client
cd python-client
python client.py --mode simple          # Normal completion
python client.py --mode auto_cancel --delay 3.0  # Cancel after 3s
```

## Expected Output

### Normal Completion
```
🚀 [RUST SERVER] Starting gRPC server...
📤 [RUST SERVER] Starting real-time message generation...
🔗 [PYTHON CLIENT] Establishing bidirectional stream...
📨 [PYTHON CLIENT] Received message 1: Message 1 from server
📨 [PYTHON CLIENT] Received message 2: Message 2 from server
...
📨 [PYTHON CLIENT] Received message 10: Message 10 from server
🎉 [RUST SERVER] All messages generated!
🏁 [RUST SERVER] Closing stream - all messages sent
✅ [PYTHON CLIENT] Stream completed by server. Total: 10 messages
   → Server finished sending all messages and closed the stream
```

### Intentional Cancellation
```
🚀 [RUST SERVER] Starting gRPC server...
⏰ [PYTHON CLIENT] AUTO CANCEL MODE: Will cancel after 3.0 seconds
📨 [PYTHON CLIENT] Received message 1: Message 1 from server
📨 [PYTHON CLIENT] Received message 2: Message 2 from server
⏰ [PYTHON CLIENT] Auto-cancel triggered after 3.1s (delay: 3.0s)
📤 [PYTHON CLIENT] Calling gRPC cancel() → RST_STREAM
🚫 [RUST SERVER] CANCELLED: Client called cancel() → RST_STREAM sent
🚫 [RUST SERVER] INTENTIONAL CANCELLATION:
   - Client called cancel() explicitly
   - Performing immediate cleanup
```

## File Structure

```
grpc-stream-cancel/
├── README.md                    # This file
├── proto/
│   └── streaming.proto         # Protocol buffer definition
├── rust-server/
│   ├── Cargo.toml
│   ├── build.rs               # Proto compilation
│   └── src/main.rs            # Server implementation
├── rust-proxy/
│   ├── Cargo.toml
│   └── src/main.rs            # Proxy for network tests
├── python-client/
│   ├── requirements.txt
│   ├── generate_protos.py     # Proto generation
│   ├── client.py              # Client implementation
│   ├── streaming_pb2.py       # Generated proto
│   └── streaming_pb2_grpc.py  # Generated gRPC
├── test_disconnect.sh         # Normal completion test
└── test_cancel.sh            # Cancellation test
```

## Protocol Buffer Schema

```protobuf
syntax = "proto3";

message DataMessage {
    int32 id = 1;
    int64 timestamp = 2;
    string payload = 3;
}

service StreamingService {
    rpc BidirectionalStream(stream DataMessage) returns (stream DataMessage);
}
```

## Client Modes

### `simple` Mode (Default)
- Connects to server and receives all messages
- Handles reconnection on network failures
- Terminates when server completes

### `auto_cancel` Mode
- Automatically calls `call.cancel()` after specified delay
- Tests intentional cancellation scenario
- Configurable delay with `--delay` parameter

```bash
python client.py --mode auto_cancel --delay 5.0  # Cancel after 5 seconds
```

## Server Configuration

The server accepts one optional parameter for message interval:

```bash
cargo run --release [interval_seconds]

# Examples:
cargo run --release 1    # 1-second intervals (fast)
cargo run --release 2    # 2-second intervals (default)
cargo run --release 5    # 5-second intervals (slow)
```

## Key Verification Points

### ✅ What This Implementation Tests:
- **Stream lifecycle**: Normal start → message exchange → graceful completion
- **Cancellation handling**: Client-initiated cancellation with proper cleanup
- **Error distinction**: CANCELLED vs UNAVAILABLE vs other gRPC status codes
- **Resource cleanup**: Proper task termination and channel closure
- **Message delivery**: Guaranteed delivery of buffered messages
- **Timing accuracy**: Precise cancellation timing and message intervals

### 🔍 gRPC Mechanisms Demonstrated:
- **Bidirectional streaming**: Full-duplex communication
- **Backpressure handling**: Channel buffering with automatic flow control
- **Status codes**: Proper gRPC error code usage and interpretation
- **Stream termination**: Multiple ways to close streams (normal, cancel, error)
- **Connection monitoring**: Detection of client state changes

### 📊 Success Criteria:
1. **Normal completion**: Server terminates after sending all messages
2. **Client completion**: Client receives completion signal and terminates
3. **Cancellation detection**: Server distinguishes intentional cancellation
4. **Clean shutdown**: All resources properly cleaned up
5. **Status reporting**: Clear logging of all state transitions

## Troubleshooting

### Server doesn't terminate after completion
- Check that all messages were generated (look for "All messages generated!")
- Verify context cancellation is working properly
- Ensure cleanup task completes successfully

### Client doesn't receive completion signal
- Verify server is properly closing the channel (`drop(tx)`)
- Check for network connectivity issues
- Ensure client is using the correct server address

### Cancellation not detected properly
- Confirm client is calling `call.cancel()` correctly
- Check server logs for RST_STREAM detection
- Verify gRPC status codes match expected values

## Advanced Usage

### Custom Message Count
Modify `main.rs` line 315 to change message count:
```rust
let streaming_server = StreamingServer::new(message_interval, 20); // 20 messages
```

### Network Simulation
Use the proxy for advanced network testing:
```bash
# Terminal 1: Server
cd rust-server && cargo run --release

# Terminal 2: Proxy  
cd rust-proxy && cargo run --release

# Terminal 3: Client (via proxy)
cd python-client
GRPC_SERVER_ADDRESS="[::1]:8080" python client.py

# Terminal 4: Kill proxy to simulate network failure
pkill -f rust-proxy
```

## Conclusion

This implementation provides a comprehensive test suite for gRPC streaming scenarios, demonstrating:

- **Proper stream lifecycle management**
- **Reliable cancellation detection** 
- **Graceful error handling**
- **Resource cleanup best practices**

The tests validate that gRPC can effectively distinguish between normal completion, intentional cancellation, and network failures, enabling robust distributed system design.