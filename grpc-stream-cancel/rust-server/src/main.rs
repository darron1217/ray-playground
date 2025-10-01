use std::env;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::{mpsc, Mutex};
use tokio_stream::{wrappers::ReceiverStream, StreamExt};
use tokio_util::sync::CancellationToken;
use tonic::{transport::Server, Request, Response, Status, Streaming};

pub mod streaming {
    tonic::include_proto!("streaming");
}

use streaming::{
    streaming_service_server::{StreamingService, StreamingServiceServer},
    DataMessage,
};


/// 메시지 생성기 - 실시간으로 메시지 생성
#[derive(Clone)]
struct MessageGenerator {
    next_id: Arc<Mutex<u64>>,
    max_messages: u64,
}

impl MessageGenerator {
    fn new(max_messages: u64) -> Self {
        Self {
            next_id: Arc::new(Mutex::new(1)),
            max_messages,
        }
    }

    async fn generate_next(&self) -> Option<DataMessage> {
        let mut next_id = self.next_id.lock().await;
        if *next_id > self.max_messages {
            return None; // 모든 메시지 생성 완료
        }

        let current_time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let message = DataMessage {
            id: *next_id,
            timestamp: current_time,
            payload: format!("Message {} from server (max: {})", *next_id, self.max_messages),
        };

        *next_id += 1;
        Some(message)
    }

    async fn get_progress(&self) -> (u64, u64) {
        let next_id = self.next_id.lock().await;
        let generated = (*next_id - 1).min(self.max_messages);
        (generated, self.max_messages)
    }
}

/// Java gRPC의 Context.cancel()과 유사한 기능 - Tokio CancellationToken 사용
#[derive(Clone)]
struct GrpcContext {
    cancellation_token: CancellationToken,
    cancellation_reason: Arc<Mutex<Option<String>>>,
}

impl GrpcContext {
    fn new() -> Self {
        Self {
            cancellation_token: CancellationToken::new(),
            cancellation_reason: Arc::new(Mutex::new(None)),
        }
    }

    /// Java의 Context.isCancelled()와 동일
    fn is_cancelled(&self) -> bool {
        self.cancellation_token.is_cancelled()
    }

    /// Java의 Context.cancel()과 동일
    async fn cancel(&self, reason: String) {
        {
            let mut cancel_reason = self.cancellation_reason.lock().await;
            *cancel_reason = Some(reason);
        }
        self.cancellation_token.cancel();
    }

    async fn get_cancellation_reason(&self) -> Option<String> {
        self.cancellation_reason.lock().await.clone()
    }

    /// Java의 Context.cancelled() future와 유사
    async fn cancelled(&self) {
        self.cancellation_token.cancelled().await;
    }

    fn token(&self) -> CancellationToken {
        self.cancellation_token.clone()
    }
}

struct StreamingServer {
    message_interval: u64,
    message_generator: MessageGenerator,
}

impl StreamingServer {
    fn new(message_interval: u64, max_messages: u64) -> Self {
        Self {
            message_interval,
            message_generator: MessageGenerator::new(max_messages),
        }
    }
}

#[tonic::async_trait]
impl StreamingService for StreamingServer {
    type BidirectionalStreamStream = ReceiverStream<Result<DataMessage, Status>>;

