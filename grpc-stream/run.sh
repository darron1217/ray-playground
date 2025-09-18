#!/bin/bash

set -e

echo "Starting gRPC Bidirectional Streaming Demo..."
echo "=============================================="

# Check if required commands exist
if ! command -v cargo &> /dev/null; then
    echo "Error: cargo not found. Please install Rust."
    exit 1
fi

if ! command -v python &> /dev/null; then
    echo "Error: python not found. Please install Python."
    exit 1
fi

# Function to cleanup background processes
cleanup() {
    echo ""
    echo "Cleaning up processes..."
    if [ ! -z "$SERVER_PID" ]; then
        kill $SERVER_PID 2>/dev/null || true
    fi
    if [ ! -z "$CLIENT_PID" ]; then
        kill $CLIENT_PID 2>/dev/null || true
    fi
    wait 2>/dev/null || true
    echo "Demo completed."
    exit 0
}

# Set up signal handlers
trap cleanup SIGINT SIGTERM

# Build and run Rust server in background
echo "Building Rust server..."
cd rust-server
cargo build --release > /dev/null 2>&1

echo "Starting Rust server..."
MESSAGE_COUNT=${1:-5}  # 기본값 5개, 첫 번째 인자로 변경 가능
echo "Will send $MESSAGE_COUNT messages"
cargo run --release $MESSAGE_COUNT &
SERVER_PID=$!
cd ..

# Wait for server to start
echo "Waiting for server to start..."
sleep 3

# Setup Python client
echo "Setting up Python client..."
cd python-client

# Install dependencies if not already installed
pip install -r requirements.txt > /dev/null 2>&1

# Generate protobuf files
python generate_protos.py > /dev/null 2>&1

echo "Starting Python client..."
python client.py &
CLIENT_PID=$!
cd ..

echo ""
echo "Both server and client are running!"
echo "Server will send $MESSAGE_COUNT messages and auto-shutdown"
echo "Press Ctrl+C to stop early"
echo ""

# Wait for client to finish (it should stop after 1 minute)
wait $CLIENT_PID 2>/dev/null || true

echo ""
echo "Client finished. Stopping server..."
kill $SERVER_PID 2>/dev/null || true
wait $SERVER_PID 2>/dev/null || true

echo "Demo completed successfully!"