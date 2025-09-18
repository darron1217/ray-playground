import asyncio
import grpc
import time
import random
from asyncio import Queue

import streaming_pb2
import streaming_pb2_grpc


class StreamingClient:
    def __init__(self, server_address='[::1]:50051'):
        self.server_address = server_address
        self.response_queue = Queue()
        self.received_messages = set()
        self.should_simulate_drops = True
        self.drop_probability = 0.1

    async def bidirectional_stream(self):
        async with grpc.aio.insecure_channel(self.server_address) as channel:
            stub = streaming_pb2_grpc.StreamingServiceStub(channel)
            
            # 스트림 종료를 위한 플래그
            stream_finished = asyncio.Event()
            
            async def request_generator():
                while not stream_finished.is_set():
                    try:
                        ack_msg = await asyncio.wait_for(self.response_queue.get(), timeout=0.5)
                        yield ack_msg
                    except asyncio.TimeoutError:
                        # 타임아웃 시에도 스트림이 종료되지 않았다면 계속 대기
                        continue
                    except Exception as e:
                        print(f"[PYTHON CLIENT] Error in request_generator: {e}")
                        break
                print("[PYTHON CLIENT] Request generator finished")

            try:
                call = stub.BidirectionalStream(request_generator())
                
                async for response in call:
                    if response.HasField('data'):
                        data_msg = response.data
                        message_id = data_msg.id
                        
                        print(f"[PYTHON CLIENT] Received message {message_id}: {data_msg.payload}")
                        
                        should_drop = (self.should_simulate_drops and 
                                     random.random() < self.drop_probability)
                        
                        if should_drop:
                            print(f"[PYTHON CLIENT] Simulating drop for message {message_id}")
                        else:
                            self.received_messages.add(message_id)
                            
                            ack = streaming_pb2.StreamMessage(
                                ack=streaming_pb2.AckMessage(
                                    ack_id=message_id,
                                    timestamp=int(time.time())
                                )
                            )
                            
                            await self.response_queue.put(ack)
                            print(f"[PYTHON CLIENT] Sent ACK for message {message_id}")
                
                # 서버 스트림이 종료되면 request_generator도 종료
                print("[PYTHON CLIENT] Server stream ended, finishing client...")
                stream_finished.set()
                            
            except grpc.aio.AioRpcError as e:
                print(f"[PYTHON CLIENT] RPC error: {e}")
                stream_finished.set()
            except Exception as e:
                print(f"[PYTHON CLIENT] Unexpected error: {e}")
                stream_finished.set()

    def run(self):
        print("[PYTHON CLIENT] Starting Python gRPC client...")
        print(f"[PYTHON CLIENT] Connecting to server at {self.server_address}")
        print(f"[PYTHON CLIENT] Simulating message drops with {self.drop_probability * 100}% probability")
        
        asyncio.run(self.bidirectional_stream())


if __name__ == "__main__":
    client = StreamingClient()
    client.run()