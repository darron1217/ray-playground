# gRPC Bidirectional Streaming Example

A Rust server and Python client implementation demonstrating bidirectional gRPC streaming with acknowledgments and retry logic.

## Features

- **Configurable message count**: Send a specified number of messages (default: 10)
- **1-second message intervals**: Messages sent at regular 1-second intervals
- **Message acknowledgments**: Client sends ACK responses for received messages
- **Retry logic**: Server retries unacknowledged messages up to 3 times
- **Message drop simulation**: Client simulates 10% message drop rate for testing
- **Smart auto-shutdown**: Server automatically closes after all messages are processed
- **Clear logging**: Distinguishable logs with [RUST SERVER] and [PYTHON CLIENT] prefixes

## Setup

### Quick Start

Use the provided script to run both server and client:

```bash
# Run with default 10 messages
./run.sh

# Run with custom message count (e.g., 5 messages)
./run.sh 5
```

### Manual Setup

#### Rust Server
```bash
cd rust-server
cargo build --release

# Run with default 10 messages
cargo run --release

# Run with custom message count
cargo run --release 15
```

#### Python Client
```bash
cd python-client
pip install -r requirements.txt
python generate_protos.py
python client.py
```

## Architecture

### Protocol Buffer Schema
- `DataMessage`: Contains ID, timestamp, payload, and acknowledgment flag
- `AckMessage`: Contains acknowledgment ID and timestamp
- `StreamMessage`: Union type wrapping both message types

### Rust Server Features
- **Configurable message sending**: Accepts command-line argument for message count
- **1-second intervals**: Sends messages at regular 1-second intervals
- **Pending message tracking**: Maintains queue of unacknowledged messages
- **Smart retry logic**: Retries unacknowledged messages after 2 seconds (max 3 retries)
- **Automatic shutdown**: Closes connection after all messages are processed or max retries reached
- **Clear logging**: All logs prefixed with `[RUST SERVER]`

### Python Client Features
- **Async message handling**: Receives messages from server asynchronously
- **Drop simulation**: Simulates 10% message drop rate for testing retry logic
- **Acknowledgment responses**: Sends ACK for successfully received messages
- **Graceful termination**: Handles server disconnection properly
- **Clear logging**: All logs prefixed with `[PYTHON CLIENT]`

## Usage Examples

### Basic Usage
```bash
# Send 5 messages with 1-second intervals
./run.sh 5
```

### Expected Output
```
[RUST SERVER] Starting gRPC server on [::1]:50051
[RUST SERVER] Will send 5 messages at 1-second intervals
[RUST SERVER] Sent message 1/5
[PYTHON CLIENT] Received message 1: Message 1
[PYTHON CLIENT] Sent ACK for message 1
[RUST SERVER] Received ACK for message 1
...
[RUST SERVER] All messages completed, stopping retry handler
```

## Testing Scenarios

Run the demo to observe:
1. **Normal flow**: Messages sent and acknowledged
2. **Drop simulation**: Client drops ~10% of messages
3. **Retry behavior**: Server retries dropped messages after 2 seconds
4. **Automatic termination**: Clean shutdown after all messages processed