# gRPC Cancellation vs Disconnection Verification

A Rust server and Python client implementation to **verify and distinguish between gRPC cancellation and network disconnection** using standard gRPC mechanisms.

## Purpose

This project verifies whether gRPC can distinguish between:
- **Intentional cancellation**: Client calls `call.cancel()` → RST_STREAM frame
- **Network disconnection**: Connection lost due to network issues or process termination

## Test Scenarios

### 1. Manual Cancellation (Ctrl+C)
```bash
python client.py --mode manual
# Press Ctrl+C → call.cancel() → RST_STREAM → Server detects send failure
```

### 2. Automatic Cancellation
```bash
python client.py --mode auto
# Auto call.cancel() after 10s → RST_STREAM → Server detects send failure
```

### 3. Process Kill (Simulated Disconnection)
```bash
python client.py --mode kill
# Process killed after 5s → Connection lost → Server detects send failure
```

## Expected Behavior

According to [gRPC specification](https://github.com/hyperium/tonic/issues/2288):
- **Cancellation**: RST_STREAM frame sent, distinct from END_STREAM
- **Disconnection**: Connection failure, no explicit frame sent

However, at the application level, both scenarios result in:
- Server: `tx.send()` fails when client is gone
- Client: Receives appropriate gRPC status codes

## Setup

### Quick Start
```bash
# Test manual cancellation
./run.sh manual

# Test automatic cancellation  
./run.sh auto

# Test process kill (disconnection simulation)
./run.sh kill
```

### Manual Setup

#### Rust Server
```bash
cd rust-server
cargo build --release
cargo run --release [interval_seconds]
```

#### Python Client
```bash
cd python-client
pip install -r requirements.txt
python generate_protos.py
python client.py --mode [manual|auto|kill]
```

## Architecture

### Protocol Buffer Schema
- Simple `DataMessage` with id, timestamp, and payload
- Bidirectional stream: `stream DataMessage → stream DataMessage`

### Rust Server Features
- **Continuous streaming**: Sends messages at configurable intervals
- **Connection monitoring**: Detects when `tx.send()` fails
- **Detailed logging**: Reports client disconnection/cancellation
- **Standard gRPC**: No custom cancellation messages needed

### Python Client Features
- **Three test modes**:
  - `manual`: Ctrl+C triggers `call.cancel()`
  - `auto`: Automatic `call.cancel()` after 10 seconds
  - `kill`: Process termination after 5 seconds
- **Status code reporting**: Shows gRPC error codes and details
- **Message counting**: Tracks messages received before disconnection

## Expected Output

### Manual Cancellation (Ctrl+C):
```
🚀 [RUST SERVER] Starting gRPC server on [::1]:50051
[RUST SERVER] New client connected
[RUST SERVER] Sent message 1 successfully
📨 [PYTHON CLIENT] Received message 1: Message 1 from server
^C
🛑 [PYTHON CLIENT] Ctrl+C detected - calling gRPC cancel()
📤 [PYTHON CLIENT] Sending RST_STREAM via call.cancel()
[RUST SERVER] ❌ Failed to send message 2 - Client disconnected/cancelled
🚫 [PYTHON CLIENT] Stream was CANCELLED: User requested cancellation (Ctrl+C)
```

### Process Kill (Disconnection):
```
💀 [PYTHON CLIENT] KILL MODE: Will kill process after 5 seconds
📨 [PYTHON CLIENT] Received message 1: Message 1 from server
📨 [PYTHON CLIENT] Received message 2: Message 2 from server
💀 [PYTHON CLIENT] Killing process after 5 seconds
[RUST SERVER] ❌ Failed to send message 3 - Client disconnected/cancelled
```

## Key Verification Points

### What This Test Validates:
- ✅ `call.cancel()` properly terminates the stream
- ✅ Server detects client disconnection via failed sends
- ✅ Different client termination methods (cancel vs kill)
- ✅ gRPC status codes (CANCELLED, UNAVAILABLE, etc.)
- ✅ Message counting and timing

### What gRPC Provides:
- **RST_STREAM vs END_STREAM**: Different frame types for cancellation vs normal closure
- **Status codes**: CANCELLED, UNAVAILABLE, DEADLINE_EXCEEDED
- **Automatic cleanup**: Connection resources cleaned up properly

### Limitations:
- **Server perspective**: Both cancellation and disconnection appear as "send failure"
- **Timing dependent**: Network disconnection detection may be delayed
- **No built-in distinction**: Application must implement additional logic if needed

## Usage Examples

```bash
# Test all scenarios
./run.sh manual    # Interactive cancellation test
./run.sh auto      # Automated cancellation test  
./run.sh kill      # Disconnection simulation test

# Custom server interval
cd rust-server && cargo run --release 1  # 1-second intervals
cd python-client && python client.py --mode manual
```

## Conclusion

This implementation demonstrates that while gRPC uses different mechanisms (RST_STREAM vs connection loss) at the protocol level, **application-level code typically sees both as "client gone"**. 

For robust applications requiring disconnection vs cancellation distinction, consider:
- **Application-level heartbeats**
- **Explicit cancellation messages** before calling `cancel()`
- **Connection state monitoring**
- **Retry logic with exponential backoff**