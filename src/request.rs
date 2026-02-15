pub struct Request {
    pub method: String,
    pub path: String,
}

impl Request {
    pub fn new(method: String, path: String) -> Self {
        Self { method, path }
    }
}
