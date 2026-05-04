//! Request body reader supporting both `Content-Length` and
//! `Transfer-Encoding: chunked` framing.
//!
//! Browsers running over HTTP/2 routinely have their requests downgraded to
//! HTTP/1.1 chunked at proxies (Traefik, nginx, Cloudflare) — without
//! chunked support, every POST/PATCH body arrives as zero bytes and
//! handlers see an empty body.

use std::time::Duration;

use tokio::io::{AsyncRead, AsyncReadExt};

const SCRATCH_SIZE: usize = 4096;
const MAX_HEADER_LINE: usize = 8192;

#[derive(Debug)]
pub(crate) enum BodyError {
    TooLarge,
    Malformed,
    Io,
}

/// Read the request body from `stream`, given the bytes already consumed
/// past the request headers in `initial`.
///
/// If `chunked` is true, decodes per RFC 7230 §4.1 and `content_length` is
/// ignored (RFC 7230 §3.3.3 — Transfer-Encoding overrides Content-Length).
pub(crate) async fn read_body<R: AsyncRead + Unpin>(
    stream: &mut R,
    initial: Vec<u8>,
    chunked: bool,
    content_length: usize,
    max_body: usize,
    io_timeout: Duration,
) -> Result<Vec<u8>, BodyError> {
    if chunked {
        read_chunked(stream, initial, max_body, io_timeout).await
    } else {
        read_with_length(stream, initial, content_length, max_body, io_timeout).await
    }
}

/// `Transfer-Encoding` may be a comma-separated list (e.g. `gzip, chunked`).
/// Per RFC 7230, `chunked` if present must be the final encoding.
pub(crate) fn header_indicates_chunked(value: &str) -> bool {
    value
        .rsplit(',')
        .next()
        .map(|s| s.trim().eq_ignore_ascii_case("chunked"))
        .unwrap_or(false)
}

async fn read_with_length<R: AsyncRead + Unpin>(
    stream: &mut R,
    mut body: Vec<u8>,
    content_length: usize,
    max_body: usize,
    io_timeout: Duration,
) -> Result<Vec<u8>, BodyError> {
    if content_length > max_body {
        return Err(BodyError::TooLarge);
    }
    if body.len() >= content_length {
        body.truncate(content_length);
        return Ok(body);
    }

    body.reserve(content_length - body.len());
    while body.len() < content_length {
        let mut chunk = [0u8; SCRATCH_SIZE];
        let n = match tokio::time::timeout(io_timeout, stream.read(&mut chunk)).await {
            Ok(Ok(0)) | Ok(Err(_)) | Err(_) => return Err(BodyError::Io),
            Ok(Ok(n)) => n,
        };
        let needed = content_length - body.len();
        body.extend_from_slice(&chunk[..n.min(needed)]);
    }
    Ok(body)
}

async fn read_chunked<R: AsyncRead + Unpin>(
    stream: &mut R,
    mut buf: Vec<u8>,
    max_body: usize,
    io_timeout: Duration,
) -> Result<Vec<u8>, BodyError> {
    let mut body = Vec::new();

    loop {
        let crlf_pos = read_until_crlf(stream, &mut buf, io_timeout).await?;
        let size_line = &buf[..crlf_pos];

        // Strip optional ;ext=val parameters per RFC 7230 §4.1.1.
        let size_str = std::str::from_utf8(size_line).map_err(|_| BodyError::Malformed)?;
        let size_part = size_str.split(';').next().unwrap_or("").trim();
        let chunk_size =
            usize::from_str_radix(size_part, 16).map_err(|_| BodyError::Malformed)?;

        buf.drain(..crlf_pos + 2);

        if chunk_size == 0 {
            consume_trailers(stream, &mut buf, io_timeout).await?;
            return Ok(body);
        }

        if body.len().saturating_add(chunk_size) > max_body {
            return Err(BodyError::TooLarge);
        }

        // Need chunk_size + 2 bytes (chunk data + trailing CRLF).
        while buf.len() < chunk_size + 2 {
            if !read_more(stream, &mut buf, io_timeout).await? {
                return Err(BodyError::Malformed);
            }
        }

        body.extend_from_slice(&buf[..chunk_size]);
        if &buf[chunk_size..chunk_size + 2] != b"\r\n" {
            return Err(BodyError::Malformed);
        }
        buf.drain(..chunk_size + 2);
    }
}

