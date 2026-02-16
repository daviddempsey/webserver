pub mod error;
pub mod health;
mod middleware;
mod request;
mod response;
mod router;
pub(crate) mod static_files;
pub mod test;
pub mod websocket;

pub use error::Error;
pub use health::{HealthCheck, HealthStatus};
pub use middleware::Next;
pub use request::Request;
pub use response::Response;
pub use router::Router;
pub use websocket::WebSocketStream;

use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;

const MAX_BODY_SIZE: usize = 1024 * 1024; // 1 MB
const IO_TIMEOUT: Duration = Duration::from_secs(30);
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_CONNECTIONS: usize = 256;

pub async fn run(addr: &str, router: Router) -> std::io::Result<()> {
    let listener = TcpListener::bind(addr).await?;
    let router = Arc::new(router);
    let semaphore = Arc::new(Semaphore::new(MAX_CONNECTIONS));
    let mut tasks = JoinSet::new();

    loop {
        while tasks.try_join_next().is_some() {}

        tokio::select! {
            result = listener.accept() => {
                let (mut stream, peer) = match result {
                    Ok(conn) => conn,
                    Err(e) => {
                        eprintln!("accept error: {e}");
                        continue;
                    }
                };
                let router = Arc::clone(&router);
                let semaphore = Arc::clone(&semaphore);

                tasks.spawn(async move {
                    let _permit = match semaphore.acquire().await {
                        Ok(p) => p,
                        Err(_) => return,
                    };

                    // Read initial data with timeout
                    let mut buf = vec![0u8; 4096];
                    let n = match tokio::time::timeout(IO_TIMEOUT, stream.read(&mut buf)).await {
                        Ok(Ok(n)) if n > 0 => n,
                        _ => return,
                    };

                    let mut headers = [httparse::EMPTY_HEADER; 64];
                    let mut parsed = httparse::Request::new(&mut headers);

                    let body_offset = match parsed.parse(&buf[..n]) {
                        Ok(httparse::Status::Complete(offset)) => offset,
                        _ => {
                            let bytes = Response::new(400, "Bad Request").to_bytes();
                            let _ = tokio::time::timeout(IO_TIMEOUT, stream.write_all(&bytes)).await;
                            return;
                        }
                    };

                    let method = parsed.method.unwrap_or("").to_string();
                    let raw_path = parsed.path.unwrap_or("/").to_string();

                    let mut req = Request::new(method.clone(), raw_path);
                    req.remote_addr = Some(peer.ip().to_string());

                    for h in parsed.headers.iter() {
                        req.headers.insert(
                            h.name.to_lowercase(),
                            String::from_utf8_lossy(h.value).to_string(),
                        );
                    }

                    // Read full body based on Content-Length
                    let content_length: usize = req
                        .header("content-length")
                        .and_then(|v| v.parse().ok())
                        .unwrap_or(0);

                    if content_length > MAX_BODY_SIZE {
                        let bytes = Response::new(413, "Payload Too Large").to_bytes();
                        let _ = tokio::time::timeout(IO_TIMEOUT, stream.write_all(&bytes)).await;
                        return;
                    }

                    let mut body = Vec::with_capacity(content_length);
                    body.extend_from_slice(&buf[body_offset..n]);

                    while body.len() < content_length {
                        let mut chunk = [0u8; 4096];
                        match tokio::time::timeout(IO_TIMEOUT, stream.read(&mut chunk)).await {
                            Ok(Ok(n)) if n > 0 => body.extend_from_slice(&chunk[..n]),
                            _ => break,
                        }
                    }

                    req.body = body;

                    // Check for WebSocket upgrade
                    if req
                        .headers
                        .get("upgrade")
                        .map(|v| v.eq_ignore_ascii_case("websocket"))
                        .unwrap_or(false)
                    {
                        if let Some((ws_handler, params)) = router.dispatch_ws(&req.path) {
                            req.params = params;

                            // Run middleware before upgrade
                            if let Some(resp) = router.run_middleware(req.clone()).await {
                                let bytes = resp.to_bytes();
                                let _ = tokio::time::timeout(IO_TIMEOUT, stream.write_all(&bytes)).await;
                                return;
                            }

                            if let Some(key) = req.headers.get("sec-websocket-key").cloned() {
                                let accept = websocket::derive_accept_key(&key);
                                let handshake = format!(
                                    "HTTP/1.1 101 Switching Protocols\r\n\
                                     Upgrade: websocket\r\n\
                                     Connection: Upgrade\r\n\
                                     Sec-WebSocket-Accept: {accept}\r\n\r\n"
                                );
                                match tokio::time::timeout(IO_TIMEOUT, stream.write_all(handshake.as_bytes())).await {
                                    Ok(Ok(())) => {}
                                    _ => return,
                                }
                                let ws_stream =
                                    tokio_tungstenite::WebSocketStream::from_raw_socket(
                                        stream,
                                        tokio_tungstenite::tungstenite::protocol::Role::Server,
                                        None,
                                    )
                                    .await;
                                ws_handler(req, ws_stream).await;
                                return;
                            }
                        }
                    }

                    let (handler, params) = router.dispatch(&method, &req.path);
                    req.params = params;
                    let resp = handler(req).await;

                    let bytes = resp.to_bytes();
                    let _ = tokio::time::timeout(IO_TIMEOUT, stream.write_all(&bytes)).await;
                });
            }
            _ = tokio::signal::ctrl_c() => {
                println!("\nShutting down...");
                break;
            }
        }
    }

    // Graceful shutdown with timeout
    if tokio::time::timeout(SHUTDOWN_TIMEOUT, async {
        while tasks.join_next().await.is_some() {}
    })
    .await
    .is_err()
    {
        eprintln!("Shutdown timeout — aborting remaining tasks");
        tasks.abort_all();
    }

    Ok(())
}
