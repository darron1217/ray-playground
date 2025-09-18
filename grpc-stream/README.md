# gRPC Bidirectional Streaming Example

A Rust server and Python client implementation demonstrating bidirectional gRPC streaming with acknowledgments and retry logic.

## Features

- **1-minute streaming duration**: Connection maintained for exactly 60 seconds
- **Message acknowledgments**: Client sends ACK responses for received messages
- **Retry logic**: Server retries unacknowledged messages up to 3 times
- **Message drop simulation**: Client simulates 10% message drop rate for testing

## Setup

### Rust Server

```bash
cd rust-server
cargo build
cargo run
```

### Python Client

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
- Sends messages every 100ms for 1 minute
- Tracks pending messages requiring acknowledgment
- Retries unacknowledged messages after 2 seconds (max 3 retries)
- Handles client acknowledgments to remove messages from retry queue

### Python Client Features
- Receives messages from server
- Simulates 10% message drop rate for testing retry logic
- Sends acknowledgments for successfully received messages
- Handles bidirectional streaming with async/await

## Testing

Run both server and client simultaneously to observe:
1. Message streaming for 60 seconds
2. Acknowledgment flow
3. Retry behavior for dropped messages
4. Connection termination after 1 minute