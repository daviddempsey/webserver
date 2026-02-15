use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use crate::{Request, Response};

pub(crate) type Handler =
    Arc<dyn Fn(Request) -> Pin<Box<dyn Future<Output = Response> + Send>> + Send + Sync>;

pub struct Router {
    routes: HashMap<(String, String), Handler>,
}

impl Router {
    pub fn new() -> Self {
        Self {
            routes: HashMap::new(),
        }
    }

    pub fn route<F, Fut>(&mut self, method: &str, path: &str, handler: F)
    where
        F: Fn(Request) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Response> + Send + 'static,
    {
        let handler: Handler = Arc::new(move |req| {
            Box::pin(handler(req)) as Pin<Box<dyn Future<Output = Response> + Send>>
        });
        self.routes
            .insert((method.to_string(), path.to_string()), handler);
    }

    pub fn get<F, Fut>(&mut self, path: &str, handler: F)
    where
        F: Fn(Request) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Response> + Send + 'static,
    {
        self.route("GET", path, handler);
    }

    pub fn post<F, Fut>(&mut self, path: &str, handler: F)
    where
        F: Fn(Request) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Response> + Send + 'static,
    {
        self.route("POST", path, handler);
    }

    pub(crate) fn dispatch(&self, method: &str, path: &str) -> Option<&Handler> {
        self.routes.get(&(method.to_string(), path.to_string()))
    }
}
