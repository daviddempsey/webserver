mod request;
mod response;
mod router;

pub use request::Request;
pub use response::Response;
pub use router::Router;

use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

pub async fn run(addr: &str, router: Router) -> std::io::Result<()> {
    let listener = TcpListener::bind(addr).await?;
    let router = Arc::new(router);

    loop {
        let (mut stream, peer) = listener.accept().await?;
        let router = Arc::clone(&router);

        tokio::spawn(async move {
            let mut buf = vec![0u8; 4096];
            let n = match stream.read(&mut buf).await {
                Ok(0) | Err(_) => return,
                Ok(n) => n,
            };

            let mut headers = [httparse::EMPTY_HEADER; 64];
            let mut parsed = httparse::Request::new(&mut headers);

            if parsed.parse(&buf[..n]).is_err() {
                let resp = Response::new(400, "Bad Request");
                let _ = stream.write_all(resp.to_bytes().as_slice()).await;
                return;
            }

            let method = parsed.method.unwrap_or("").to_string();
            let path = parsed.path.unwrap_or("/").to_string();

            println!("{peer} - {method} {path}");

            let req = Request::new(method.clone(), path.clone());
            let resp = match router.dispatch(&method, &path) {
                Some(handler) => handler(req).await,
                None => Response::new(404, "Not Found"),
            };

            let _ = stream.write_all(resp.to_bytes().as_slice()).await;
        });
    }
}
