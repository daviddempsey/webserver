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
use tokio::sync::{Semaphore, watch};
use tokio::task::JoinSet;

/// Configuration applied at server boot. Changing these requires a restart.
#[derive(Clone, Debug)]
pub struct InstallConfig {
    pub addr: String,
    pub max_connections: usize,
    pub shutdown_timeout: Duration,
}

impl Default for InstallConfig {
    fn default() -> Self {
        Self {
            addr: "127.0.0.1:8080".into(),
            max_connections: 256,
            shutdown_timeout: Duration::from_secs(5),
        }
    }
}

impl AsRef<InstallConfig> for InstallConfig {
    fn as_ref(&self) -> &InstallConfig {
        self
    }
}

/// Configuration read per-request. Updates pushed through the `watch` channel
/// take effect on the next request handled by each connection.
#[derive(Clone, Debug)]
pub struct RuntimeConfig {
    pub max_body_size: usize,
    pub io_timeout: Duration,
    pub keep_alive_timeout: Duration,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            max_body_size: 1024 * 1024,
            io_timeout: Duration::from_secs(30),
            keep_alive_timeout: Duration::from_secs(15),
        }
    }
}

impl AsRef<RuntimeConfig> for RuntimeConfig {
    fn as_ref(&self) -> &RuntimeConfig {
        self
    }
}

pub async fn run<I, R>(
    install: I,
    runtime: watch::Receiver<R>,
    router: Router,
) -> std::io::Result<()>
where
    I: AsRef<InstallConfig>,
    R: AsRef<RuntimeConfig> + Send + Sync + 'static,
{
    let listener = TcpListener::bind(&install.as_ref().addr).await?;
    let router = Arc::new(router);
    let semaphore = Arc::new(Semaphore::new(install.as_ref().max_connections));
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
                let runtime = runtime.clone();

                tasks.spawn(async move {
                    let _permit = match semaphore.acquire().await {
                        Ok(p) => p,
                        Err(_) => return,
                    };

                    let mut buf = vec![0u8; 4096];
                    let mut first_request = true;

                    loop {
                        // Snapshot runtime config per request so updates take
                        // effect at request boundaries without holding the
                        // watch borrow across awaits.
                        let (io_timeout, ka_timeout, max_body) = {
                            let cfg_ref = runtime.borrow();
                            let cfg: &RuntimeConfig = cfg_ref.as_ref();
                            (cfg.io_timeout, cfg.keep_alive_timeout, cfg.max_body_size)
                        };
                        let read_timeout = if first_request { io_timeout } else { ka_timeout };
                        first_request = false;

                        let n = match tokio::time::timeout(read_timeout, stream.read(&mut buf)).await {
                            Ok(Ok(n)) if n > 0 => n,
                            _ => break,
                        };

                        let mut headers = [httparse::EMPTY_HEADER; 64];
                        let mut parsed = httparse::Request::new(&mut headers);

                        let body_offset = match parsed.parse(&buf[..n]) {
                            Ok(httparse::Status::Complete(offset)) => offset,
                            _ => {
                                let bytes = Response::new(400, "Bad Request")
                                    .header("connection", "close")
                                    .to_bytes();
                                let _ = tokio::time::timeout(io_timeout, stream.write_all(&bytes)).await;
                                break;
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

                        let client_wants_close = req
                            .header("connection")
                            .map(|v| v.eq_ignore_ascii_case("close"))
                            .unwrap_or(false);

                        // Read full body based on Content-Length
                        let content_length: usize = req
                            .header("content-length")
                            .and_then(|v| v.parse().ok())
                            .unwrap_or(0);

                        if content_length > max_body {
                            let bytes = Response::new(413, "Payload Too Large")
                                .header("connection", "close")
                                .to_bytes();
                            let _ = tokio::time::timeout(io_timeout, stream.write_all(&bytes)).await;
                            break;
                        }

                        let mut body = Vec::with_capacity(content_length);
                        body.extend_from_slice(&buf[body_offset..n]);

                        while body.len() < content_length {
                            let mut chunk = [0u8; 4096];
                            match tokio::time::timeout(io_timeout, stream.read(&mut chunk)).await {
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
                                    let _ = tokio::time::timeout(io_timeout, stream.write_all(&bytes)).await;
                                    break;
                                }

                                if let Some(key) = req.headers.get("sec-websocket-key").cloned() {
                                    let accept = websocket::derive_accept_key(&key);
                                    let handshake = format!(
                                        "HTTP/1.1 101 Switching Protocols\r\n\
                                         Upgrade: websocket\r\n\
                                         Connection: Upgrade\r\n\
                                         Sec-WebSocket-Accept: {accept}\r\n\r\n"
                                    );
                                    match tokio::time::timeout(io_timeout, stream.write_all(handshake.as_bytes())).await {
                                        Ok(Ok(())) => {}
                                        _ => break,
                                    }
                                    let ws_stream =
                                        tokio_tungstenite::WebSocketStream::from_raw_socket(
                                            stream,
                                            tokio_tungstenite::tungstenite::protocol::Role::Server,
                                            None,
                                        )
                                        .await;
                                    ws_handler(req, ws_stream).await;
                                }
                            }
                            break;
                        }

                        let (handler, params) = router.dispatch(&method, &req.path);
                        req.params = params;
                        let mut resp = handler(req).await;

                        if client_wants_close {
                            resp = resp.header("connection", "close");
                        }

                        let keep_alive = resp.keep_alive();
                        let bytes = resp.to_bytes();
                        let _ = tokio::time::timeout(io_timeout, stream.write_all(&bytes)).await;

                        if !keep_alive {
                            break;
                        }
                    }
                });
            }
            _ = tokio::signal::ctrl_c() => {
                println!("\nShutting down...");
                break;
            }
        }
    }

    // Graceful shutdown with timeout
    if tokio::time::timeout(install.as_ref().shutdown_timeout, async {
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
