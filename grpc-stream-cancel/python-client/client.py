import asyncio
import grpc
import time
import sys
import os

import streaming_pb2
import streaming_pb2_grpc


class StreamingClient:
    def __init__(self, server_address=None):
        # 환경변수에서 서버 주소 가져오기 (프록시 테스트용)
        import os
        if server_address:
            self.server_address = server_address
        else:
            env_address = os.getenv('GRPC_SERVER_ADDRESS')
            if env_address:
                self.server_address = env_address
                print(f"🔗 [PYTHON CLIENT] Using server address from env: {env_address}")
            else:
                self.server_address = '[::1]:50051'
                print(f"🔗 [PYTHON CLIENT] Using default server address: [::1]:50051")
        self.call = None
        self.message_count = 0
        self.auto_cancel_delay = None
        self.start_time = None

        
    async def bidirectional_stream(self):
        async with grpc.aio.insecure_channel(self.server_address) as channel:
            stub = streaming_pb2_grpc.StreamingServiceStub(channel)
            
            # 순수 gRPC 표준 request generator (데이터 전송 없음)
            async def empty_request_generator():
                # 클라이언트에서 서버로 보낼 데이터가 없는 경우
                # 빈 generator로 테스트
                yield

            try:
                print("🔗 [PYTHON CLIENT] Establishing bidirectional stream...")
                self.call = stub.BidirectionalStream(empty_request_generator())
                self.start_time = time.time()
                
                async for response in self.call:
                    self.message_count += 1
                    # 이제 직접 DataMessage를 받음
                    print(f"📨 [PYTHON CLIENT] Received message {response.id}: {response.payload}")
                    
                    # 자동 cancel 체크
                    if self.auto_cancel_delay is not None:
                        elapsed = time.time() - self.start_time
                        if elapsed >= self.auto_cancel_delay:
                            print(f"⏰ [PYTHON CLIENT] Auto-cancel triggered after {elapsed:.1f}s (delay: {self.auto_cancel_delay}s)")
                            print("📤 [PYTHON CLIENT] Calling gRPC cancel() → RST_STREAM")
                            self.call.cancel()
                            break
                
                print(f"✅ [PYTHON CLIENT] Stream ended normally. Total: {self.message_count} messages")
                            
            except grpc.aio.AioRpcError as e:
                print(f"⚠️  [PYTHON CLIENT] gRPC Error occurred:")
                print(f"   Status Code: {e.code()}")
                print(f"   Details: {e.details()}")
                print(f"   Messages received before error: {self.message_count}")
                
                if e.code() == grpc.StatusCode.CANCELLED:
                    print("🚫 [PYTHON CLIENT] Stream was CANCELLED")
                    print("   → This should correspond to RST_STREAM on server side")
                elif e.code() == grpc.StatusCode.UNAVAILABLE:
                    print("🔌 [PYTHON CLIENT] Server UNAVAILABLE - likely network issue")
                elif e.code() == grpc.StatusCode.DEADLINE_EXCEEDED:
                    print("⏰ [PYTHON CLIENT] DEADLINE_EXCEEDED - timeout occurred")
                else:
                    print(f"❓ [PYTHON CLIENT] Other error: {e.code()}")
                    
            except asyncio.CancelledError:
                print("🚫 [PYTHON CLIENT] AsyncIO CancelledError - likely from call.cancel()")
                print(f"   Messages received before cancellation: {self.message_count}")
                    
            except Exception as e:
                print(f"💥 [PYTHON CLIENT] Unexpected error: {e}")


    async def run_simple_test(self):
        """간단한 연결 테스트 - 재연결 포함"""
        print("🔗 [PYTHON CLIENT] Starting connection test with auto-reconnection...")
        
        max_retries = 50  # 더 많은 재시도 허용
        retry_count = 0
        consecutive_failures = 0
        
        while retry_count < max_retries:
            try:
                print(f"🔗 [PYTHON CLIENT] Connection attempt {retry_count + 1}")
                await self.bidirectional_stream()
                
                # 스트림이 정상 종료되면 완료
                print("✅ [PYTHON CLIENT] Stream completed by server")
                consecutive_failures = 0  # 성공 시 연속 실패 카운트 리셋
                
                # 서버가 모든 메시지를 보냈는지 확인
                print(f"📊 [PYTHON CLIENT] Total messages received: {self.message_count}")
                print("🎉 [PYTHON CLIENT] Server completed sending all messages!")
                break  # 서버가 스트림을 닫았으므로 재시도 불필요
                
            except grpc.aio.AioRpcError as e:
                retry_count += 1
                consecutive_failures += 1
                
                print(f"🔌 [PYTHON CLIENT] gRPC error (attempt {retry_count}/{max_retries}): {e.code()}")
                print(f"📊 [PYTHON CLIENT] Messages received so far: {self.message_count}")
                
                # 네트워크 오류인 경우 더 자주 재시도
                if e.code() in [grpc.StatusCode.UNAVAILABLE, grpc.StatusCode.DEADLINE_EXCEEDED]:
                    retry_delay = 1  # 네트워크 오류는 1초 후 재시도
                    print(f"🔄 [PYTHON CLIENT] Network error detected, retrying in {retry_delay}s...")
                else:
                    retry_delay = 2  # 다른 오류는 2초 후 재시도
                    print(f"🔄 [PYTHON CLIENT] Other gRPC error, retrying in {retry_delay}s...")
                
                if retry_count < max_retries:
                    await asyncio.sleep(retry_delay)
                else:
                    print("❌ [PYTHON CLIENT] Max retries reached")
                    
            except Exception as e:
                retry_count += 1
                consecutive_failures += 1
                
                print(f"💥 [PYTHON CLIENT] Unexpected error (attempt {retry_count}/{max_retries}): {e}")
                print(f"📊 [PYTHON CLIENT] Messages received so far: {self.message_count}")
                
                if retry_count < max_retries:
                    print("🔄 [PYTHON CLIENT] Retrying in 2 seconds...")
                    await asyncio.sleep(2)
                else:
                    print("❌ [PYTHON CLIENT] Max retries reached")
            
            # 연속 실패가 너무 많으면 잠시 대기
            if consecutive_failures >= 5:
                print(f"⚠️ [PYTHON CLIENT] {consecutive_failures} consecutive failures, waiting longer...")
                await asyncio.sleep(5)
                consecutive_failures = 0
                    
        print(f"📊 [PYTHON CLIENT] Final stats: {self.message_count} messages received total")
        print("🏁 [PYTHON CLIENT] Test completed")

    async def run_auto_cancel_test(self, delay):
        """자동 cancel 테스트 - 지정된 시간 후 자동으로 cancel"""
        print(f"⏰ [PYTHON CLIENT] AUTO CANCEL MODE: Will cancel after {delay} seconds")
        print("   Expected: Automatic call.cancel() → Server detects intentional cancellation")
        
        self.auto_cancel_delay = delay
        await self.bidirectional_stream()

    def run(self, mode="auto_cancel", cancel_delay=3.0):
        """클라이언트 실행 - 의도적 취소 vs 네트워크 단절 테스트"""
        print("🚀 [PYTHON CLIENT] Starting gRPC reconnection test client")
        print(f"🔗 [PYTHON CLIENT] Connecting to server at {self.server_address}")
        print()
        
        if mode == "auto_cancel":
            asyncio.run(self.run_auto_cancel_test(cancel_delay))
            
        elif mode == "simple":
            print("🔄 [PYTHON CLIENT] SIMPLE MODE: Basic connection test")
            print("   Expected: Receive all messages from server")
            asyncio.run(self.run_simple_test())
            
        else:
            print(f"❌ [PYTHON CLIENT] Unknown mode: {mode}. Use 'auto_cancel' or 'simple'")
            sys.exit(1)


if __name__ == "__main__":
    import argparse
    
    parser = argparse.ArgumentParser(description='gRPC Cancellation vs Disconnection Test Client')
    parser.add_argument('--mode', choices=['auto_cancel', 'simple'], default='auto_cancel',
                       help='Test mode: auto_cancel (automatic cancel after delay), simple (basic connection test)')
    parser.add_argument('--delay', type=float, default=3.0,
                       help='Delay in seconds before auto-cancellation (default: 3.0)')
    
    args = parser.parse_args()
    
    print("=" * 60)
    print("🧪 gRPC INTENTIONAL CANCEL vs NETWORK DISCONNECTION")
    print("=" * 60)
    print("This client tests two key scenarios:")
    print("• auto_cancel: Automatic cancellation after specified delay")
    print("• simple: Basic connection test → Receive all messages")
    print("=" * 60)
    print()
    
    client = StreamingClient()
    client.run(args.mode, args.delay)