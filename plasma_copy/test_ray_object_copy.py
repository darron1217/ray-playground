import os
import time

import ray

from plasma_copy.cluster_memory_monitor import get_per_node_memory_usage

# --- 설정값 ---
OBJECT_SIZE_MB = 200
NUM_OBJECTS = 10
OBJECT_SIZE_BYTES = OBJECT_SIZE_MB * 1024 * 1024

# --- node2에서 데이터를 생성 ---
@ray.remote(resources={"node1": 1})
class DataProducer:
    def create_object(self):
        """지정된 크기의 랜덤 바이트 객체를 생성하여 반환합니다."""
        print(f"[Producer on node1] {OBJECT_SIZE_MB}MB 객체 생성 중...")
        # os.urandom을 사용해 압축되지 않는 랜덤 데이터 생성
        start_time = time.time()
        data = os.urandom(OBJECT_SIZE_BYTES)
        print(f"[Producer on node1] 객체 생성 완료. 크기: {len(data)} bytes, 소요 시간: {time.time() - start_time:.2f} 초")
        return data


# --- node1에서 데이터를 수신 ---
@ray.remote(resources={"node2": 1})
class DataConsumer:
    def receive_object(self, data):
        """데이터를 받아서 처리합니다. receive_object.remote() 호출 시점에 이미 데이터 전송이 완료됨."""
        print(f"[Consumer on node2] 데이터 크기: {len(data)} bytes")
        
        # 수신된 데이터의 크기를 반환하여 작업 완료 확인
        return len(data)

ray_host = "0.0.0.0"

def main():
    if not ray.is_initialized():
        ray.init(address=f"ray://{ray_host}:10001")

    get_object_store_usage()

    print("Ray 클러스터에 연결되었습니다.")
    print(f"[Main on node1] node2에서 {NUM_OBJECTS}개의 {OBJECT_SIZE_MB}MB 객체를 생성하고 node1으로 전송합니다.")
    print("-" * 50)


    # 액터들 생성
    producer = DataProducer.remote()  # node2에서 데이터 생성
    consumer = DataConsumer.remote()  # node1에서 데이터 수신

    # node2에서 객체 생성 (ObjectRef만 반환됨, 실제 데이터는 아직 node1으로 전송되지 않음)
    print(f"[Main on node1] node2에서 {NUM_OBJECTS}개의 객체 생성 요청...")
    object_refs = [producer.create_object.remote() for _ in range(NUM_OBJECTS)]
    
    ray.wait(object_refs, num_returns=len(object_refs))

    # 시간 측정 시작 - consumer.receive_object.remote() 호출 시점부터 데이터 전송 시작
    print("[Main on node1] node1 -> node2 데이터 전송 시작...")
    start_time = time.time()

    # consumer에게 ObjectRef들을 전달하여 실제 데이터 전송 발생
    # receive_object.remote(obj_ref) 호출 시점에 ray.get(obj_ref)이 암시적으로 실행됨
    result_refs = [consumer.receive_object.remote(obj_ref) for obj_ref in object_refs]
    results = ray.get(result_refs)  # consumer 작업 완료 대기
    
    # 시간 측정 종료
    end_time = time.time()
    
    # 전송된 데이터 크기 확인
    total_received_bytes = sum(results)  # consumer가 반환한 데이터 크기들의 합
    print(f"[Main on node1] 데이터 전송 완료! 총 수신: {total_received_bytes} bytes")
    
    # --- 결과 계산 및 출력 ---
    total_data_mb = total_received_bytes / (1024 * 1024)  # 실제 전송된 데이터 크기
    duration = end_time - start_time
    # 초당 MB 전송 속도
    speed_mbps = total_data_mb / duration

    print("-" * 50)
    print("✅ node1 -> node2 데이터 전송이 완료되었습니다.")
    print(f"  - 총 전송 데이터: {total_data_mb:.2f} MB ({NUM_OBJECTS}개 x {OBJECT_SIZE_MB}MB)")
    print(f"  - 총 소요 시간: {duration:.2f} 초")
    print(f"  - 평균 전송 속도: {speed_mbps:.2f} MB/s")
    print(f"  - 네트워크 처리량: {speed_mbps * 8:.2f} Mbps")

    ray.kill(producer)
    ray.kill(consumer)

def get_object_store_usage():
    stats =  get_per_node_memory_usage(f"{ray_host}:6379")

    for stat in stats:
        print("-" * 50)
        print(f"Memory Usage: {stat.node_address}: {stat.used_memory} / {stat.total_memory}")
        print(f"Object Store Usage: {stat.node_address}: {stat.object_store_used} / {stat.object_store_total}")
        

if __name__ == "__main__":
    main()
