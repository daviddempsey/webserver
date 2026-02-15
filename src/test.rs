use crate::{Request, Response, Router};

pub struct TestClient<'a> {
    router: &'a Router,
}

impl<'a> TestClient<'a> {
    pub fn new(router: &'a Router) -> Self {
        Self { router }
    }

    pub fn request(&self, method: &str, path: &str) -> TestRequest<'a> {
        TestRequest {
            router: self.router,
            method: method.to_string(),
            path: path.to_string(),
            headers: Vec::new(),
            body: Vec::new(),
        }
    }

    pub async fn get(&self, path: &str) -> Response {
        self.request("GET", path).send().await
    }

    pub async fn post(&self, path: &str, body: impl Into<Vec<u8>>) -> Response {
        self.request("POST", path).body(body).send().await
    }
}

pub struct TestRequest<'a> {
    router: &'a Router,
    method: String,
    path: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl<'a> TestRequest<'a> {
    pub fn header(mut self, name: &str, value: &str) -> Self {
        self.headers.push((name.to_lowercase(), value.to_string()));
        self
    }

    pub fn body(mut self, body: impl Into<Vec<u8>>) -> Self {
        self.body = body.into();
        self
    }

    pub fn json<T: serde::Serialize>(self, value: &T) -> Self {
        let body = serde_json::to_vec(value).unwrap();
        self.header("content-type", "application/json").body(body)
    }

    pub async fn send(self) -> Response {
        let mut req = Request::new(self.method.clone(), self.path.clone());
        req.body = self.body;
        for (k, v) in self.headers {
            req.headers.insert(k, v);
        }

        let (handler, params) = self.router.dispatch(&self.method, &req.path);
        req.params = params;
        handler(req).await
    }
}

// Convenience functions
pub async fn get(router: &Router, path: &str) -> Response {
    TestClient::new(router).get(path).await
}

pub async fn post(router: &Router, path: &str, body: impl Into<Vec<u8>>) -> Response {
    TestClient::new(router).post(path, body).await
}
