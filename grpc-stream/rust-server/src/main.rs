use std::collections::HashMap;
use std::env;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::{mpsc, Mutex};
use tokio_stream::{wrappers::ReceiverStream, StreamExt};
use tonic::{transport::Server, Request, Response, Status, Streaming};

pub mod streaming {
    tonic::include_proto!("streaming");
}

use streaming::{
    streaming_service_server::{StreamingService, StreamingServiceServer},
    DataMessage, StreamMessage,
};

#[derive(Debug, Clone)]
struct PendingMessage {
    message: DataMessage,
    sent_at: u64,
    retry_count: u32,
}

#[derive(Default)]
struct StreamingServer {
    pending_messages: Arc<Mutex<HashMap<u64, PendingMessage>>>,
    total_messages: u64,
}

#[tonic::async_trait]
impl StreamingService for StreamingServer {
    type BidirectionalStreamStream = ReceiverStream<Result<StreamMessage, Status>>;

    async fn bidirectional_stream(
        &self,
        request: Request<Streaming<StreamMessage>>,
    ) -> Result<Response<Self::BidirectionalStreamStream>, Status> {
        let mut in_stream = request.into_inner();
        let (tx, rx) = mpsc::channel(128);
        let pending_messages = self.pending_messages.clone();
        let pending_messages_sender = pending_messages.clone();
        let pending_messages_retry = pending_messages.clone();
        let pending_messages_ack = pending_messages.clone();

        let tx_clone = tx.clone();
        let total_messages = self.total_messages;
        let message_sender = tokio::spawn(async move {
            for message_id in 1..=total_messages {
                let current_time = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs();

                let data_msg = DataMessage {
                    id: message_id,
                    timestamp: current_time,
                    payload: format!("Message {}", message_id),
                    needs_ack: true,
                };

                let pending_msg = PendingMessage {
                    message: data_msg.clone(),
                    sent_at: current_time,
                    retry_count: 0,
                };

                {
                    let mut pending = pending_messages_sender.lock().await;
                    pending.insert(message_id, pending_msg);
                }

                let stream_msg = StreamMessage {
                    message_type: Some(streaming::stream_message::MessageType::Data(data_msg)),
                };

                if tx_clone.send(Ok(stream_msg)).await.is_err() {
                    break;
                }

                println!("Sent message {}/{}", message_id, total_messages);
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
            
            println!("All {} messages sent, waiting for ACKs and retries...", total_messages);
        });

        let tx_retry = tx.clone();
        let retry_handler = tokio::spawn(async move {
            let mut retry_interval = tokio::time::interval(Duration::from_secs(2));
            
            loop {
                retry_interval.tick().await;
                
                let current_time = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs();

                let mut to_retry = Vec::new();
                let mut all_completed = false;
                
                {
                    let mut pending = pending_messages_retry.lock().await;
                    
                    // 재전송할 메시지 찾기
                    for (id, msg) in pending.iter_mut() {
                        if current_time - msg.sent_at > 2 && msg.retry_count < 3 {
                            msg.retry_count += 1;
                            msg.sent_at = current_time;
                            to_retry.push((*id, msg.message.clone()));
                        } else if msg.retry_count >= 3 {
                            println!("Message {} failed after 3 retries", id);
                        }
                    }
                    
                    // 모든 메시지가 완료되었는지 확인 (실패한 메시지 제외)
                    all_completed = pending.iter().all(|(_, msg)| msg.retry_count >= 3) || pending.is_empty();
                }

                // 재전송
                for (id, data_msg) in to_retry {
                    println!("Retrying message {}", id);
                    let stream_msg = StreamMessage {
                        message_type: Some(streaming::stream_message::MessageType::Data(data_msg)),
                    };
                    
                    if tx_retry.send(Ok(stream_msg)).await.is_err() {
                        println!("Failed to send retry message, stopping retry handler");
                        return;
                    }
                }
                
                // 모든 메시지가 완료되면 종료
                if all_completed {
                    println!("All messages completed, stopping retry handler");
                    break;
                }
            }
        });

        let ack_handler = tokio::spawn(async move {
            while let Some(message) = in_stream.next().await {
                match message {
                    Ok(stream_msg) => {
                        if let Some(streaming::stream_message::MessageType::Ack(ack)) = stream_msg.message_type {
                            println!("Received ACK for message {}", ack.ack_id);
                            let mut pending = pending_messages_ack.lock().await;
                            pending.remove(&ack.ack_id);
                        }
                    }
                    Err(e) => {
                        println!("Error receiving message: {}", e);
                        break;
                    }
                }
            }
        });

        // 모든 작업 완료 후 스트림 종료
        tokio::spawn(async move {
            // 메시지 전송 완료 대기
            let _ = message_sender.await;
            println!("Message sending completed, waiting for retries to finish...");
            
            // 재전송 핸들러 완료 대기
            let _ = retry_handler.await;
            
            // 모든 채널 닫기
            drop(tx);
            println!("All messages processed, closing stream");
            
            // ACK 핸들러 완료 대기
            let _ = ack_handler.await;
            println!("Stream closed completely");
        });

        Ok(Response::new(ReceiverStream::new(rx)))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    let message_count = if args.len() > 1 {
        args[1].parse::<u64>().unwrap_or(10)
    } else {
        10
    };

    let addr = "[::1]:50051".parse()?;
    let streaming_server = StreamingServer {
        pending_messages: Arc::new(Mutex::new(HashMap::new())),
        total_messages: message_count,
    };

    println!("Starting gRPC server on {}", addr);
    println!("Will send {} messages at 1-second intervals", message_count);

    // 서버 실행 (스트림이 자동으로 종료되면 서버도 종료됨)
    Server::builder()
        .add_service(StreamingServiceServer::new(streaming_server))
        .serve(addr)
        .await?;

    Ok(())
}