/// Trailers end at the first empty line. Most clients send no trailers, so
/// the typical sequence is just `\r\n` immediately after the `0\r\n` chunk.
async fn consume_trailers<R: AsyncRead + Unpin>(
    stream: &mut R,
    buf: &mut Vec<u8>,
    io_timeout: Duration,
) -> Result<(), BodyError> {
    loop {
        let crlf_pos = read_until_crlf(stream, buf, io_timeout).await?;
        let is_empty = crlf_pos == 0;
        buf.drain(..crlf_pos + 2);
        if is_empty {
            return Ok(());
        }
    }
}

async fn read_until_crlf<R: AsyncRead + Unpin>(
    stream: &mut R,
    buf: &mut Vec<u8>,
    io_timeout: Duration,
) -> Result<usize, BodyError> {
    loop {
        if let Some(pos) = find_crlf(buf) {
            return Ok(pos);
        }
        if buf.len() > MAX_HEADER_LINE {
            return Err(BodyError::Malformed);
        }
        if !read_more(stream, buf, io_timeout).await? {
            return Err(BodyError::Malformed);
        }
    }
}

async fn read_more<R: AsyncRead + Unpin>(
    stream: &mut R,
    buf: &mut Vec<u8>,
    io_timeout: Duration,
) -> Result<bool, BodyError> {
    let mut chunk = [0u8; SCRATCH_SIZE];
    let n = match tokio::time::timeout(io_timeout, stream.read(&mut chunk)).await {
        Ok(Ok(n)) => n,
        Ok(Err(_)) | Err(_) => return Err(BodyError::Io),
    };
    if n == 0 {
        return Ok(false);
    }
    buf.extend_from_slice(&chunk[..n]);
    Ok(true)
}

