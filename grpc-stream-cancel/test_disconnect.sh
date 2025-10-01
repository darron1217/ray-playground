#!/bin/bash

echo "🧪 Simple Network Failure Test with Rust Proxy"
echo "=" * 50

# 기존 프로세스 정리
pkill -f 'grpc-stream-server' 2>/dev/null || true
pkill -f 'rust-proxy' 2>/dev/null || true

# 1. 서버 시작
echo "🚀 Starting gRPC server..."
cd rust-server
cargo run --release 1 &
SERVER_PID=$!
cd ..
sleep 3

# 2. 프록시 시작
echo "🔗 Starting Rust proxy..."
cd rust-proxy
cargo run --release &
PROXY_PID=$!
cd ..
sleep 2

# 3. 클라이언트 시작
echo "📱 Starting client..."
cd python-client
GRPC_SERVER_ADDRESS="[::1]:8080" timeout 20 python client.py --mode simple
cd ..

# 정리
echo "🧹 Cleaning up..."
kill $SERVER_PID $PROXY_PID 2>/dev/null || true
pkill -f 'grpc-stream-server' 2>/dev/null || true
pkill -f 'rust-proxy' 2>/dev/null || true

echo "✅ Test completed"
