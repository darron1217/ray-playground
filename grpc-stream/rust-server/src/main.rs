use std::collections::HashMap;
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
        let pending_messages_retry = pending_messages.clone();
        let pending_messages_ack = pending_messages.clone();

        let tx_clone = tx.clone();
        let message_sender = tokio::spawn(async move {
            let mut message_id = 1u64;
            let start_time = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs();

            loop {
                let current_time = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs();

                if current_time - start_time > 10 {
                    println!("10 seconds elapsed, stopping stream");
                    break;
                }

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
                    let mut pending = pending_messages.lock().await;
                    pending.insert(message_id, pending_msg);
                }

                let stream_msg = StreamMessage {
                    message_type: Some(streaming::stream_message::MessageType::Data(data_msg)),
                };

                if tx_clone.send(Ok(stream_msg)).await.is_err() {
                    break;
                }

                message_id += 1;
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            
            // 메시지 전송 완료 후 채널 닫기
            drop(tx_clone);
            println!("Message sender finished and closed channel");
        });

        let tx_retry = tx.clone();
        let retry_handler = tokio::spawn(async move {
            let mut retry_interval = tokio::time::interval(Duration::from_secs(1));
            
            loop {
                tokio::select! {
                    _ = retry_interval.tick() => {
                        let current_time = SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .unwrap()
                            .as_secs();

                        let mut to_retry = Vec::new();
                        
                        {
                            let mut pending = pending_messages_retry.lock().await;
                            for (id, msg) in pending.iter_mut() {
                                if current_time - msg.sent_at > 2 && msg.retry_count < 3 {
                                    msg.retry_count += 1;
                                    msg.sent_at = current_time;
                                    to_retry.push((*id, msg.message.clone()));
                                } else if msg.retry_count >= 3 {
                                    println!("Message {} failed after 3 retries", id);
                                }
                            }
                        }

                        for (id, data_msg) in to_retry {
                            println!("Retrying message {}", id);
                            let stream_msg = StreamMessage {
                                message_type: Some(streaming::stream_message::MessageType::Data(data_msg)),
                            };
                            
                            if tx_retry.send(Ok(stream_msg)).await.is_err() {
                                println!("Failed to send retry message, stopping retry handler");
                                break;
                            }
                        }
                    }
                    else => {
                        println!("Retry handler stopping");
                        break;
                    }
                }
            }
            
            // 재시도 핸들러 완료 후 채널 닫기
            drop(tx_retry);
            println!("Retry handler finished and closed channel");
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

        // 메시지 전송 완료 후 남은 채널들을 정리
        tokio::spawn(async move {
            // 메시지 전송이 완료될 때까지 대기
            let _ = message_sender.await;
            
            // 짧은 대기 후 retry handler도 종료
            tokio::time::sleep(Duration::from_secs(2)).await;
            
            // 모든 채널 닫기
            drop(tx);
            println!("Main tx channel closed, stream will terminate");
            
            // ack_handler와 retry_handler 완료 대기
            let _ = tokio::join!(retry_handler, ack_handler);
            println!("All message handlers finished, stream closed");
        });

        Ok(Response::new(ReceiverStream::new(rx)))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let addr = "[::1]:50051".parse()?;
    let streaming_server = StreamingServer::default();

    println!("Starting gRPC server on {}", addr);

    Server::builder()
        .add_service(StreamingServiceServer::new(streaming_server))
        .serve(addr)
        .await?;

    Ok(())
}
