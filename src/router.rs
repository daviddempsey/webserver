use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use crate::middleware::{self, Handler, MiddlewareFn, Next};
use crate::{Request, Response};

pub struct Router {
    routes: HashMap<(String, String), Handler>,
    middlewares: Vec<MiddlewareFn>,
}

impl Router {
    pub fn new() -> Self {
        Self {
            routes: HashMap::new(),
            middlewares: Vec::new(),
        }
    }

    pub fn middleware<F, Fut>(&mut self, mw: F)
    where
        F: Fn(Request, Next) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Response> + Send + 'static,
    {
        self.middlewares.push(Arc::new(move |req, next| {
            Box::pin(mw(req, next)) as Pin<Box<dyn Future<Output = Response> + Send>>
        }));
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

    pub(crate) fn dispatch(&self, method: &str, path: &str) -> Handler {
        let handler = match self.routes.get(&(method.to_string(), path.to_string())) {
            Some(h) => Arc::clone(h),
            None => Arc::new(|_req| {
                Box::pin(async { Response::new(404, "Not Found") })
                    as Pin<Box<dyn Future<Output = Response> + Send>>
            }),
        };

        middleware::chain(&self.middlewares, handler)
    }
}
