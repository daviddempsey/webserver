pub mod error;
mod middleware;
mod request;
mod response;
mod router;
pub(crate) mod static_files;
pub mod test;

pub use error::Error;
pub use middleware::Next;
pub use request::Request;
pub use response::Response;
pub use router::Router;

use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::task::JoinSet;

pub async fn run(addr: &str, router: Router) -> std::io::Result<()> {
    let listener = TcpListener::bind(addr).await?;
    let router = Arc::new(router);
    let mut tasks = JoinSet::new();

    loop {
        // Clean up finished tasks
        while tasks.try_join_next().is_some() {}

        tokio::select! {
            result = listener.accept() => {
                let (mut stream, _peer): (tokio::net::TcpStream, _) = result?;
                let router = Arc::clone(&router);

                tasks.spawn(async move {
                    let mut buf = vec![0u8; 4096];
                    let n = match stream.read(&mut buf).await {
                        Ok(0) | Err(_) => return,
                        Ok(n) => n,
                    };

                    let mut headers = [httparse::EMPTY_HEADER; 64];
                    let mut parsed = httparse::Request::new(&mut headers);

                    let body_offset = match parsed.parse(&buf[..n]) {
                        Ok(httparse::Status::Complete(offset)) => offset,
                        _ => {
                            let resp = Response::new(400, "Bad Request");
                            let _ = stream.write_all(resp.to_bytes().as_slice()).await;
                            return;
                        }
                    };

                    let method = parsed.method.unwrap_or("").to_string();
                    let raw_path = parsed.path.unwrap_or("/").to_string();

                    let mut req = Request::new(method.clone(), raw_path);
                    req.body = buf[body_offset..n].to_vec();

                    for h in parsed.headers.iter() {
                        req.headers.insert(
                            h.name.to_lowercase(),
                            String::from_utf8_lossy(h.value).to_string(),
                        );
                    }

                    let (handler, params) = router.dispatch(&method, &req.path);
                    req.params = params;
                    let resp = handler(req).await;

                    let _ = stream.write_all(resp.to_bytes().as_slice()).await;
                });
            }
            _ = tokio::signal::ctrl_c() => {
                println!("\nShutting down...");
                break;
            }
        }
    }

    // Wait for in-flight requests to finish
    while tasks.join_next().await.is_some() {}

    Ok(())
}
