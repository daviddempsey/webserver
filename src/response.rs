use std::collections::HashMap;

pub struct Response {
    pub status: u16,
    pub headers: HashMap<String, String>,
    pub body: String,
}

impl Response {
    pub fn new(status: u16, body: impl Into<String>) -> Self {
        Self {
            status,
            headers: HashMap::new(),
            body: body.into(),
        }
    }

    pub fn json<T: serde::Serialize>(status: u16, value: &T) -> Self {
        let body = serde_json::to_string(value).unwrap_or_else(|_| "null".to_string());
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

    pub fn to_bytes(&self) -> Vec<u8> {
        let reason = match self.status {
            200 => "OK",
            201 => "Created",
            400 => "Bad Request",
            401 => "Unauthorized",
            404 => "Not Found",
            405 => "Method Not Allowed",
            409 => "Conflict",
            422 => "Unprocessable Entity",
            429 => "Too Many Requests",
            503 => "Service Unavailable",
            500 => "Internal Server Error",
            _ => "Unknown",
        };

        let mut header_str = format!("content-length: {}\r\n", self.body.len());
        for (k, v) in &self.headers {
            if k != "content-length" {
                header_str.push_str(&format!("{k}: {v}\r\n"));
            }
        }

        format!(
            "HTTP/1.1 {} {reason}\r\n{header_str}\r\n{}",
            self.status, self.body,
        )
        .into_bytes()
    }
}
