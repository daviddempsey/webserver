//! In-memory test client for exercising routes without network I/O.
//!
//! # Example
//!
//! ```
//! # use webserver::{Router, Response};
//! # #[tokio::main]
//! # async fn main() {
//! let mut router = Router::new();
//! router.get("/", |_req| async { Response::new(200, "ok") });
//!
//! // Quick one-liner
//! let resp = webserver::test::get(&router, "/").await;
//! assert_eq!(resp.status, 200);
//! assert_eq!(resp.body_str(), "ok");
//!
//! // Builder for headers, JSON body, etc.
//! let client = webserver::test::TestClient::new(&router);
//! let resp = client.request("POST", "/items")
//!     .header("x-api-key", "secret")
//!     .json(&serde_json::json!({"name": "thing"}))
//!     .send()
//!     .await;
//! # }
//! ```

use crate::{Request, Response, Router};

/// Test client that dispatches requests through a [`Router`] in-memory.
pub struct TestClient<'a> {
    router: &'a Router,
}

impl<'a> TestClient<'a> {
    /// Wrap a router for testing.
    pub fn new(router: &'a Router) -> Self {
        Self { router }
    }

    /// Start building a request with the given method and path.
    pub fn request(&self, method: &str, path: &str) -> TestRequest<'a> {
        TestRequest {
            router: self.router,
            method: method.to_string(),
            path: path.to_string(),
            headers: Vec::new(),
            body: Vec::new(),
            remote_addr: None,
        }
    }

    /// Send a GET request and return the response.
    pub async fn get(&self, path: &str) -> Response {
        self.request("GET", path).send().await
    }

    /// Send a POST request with a body and return the response.
    pub async fn post(&self, path: &str, body: impl Into<Vec<u8>>) -> Response {
        self.request("POST", path).body(body).send().await
    }
}

/// A request being built before dispatch. Supports headers, body, JSON, and
/// a fake remote address.
pub struct TestRequest<'a> {
    router: &'a Router,
    method: String,
    path: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
    remote_addr: Option<String>,
}

impl<'a> TestRequest<'a> {
    /// Add a header.
    pub fn header(mut self, name: &str, value: &str) -> Self {
        self.headers.push((name.to_lowercase(), value.to_string()));
        self
    }

    /// Set the raw body bytes.
    pub fn body(mut self, body: impl Into<Vec<u8>>) -> Self {
        self.body = body.into();
        self
    }

    /// Serialize `value` as JSON and set the `content-type` header.
    pub fn json<T: serde::Serialize>(self, value: &T) -> Self {
        let body = serde_json::to_vec(value).unwrap();
        self.header("content-type", "application/json").body(body)
    }

    /// Set a fake remote address for the request.
    pub fn remote_addr(mut self, addr: &str) -> Self {
        self.remote_addr = Some(addr.to_string());
        self
    }

    /// Dispatch the request through the router and return the response.
    pub async fn send(self) -> Response {
        let mut req = Request::new(self.method.clone(), self.path.clone());
        req.remote_addr = self.remote_addr;
        req.body = self.body;
        for (k, v) in self.headers {
            req.headers.insert(k, v);
        }

        let (handler, params) = self.router.dispatch(&self.method, &req.path);
        req.params = params;
        handler(req).await
    }
}

/// Send a GET request through the router. Shorthand for `TestClient::new(router).get(path)`.
pub async fn get(router: &Router, path: &str) -> Response {
    TestClient::new(router).get(path).await
}

/// Send a POST request through the router. Shorthand for `TestClient::new(router).post(path, body)`.
pub async fn post(router: &Router, path: &str, body: impl Into<Vec<u8>>) -> Response {
    TestClient::new(router).post(path, body).await
}
