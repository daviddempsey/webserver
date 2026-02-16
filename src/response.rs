use std::collections::HashMap;

pub struct Response {
    pub status: u16,
    pub headers: HashMap<String, String>,
    pub body: Vec<u8>,
}

impl Response {
    pub fn new(status: u16, body: impl Into<String>) -> Self {
        Self {
            status,
            headers: HashMap::new(),
            body: body.into().into_bytes(),
        }
    }

    pub fn bytes(status: u16, body: Vec<u8>) -> Self {
        Self {
            status,
            headers: HashMap::new(),
            body,
        }
    }

    pub fn json<T: serde::Serialize>(status: u16, value: &T) -> Self {
        let body = serde_json::to_vec(value).unwrap_or_else(|_| b"null".to_vec());
        let mut headers = HashMap::new();
        headers.insert("content-type".to_string(), "application/json".to_string());
        Self {
            status,
            headers,
            body,
        }
    }

    pub fn header(mut self, name: &str, value: &str) -> Self {
        self.headers.insert(name.to_lowercase(), value.to_string());
        self
    }

    pub fn body_str(&self) -> &str {
        std::str::from_utf8(&self.body).unwrap_or("")
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let reason = match self.status {
            200 => "OK",
            201 => "Created",
            400 => "Bad Request",
            401 => "Unauthorized",
            403 => "Forbidden",
            404 => "Not Found",
            405 => "Method Not Allowed",
            408 => "Request Timeout",
            409 => "Conflict",
            413 => "Payload Too Large",
            422 => "Unprocessable Entity",
            429 => "Too Many Requests",
            503 => "Service Unavailable",
            500 => "Internal Server Error",
            _ => "Unknown",
        };

        let mut header_str = format!(
            "content-length: {}\r\nconnection: close\r\n",
            self.body.len()
        );
        for (k, v) in &self.headers {
            if k != "content-length" && k != "connection" {
                header_str.push_str(&format!("{k}: {v}\r\n"));
            }
        }

        let mut out =
            format!("HTTP/1.1 {} {reason}\r\n{header_str}\r\n", self.status).into_bytes();
        out.extend_from_slice(&self.body);
        out
    }
}
