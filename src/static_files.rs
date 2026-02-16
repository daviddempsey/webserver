use std::path::{Path, PathBuf};

use crate::Response;

pub async fn serve(root: &Path, file_path: &str) -> Response {
    let clean: PathBuf = file_path
        .split('/')
        .filter(|s| !s.is_empty() && *s != "." && *s != "..")
        .collect();

    let full_path = root.join(&clean);

    // Ensure the resolved path is still under root
    let canonical_root = match root.canonicalize() {
        Ok(p) => p,
        Err(_) => return Response::new(404, "Not Found"),
    };
    let canonical_file = match full_path.canonicalize() {
        Ok(p) => p,
        Err(_) => return Response::new(404, "Not Found"),
    };
    if !canonical_file.starts_with(&canonical_root) {
        return Response::new(404, "Not Found");
    }

    // Serve index.html for directories
    let target = if canonical_file.is_dir() {
        canonical_file.join("index.html")
    } else {
        canonical_file
    };

    match tokio::fs::read(&target).await {
        Ok(contents) => {
            let content_type = mime_from_path(&target);
            Response::bytes(200, contents).header("content-type", content_type)
        }
        Err(_) => Response::new(404, "Not Found"),
    }
}

fn mime_from_path(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()) {
        Some("html" | "htm") => "text/html; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("js" | "mjs") => "text/javascript; charset=utf-8",
        Some("json") => "application/json",
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("svg") => "image/svg+xml",
        Some("ico") => "image/x-icon",
        Some("woff") => "font/woff",
        Some("woff2") => "font/woff2",
        Some("txt") => "text/plain; charset=utf-8",
        Some("xml") => "application/xml",
        Some("pdf") => "application/pdf",
        Some("wasm") => "application/wasm",
        _ => "application/octet-stream",
    }
}
