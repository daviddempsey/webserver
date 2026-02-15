pub struct Response {
    pub status: u16,
    pub body: String,
}

impl Response {
    pub fn new(status: u16, body: impl Into<String>) -> Self {
        Self {
            status,
            body: body.into(),
        }
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let reason = match self.status {
            200 => "OK",
            400 => "Bad Request",
            404 => "Not Found",
            405 => "Method Not Allowed",
            500 => "Internal Server Error",
            _ => "Unknown",
        };

        format!(
            "HTTP/1.1 {} {reason}\r\nContent-Length: {}\r\n\r\n{}",
            self.status,
            self.body.len(),
            self.body,
        )
        .into_bytes()
    }
}
