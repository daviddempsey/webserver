use std::collections::HashMap;
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;

use crate::error::IntoResponse;
use crate::middleware::{self, Handler, MiddlewareFn, Next};
use crate::{Request, Response};

struct Route {
    method: String,
    segments: Vec<Segment>,
    handler: Handler,
}

enum Segment {
    Literal(String),
    Param(String),
}

fn parse_segments(pattern: &str) -> Vec<Segment> {
    pattern
        .split('/')
        .filter(|s| !s.is_empty())
        .map(|s| {
            if let Some(name) = s.strip_prefix(':') {
                Segment::Param(name.to_string())
            } else {
                Segment::Literal(s.to_string())
            }
        })
        .collect()
}

fn match_path(segments: &[Segment], path: &str) -> Option<HashMap<String, String>> {
    let path_parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();

    if segments.len() != path_parts.len() {
        return None;
    }

    let mut params = HashMap::new();

    for (seg, part) in segments.iter().zip(path_parts.iter()) {
        match seg {
            Segment::Literal(expected) if expected == part => {}
            Segment::Param(name) => {
                params.insert(name.clone(), part.to_string());
            }
            _ => return None,
        }
    }

    Some(params)
}

struct StaticMount {
    prefix: String,
    root: PathBuf,
}

pub struct Router {
    routes: Vec<Route>,
    statics: Vec<StaticMount>,
    middlewares: Vec<MiddlewareFn>,
}

impl Router {
    pub fn new() -> Self {
        Self {
            routes: Vec::new(),
            statics: Vec::new(),
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

    pub fn route<F, Fut, R>(&mut self, method: &str, path: &str, handler: F)
    where
        F: Fn(Request) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = R> + Send + 'static,
        R: IntoResponse + 'static,
    {
        let handler: Handler = Arc::new(move |req| {
            let fut = handler(req);
            Box::pin(async move { fut.await.into_response() })
                as Pin<Box<dyn Future<Output = Response> + Send>>
        });
        self.routes.push(Route {
            method: method.to_string(),
            segments: parse_segments(path),
            handler,
        });
    }

    pub fn get<F, Fut, R>(&mut self, path: &str, handler: F)
    where
        F: Fn(Request) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = R> + Send + 'static,
        R: IntoResponse + 'static,
    {
        self.route("GET", path, handler);
    }

    pub fn post<F, Fut, R>(&mut self, path: &str, handler: F)
    where
        F: Fn(Request) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = R> + Send + 'static,
        R: IntoResponse + 'static,
    {
        self.route("POST", path, handler);
    }

    pub fn static_dir(&mut self, prefix: &str, dir: impl Into<PathBuf>) {
        let prefix = prefix.trim_end_matches('/').to_string();
        self.statics.push(StaticMount {
            prefix,
            root: dir.into(),
        });
    }

    pub(crate) fn dispatch(&self, method: &str, path: &str) -> (Handler, HashMap<String, String>) {
        for route in &self.routes {
            if route.method != method {
                continue;
            }
            if let Some(params) = match_path(&route.segments, path) {
                let handler = middleware::chain(&self.middlewares, Arc::clone(&route.handler));
                return (handler, params);
            }
        }

        // Check static file mounts for GET requests
        if method == "GET" {
            for mount in &self.statics {
                if let Some(file_path) = path
                    .strip_prefix(&mount.prefix)
                    .map(|p| p.strip_prefix('/').unwrap_or(p))
                {
                    let root = mount.root.clone();
                    let file_path = file_path.to_string();
                    let handler: Handler = Arc::new(move |_req| {
                        let root = root.clone();
                        let file_path = file_path.clone();
                        Box::pin(async move { crate::static_files::serve(&root, &file_path).await })
                            as Pin<Box<dyn Future<Output = Response> + Send>>
                    });
                    return (
                        middleware::chain(&self.middlewares, handler),
                        HashMap::new(),
                    );
                }
            }
        }

        let handler: Handler = Arc::new(|_req| {
            Box::pin(async { Response::new(404, "Not Found") })
                as Pin<Box<dyn Future<Output = Response> + Send>>
        });
        (middleware::chain(&self.middlewares, handler), HashMap::new())
    }
}
