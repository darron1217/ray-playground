use std::time::{Duration, Instant};
use tokio::io::{self, AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::sleep;

struct NetworkProxy {
    start_time: Instant,
    is_blocked: bool,
}

impl NetworkProxy {
    fn new() -> Self {
        Self {
            start_time: Instant::now(),
            is_blocked: false,
        }
    }

    fn should_block(&mut self) -> bool {
        let elapsed = self.start_time.elapsed().as_secs();
        
        // 5초 후 5초간 차단
        if elapsed >= 5 && elapsed < 10 {
            if !self.is_blocked {
                println!("🚫 [PROXY] Network BLOCKED (5 seconds)");
                self.is_blocked = true;
            }
            true
        } else {
            if self.is_blocked && elapsed >= 10 {
                println!("✅ [PROXY] Network RESTORED");
                self.is_blocked = false;
            }
            false
        }
    }

    async fn handle_client(&mut self, mut client: TcpStream) -> io::Result<()> {
        // 서버에 연결
        let mut server = TcpStream::connect("[::1]:50051").await?;
        
        let (mut client_read, mut client_write) = client.split();
        let (mut server_read, mut server_write) = server.split();

        // 양방향 데이터 전달
        let proxy_clone = std::sync::Arc::new(std::sync::Mutex::new(self));
        
        let client_to_server = {
            let proxy = proxy_clone.clone();
            async move {
                let mut buffer = [0; 4096];
                loop {
                    // 네트워크 차단 확인
                    if proxy.lock().unwrap().should_block() {
                        sleep(Duration::from_millis(100)).await;
                        continue;
                    }

                    match client_read.read(&mut buffer).await {
                        Ok(0) => break, // 연결 종료
                        Ok(n) => {
                            if server_write.write_all(&buffer[..n]).await.is_err() {
                                break;
                            }
                        }
                        Err(_) => break,
                    }
                }
            }
        };

        let server_to_client = {
            let proxy = proxy_clone.clone();
            async move {
                let mut buffer = [0; 4096];
                loop {
                    // 네트워크 차단 확인
                    if proxy.lock().unwrap().should_block() {
                        sleep(Duration::from_millis(100)).await;
                        continue;
                    }

                    match server_read.read(&mut buffer).await {
                        Ok(0) => break, // 연결 종료
                        Ok(n) => {
                            if client_write.write_all(&buffer[..n]).await.is_err() {
                                break;
                            }
                        }
                        Err(_) => break,
                    }
                }
            }
        };

        // 양방향 전달을 동시에 실행
        tokio::select! {
            _ = client_to_server => {},
            _ = server_to_client => {},
        }

        Ok(())
    }
}

#[tokio::main]
async fn main() -> io::Result<()> {
    println!("🚀 [PROXY] Rust Network Proxy starting on [::1]:8080");
    println!("🎯 [PROXY] Will block network for 1 second after 5 seconds");
    
    let listener = TcpListener::bind("[::1]:8080").await?;
    
    loop {
        let (client, _) = listener.accept().await?;
        let mut proxy = NetworkProxy::new();
        
        tokio::spawn(async move {
            if let Err(e) = proxy.handle_client(client).await {
                eprintln!("❌ [PROXY] Error handling client: {}", e);
            }
        });
    }
}