//! Error types and the [`IntoResponse`] trait for handler return values.

use crate::Response;

/// An HTTP error with a status code and message.
///
/// Handlers can return `Result<Response, Error>` — errors are automatically
/// converted to JSON responses via [`IntoResponse`].
///
/// # Convenience constructors
///
/// ```
/// use webserver::Error;
///
/// let e = Error::bad_request("missing field: name");   // 400
/// let e = Error::not_found("no such user");             // 404
/// let e = Error::timeout("upstream took too long");     // 408
/// let e = Error::internal("database connection lost");  // 500
/// ```
///
/// # Auto-conversion
///
/// `std::io::Error` converts to 500 and `serde_json::Error` converts to 400,
/// so `?` works in handlers:
///
/// ```
/// use webserver::{Error, Request, Response};
///
/// async fn handler(req: Request) -> Result<Response, Error> {
///     let data: serde_json::Value = req.json()?; // 400 on bad JSON
///     Ok(Response::json(200, &data))
/// }
/// ```
#[derive(Debug)]
pub struct Error {
    /// HTTP status code.
    pub status: u16,
    /// Human-readable error message (included in the JSON response body).
    pub message: String,
}

impl Error {
    /// Create an error with an arbitrary status code.
    pub fn new(status: u16, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
        }
    }

    /// 500 Internal Server Error.
    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(500, message)
    }

    /// 400 Bad Request.
    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::new(400, message)
    }

    /// 404 Not Found.
    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(404, message)
    }

    /// 408 Request Timeout.
    pub fn timeout(message: impl Into<String>) -> Self {
        Self::new(408, message)
    }

    pub(crate) fn into_response(self) -> Response {
        Response::json(self.status, &serde_json::json!({"error": self.message}))
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} {}", self.status, self.message)
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Self::internal(e.to_string())
    }
}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Self::bad_request(e.to_string())
    }
}

/// Trait for types that can be converted into an HTTP [`Response`].
///
/// Implemented for [`Response`] (identity) and `Result<Response, E>` where
/// `E: Into<Error>`, enabling handlers to return either type.
pub trait IntoResponse {
    /// Convert into a [`Response`].
    fn into_response(self) -> Response;
}

impl IntoResponse for Response {
    fn into_response(self) -> Response {
        self
    }
}

impl<E: Into<Error>> IntoResponse for Result<Response, E> {
    fn into_response(self) -> Response {
        match self {
            Ok(r) => r,
            Err(e) => e.into().into_response(),
        }
    }
}