    async fn bidirectional_stream(
        &self,
        request: Request<Streaming<DataMessage>>,
    ) -> Result<Response<Self::BidirectionalStreamStream>, Status> {
        println!("[RUST SERVER] 🔗 New client connected");
        
        let mut in_stream = request.into_inner();
        let (tx, rx) = mpsc::channel(10); // 10개 메시지 버퍼 (채널이 큐 역할)
        let message_interval = self.message_interval;

        // Java 스타일 gRPC Context 생성
        let grpc_context = GrpcContext::new();
        let context_sender = grpc_context.clone();
        let context_receiver = grpc_context.clone();
        let context_monitor = grpc_context.clone();

        let tx_sender = tx.clone();
        let generator = self.message_generator.clone();

        // 채널 기반 실시간 메시지 생성 + 전송
        let message_sender = tokio::spawn(async move {
            println!("[RUST SERVER] 📤 Starting real-time message generation (1 msg/sec)...");
            println!("[RUST SERVER] 📦 Channel buffer size: 10 messages");
            
            loop {
                // 취소 상태 확인
                if context_sender.is_cancelled() {
                    let reason = context_sender.get_cancellation_reason().await
                        .unwrap_or_else(|| "Unknown reason".to_string());
                    println!("[RUST SERVER] 🚫 Context cancelled: {}", reason);
                    break;
                }

                // 새 메시지 생성
                let message = match generator.generate_next().await {
                    Some(new_msg) => {
                        println!("[RUST SERVER] 🆕 Generated message {}", new_msg.id);
                        new_msg
                    }
                    None => {
                        println!("[RUST SERVER] 🎉 All messages generated!");
                        let (generated, max) = generator.get_progress().await;
                        println!("[RUST SERVER] 📊 Final progress: {}/{} messages", generated, max);
                        println!("[RUST SERVER] 🏁 Closing stream - all messages sent");
                        
                        // 모든 메시지 전송 완료 - 스트림을 정상 종료하기 위해 context cancel
                        context_sender.cancel("All messages sent - normal completion".to_string()).await;
                        drop(tx_sender);
                        break;
                    }
                };

                // 채널로 메시지 전송 (채널이 가득 차면 자동으로 대기)
                tokio::select! {
                    send_result = tx_sender.send(Ok(message.clone())) => {
                        match send_result {
                            Ok(_) => {
                                let (generated, max) = generator.get_progress().await;
                                println!("[RUST SERVER] ✅ Message {} sent to channel! Progress: {}/{}", 
                                    message.id, generated, max);
                            }
                            Err(_) => {
                                println!("[RUST SERVER] ❌ Channel closed - Client disconnected");
                                context_sender.cancel("Network disconnection detected".to_string()).await;
                                break;
                            }
                        }
                    }
                    _ = context_sender.cancelled() => {
                        println!("[RUST SERVER] 🚫 Context cancellation detected");
                        break;
                    }
                }
                
                // 1초 간격으로 메시지 생성
                tokio::select! {
                    _ = tokio::time::sleep(Duration::from_secs(message_interval)) => {}
                    _ = context_sender.cancelled() => {
                        println!("[RUST SERVER] 🚫 Context cancelled during sleep");
                        break;
                    }
                }
            }
            
            println!("[RUST SERVER] 🏁 Message generator finished");
        });

        // 클라이언트 메시지 수신 및 gRPC 표준 상태 감지
        let message_receiver = tokio::spawn(async move {
            println!("[RUST SERVER] 👂 Starting to listen for client messages (pure gRPC standard)...");
            
            while let Some(message_result) = in_stream.next().await {
                match message_result {
                    Ok(data_msg) => {
                        // 클라이언트가 데이터를 보냈다면 (실제로는 거의 없을 것)
                        println!("[RUST SERVER] 📨 Received data from client: {}", data_msg.payload);
                    }
                    Err(status) => {
                        println!("[RUST SERVER] ❌ gRPC Error from client:");
                        println!("[RUST SERVER]   Status Code: {:?}", status.code());
                        println!("[RUST SERVER]   Message: {}", status.message());
                        
                        // 순수 gRPC 상태 코드 기반 구분
                        let cancel_reason = match status.code() {
                            tonic::Code::Cancelled => {
                                println!("[RUST SERVER] 🚫 CANCELLED: Client called cancel() → RST_STREAM sent");
                                "gRPC standard cancellation - client called cancel()".to_string()
                            }
                            tonic::Code::Unavailable => {
                                println!("[RUST SERVER] 🔌 UNAVAILABLE: Network disconnection or server unavailable");
                                "gRPC unavailable - likely network disconnection".to_string()
                            }
                            tonic::Code::DeadlineExceeded => {
                                println!("[RUST SERVER] ⏰ DEADLINE_EXCEEDED: Timeout occurred");
                                "gRPC deadline exceeded - timeout".to_string()
                            }
                            _ => {
                                println!("[RUST SERVER] ❓ Other gRPC error: {:?}", status.code());
                                format!("gRPC error: {:?}", status.code())
                            }
                        };
                        
                        context_receiver.cancel(cancel_reason).await;
                        break;
                    }
                }
            }
            
            // 정상 종료 감지 - 네트워크 단절로 가정하고 재연결 대기
            println!("[RUST SERVER] 📋 Client stream ended → Assuming NETWORK DISCONNECTION");
            println!("[RUST SERVER] 💡 Keeping message generator running for reconnection...");
            println!("[RUST SERVER] 📦 Messages will continue buffering in channel");
            
            // 재연결을 위해 메시지 생성기는 계속 실행되도록 함
            // context_receiver.cancel()을 호출하지 않음 - 재연결 대기
            println!("[RUST SERVER] 🏁 Message receiver finished");
        });

        // 취소 원인 분석 및 처리
        let cancellation_monitor = tokio::spawn(async move {
            context_monitor.cancelled().await;
            
            let reason = context_monitor.get_cancellation_reason().await
                .unwrap_or_else(|| "Unknown".to_string());
            
            println!("[RUST SERVER] 🔔 Cancellation detected: {}", reason);
            
            // 의도적 취소 vs 네트워크 단절 vs 정상 완료 구분
            if reason.contains("gRPC standard cancellation") {
                println!("[RUST SERVER] 🚫 INTENTIONAL CANCELLATION:");
                println!("[RUST SERVER]   - Client called cancel() explicitly");
                println!("[RUST SERVER]   - Performing immediate cleanup");
            } else if reason.contains("All messages sent") {
                println!("[RUST SERVER] ✅ NORMAL COMPLETION:");
                println!("[RUST SERVER]   - All messages successfully sent");
                println!("[RUST SERVER]   - Stream closed gracefully");
            } else if reason.contains("Network disconnection") {
                println!("[RUST SERVER] 🔌 NETWORK DISCONNECTION:");
                println!("[RUST SERVER]   - Temporary network issue detected");
                println!("[RUST SERVER]   - Reconnection logic was applied");
            } else if reason.contains("Reconnection timeout") {
                println!("[RUST SERVER] ⏰ RECONNECTION TIMEOUT:");
                println!("[RUST SERVER]   - Client did not reconnect within timeout");
                println!("[RUST SERVER]   - Assuming permanent disconnection");
            } else {
                println!("[RUST SERVER] ❓ OTHER: {}", reason);
            }
            
            println!("[RUST SERVER] 🏁 Cancellation monitor finished");
        });

        // 정리 태스크
        tokio::spawn(async move {
            // 모든 태스크 완료 대기
            let _ = tokio::join!(message_sender, message_receiver, cancellation_monitor);
            
            // 스트림 종료
            drop(tx);
            println!("[RUST SERVER] 🏁 All tasks completed - stream closed");
        });

        println!("[RUST SERVER] ✅ Stream established with Java-style cancellation observer");
        Ok(Response::new(ReceiverStream::new(rx)))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    let message_interval = if args.len() > 1 {
        args[1].parse::<u64>().unwrap_or(2)
    } else {
        2
    };

    let addr = "[::1]:50051".parse()?;
    let streaming_server = StreamingServer::new(message_interval, 10); // 10개 메시지 생성

    println!("🚀 [RUST SERVER] Starting gRPC channel-based message server");
    println!("🔗 [RUST SERVER] Address: {}", addr);
    println!("⏱️  [RUST SERVER] Message interval: {} seconds", message_interval);
    println!("🎯 [RUST SERVER] Features:");
    println!("   - Real-time message generation (10 messages total)");
    println!("   - Channel buffer (10 messages) - automatic backpressure");
    println!("   - Client disconnects every 5s, server continues from buffer");
    println!();

    Server::builder()
        .add_service(StreamingServiceServer::new(streaming_server))
        .serve(addr)
        .await?;

    Ok(())
}