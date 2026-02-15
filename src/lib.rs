use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

pub async fn run(addr: &str) -> std::io::Result<()> {
    let listener = TcpListener::bind(addr).await?;

    loop {
        let (mut stream, peer) = listener.accept().await?;

        tokio::spawn(async move {
            let mut buf = vec![0u8; 4096];
            let n = match stream.read(&mut buf).await {
                Ok(0) | Err(_) => return,
                Ok(n) => n,
            };

            let mut headers = [httparse::EMPTY_HEADER; 64];
            let mut req = httparse::Request::new(&mut headers);

            if req.parse(&buf[..n]).is_err() {
                let response = "HTTP/1.1 400 Bad Request\r\nContent-Length: 11\r\n\r\nBad Request";
                let _ = stream.write_all(response.as_bytes()).await;
                return;
            }

            let method = req.method.unwrap_or("");
            let path = req.path.unwrap_or("/");

            println!("{peer} - {method} {path}");

            let body = format!("{method} {path}");
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(response.as_bytes()).await;
        });
    }
}
