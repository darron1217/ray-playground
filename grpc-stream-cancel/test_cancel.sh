#!/bin/bash

echo "🧪 Explicit gRPC Cancellation Test"
echo "=" * 50

# 기존 프로세스 정리
pkill -f 'grpc-stream-server' 2>/dev/null || true

# 1. 서버 시작 (프록시 없이 직접 연결)
echo "🚀 Starting gRPC server..."
cd rust-server
cargo run --release 1 &
SERVER_PID=$!
cd ..
sleep 3

echo ""
echo "🎯 Testing mid-stream cancellation:"
echo ""

# 중간 취소 테스트 (스트림 중간에)
echo "📋 Mid-stream cancellation test (3s delay)"
echo "Expected: Client receives several messages then cancels"
cd python-client
timeout 15 python client.py --mode auto_cancel --delay 3.0
CLIENT_EXIT_CODE=$?
cd ..

echo ""
echo "🔍 Checking server status..."
sleep 2

# 서버가 자동으로 종료되었는지 확인
if kill -0 $SERVER_PID 2>/dev/null; then
    echo "⚠️  Server is still running - this might indicate an issue"
    echo "🧹 Manually stopping server..."
    kill $SERVER_PID 2>/dev/null || true
    sleep 1
    if kill -0 $SERVER_PID 2>/dev/null; then
        echo "🔥 Force killing server..."
        kill -9 $SERVER_PID 2>/dev/null || true
    fi
    echo "❌ Test result: Server did not auto-terminate"
else
    echo "✅ Server terminated automatically - this is expected behavior"
    echo "✅ Test result: SUCCESS - Server detected cancellation and shut down"
fi

# 추가 정리
pkill -f 'grpc-stream-server' 2>/dev/null || true
pkill -f 'python client.py' 2>/dev/null || true

echo ""
echo "🏁 Cancellation test completed"
echo ""
echo "📊 Summary:"
echo "• Mid-stream cancel (3.0s delay)"
echo "• Server auto-termination check"
echo ""
echo "🔍 Success criteria:"
echo "• ✅ Client receives several messages before cancelling"
echo "• ✅ Server detects RST_STREAM for the cancellation"
echo "• ✅ Server terminates automatically after detecting cancellation"
echo "• ✅ Cancellation is 'intentional' (not network failure)"