fn find_crlf(buf: &[u8]) -> Option<usize> {
    buf.windows(2).position(|w| w == b"\r\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::time::Duration;
    use tokio::io::duplex;
    use tokio::io::AsyncWriteExt;

    const TIMEOUT: Duration = Duration::from_secs(2);
    const MAX_BODY: usize = 1024 * 1024;

    fn empty_initial() -> Vec<u8> {
        Vec::new()
    }

    #[test]
    fn chunked_header_parsing() {
        assert!(header_indicates_chunked("chunked"));
        assert!(header_indicates_chunked("CHUNKED"));
        assert!(header_indicates_chunked("gzip, chunked"));
        assert!(header_indicates_chunked("identity, chunked"));
        assert!(!header_indicates_chunked("identity"));
        assert!(!header_indicates_chunked("chunked, gzip")); // chunked must be last
        assert!(!header_indicates_chunked(""));
    }

    #[tokio::test]
    async fn content_length_passthrough() {
        let (mut client, mut server) = duplex(64);
        client.write_all(b"hello").await.unwrap();
        drop(client);
        let body =
            read_body(&mut server, empty_initial(), false, 5, MAX_BODY, TIMEOUT).await.unwrap();
        assert_eq!(body, b"hello");
    }

    #[tokio::test]
    async fn content_length_with_initial_already_complete() {
        let (_client, mut server) = duplex(64);
        let body = read_body(
            &mut server,
            b"already-here".to_vec(),
            false,
            12,
            MAX_BODY,
            TIMEOUT,
        )
        .await
        .unwrap();
        assert_eq!(body, b"already-here");
    }

    #[tokio::test]
    async fn content_length_truncates_overflow() {
        // Initial buffer has 12 bytes but content_length says 5.
        let (_client, mut server) = duplex(64);
        let body =
            read_body(&mut server, b"hello-extras".to_vec(), false, 5, MAX_BODY, TIMEOUT)
                .await
                .unwrap();
        assert_eq!(body, b"hello");
    }

    #[tokio::test]
    async fn content_length_too_large() {
        let (_client, mut server) = duplex(64);
        let err = read_body(&mut server, empty_initial(), false, 10, 5, TIMEOUT)
            .await
            .unwrap_err();
        assert!(matches!(err, BodyError::TooLarge));
    }

    #[tokio::test]
    async fn chunked_single_chunk() {
        let (mut client, mut server) = duplex(64);
        client.write_all(b"5\r\nhello\r\n0\r\n\r\n").await.unwrap();
        drop(client);
        let body = read_body(&mut server, empty_initial(), true, 0, MAX_BODY, TIMEOUT)
            .await
            .unwrap();
        assert_eq!(body, b"hello");
    }

    #[tokio::test]
    async fn chunked_multiple_chunks() {
        let (mut client, mut server) = duplex(64);
        client
            .write_all(b"5\r\nhello\r\n7\r\n, world\r\n0\r\n\r\n")
            .await
            .unwrap();
        drop(client);
        let body = read_body(&mut server, empty_initial(), true, 0, MAX_BODY, TIMEOUT)
            .await
            .unwrap();
        assert_eq!(body, b"hello, world");
    }

    #[tokio::test]
    async fn chunked_empty_body() {
        let (mut client, mut server) = duplex(64);
        client.write_all(b"0\r\n\r\n").await.unwrap();
        drop(client);
        let body = read_body(&mut server, empty_initial(), true, 0, MAX_BODY, TIMEOUT)
            .await
            .unwrap();
        assert_eq!(body, b"");
    }

    #[tokio::test]
    async fn chunked_with_size_extension_ignored() {
        let (mut client, mut server) = duplex(64);
        client
            .write_all(b"5;ext=val\r\nhello\r\n0\r\n\r\n")
            .await
            .unwrap();
        drop(client);
        let body = read_body(&mut server, empty_initial(), true, 0, MAX_BODY, TIMEOUT)
            .await
            .unwrap();
        assert_eq!(body, b"hello");
    }

    #[tokio::test]
    async fn chunked_with_trailer_ignored() {
        let (mut client, mut server) = duplex(64);
        client
            .write_all(b"5\r\nhello\r\n0\r\nX-Trailer: yes\r\n\r\n")
            .await
            .unwrap();
        drop(client);
        let body = read_body(&mut server, empty_initial(), true, 0, MAX_BODY, TIMEOUT)
            .await
            .unwrap();
        assert_eq!(body, b"hello");
    }

    #[tokio::test]
    async fn chunked_split_across_reads() {
        // Server-side reads will receive bytes incrementally.
        let (mut client, mut server) = duplex(8);
        let writer = tokio::spawn(async move {
            for fragment in [
                b"5\r\nhel".as_ref(),
                b"lo\r\n7\r\n, ".as_ref(),
                b"world\r\n0".as_ref(),
                b"\r\n\r\n".as_ref(),
            ] {
                client.write_all(fragment).await.unwrap();
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            drop(client);
        });
        let body = read_body(&mut server, empty_initial(), true, 0, MAX_BODY, TIMEOUT)
            .await
            .unwrap();
        writer.await.unwrap();
        assert_eq!(body, b"hello, world");
    }

    #[tokio::test]
    async fn chunked_initial_buffer_carries_complete_body() {
        // Whole body already in `initial` (common case for small POSTs).
        let (_client, mut server) = duplex(64);
        let body = read_body(
            &mut server,
            b"5\r\nhello\r\n0\r\n\r\n".to_vec(),
            true,
            0,
            MAX_BODY,
            TIMEOUT,
        )
        .await
        .unwrap();
        assert_eq!(body, b"hello");
    }

    #[tokio::test]
    async fn chunked_too_large() {
        // 0xff = 255 bytes claimed; max_body is 100, so the server
        // short-circuits with TooLarge after parsing the size line.
        // Only write the size line — the chunk data would block on the
        // duplex back-pressure since the server never reads it.
        let (mut client, mut server) = duplex(64);
        let writer = tokio::spawn(async move {
            client.write_all(b"ff\r\n").await.unwrap();
            drop(client);
        });
        let err = read_body(&mut server, empty_initial(), true, 0, 100, TIMEOUT)
            .await
            .unwrap_err();
        assert!(matches!(err, BodyError::TooLarge));
        writer.await.unwrap();
    }

    #[tokio::test]
    async fn chunked_malformed_hex() {
        let (mut client, mut server) = duplex(64);
        client.write_all(b"zzz\r\n").await.unwrap();
        drop(client);
        let err = read_body(&mut server, empty_initial(), true, 0, MAX_BODY, TIMEOUT)
            .await
            .unwrap_err();
        assert!(matches!(err, BodyError::Malformed));
    }

    #[tokio::test]
    async fn chunked_missing_terminator() {
        let (mut client, mut server) = duplex(64);
        // Chunk says 5 bytes but data isn't followed by CRLF.
        client.write_all(b"5\r\nhelloXX0\r\n\r\n").await.unwrap();
        drop(client);
        let err = read_body(&mut server, empty_initial(), true, 0, MAX_BODY, TIMEOUT)
            .await
            .unwrap_err();
        assert!(matches!(err, BodyError::Malformed));
    }
}
