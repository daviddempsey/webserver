use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use crate::{Request, Response};

pub(crate) type Handler =
    Arc<dyn Fn(Request) -> Pin<Box<dyn Future<Output = Response> + Send>> + Send + Sync>;

pub type MiddlewareFn =
    Arc<dyn Fn(Request, Next) -> Pin<Box<dyn Future<Output = Response> + Send>> + Send + Sync>;

pub struct Next {
    inner: Handler,
}

impl Next {
    pub(crate) fn new(inner: Handler) -> Self {
        Self { inner }
    }

    pub async fn run(self, req: Request) -> Response {
        (self.inner)(req).await
    }
}

pub(crate) fn chain(middlewares: &[MiddlewareFn], handler: Handler) -> Handler {
    let mut current = handler;

    for mw in middlewares.iter().rev() {
        let mw = Arc::clone(mw);
        let next_handler = current;
        current = Arc::new(move |req: Request| {
            let mw = Arc::clone(&mw);
            let next_handler = Arc::clone(&next_handler);
            Box::pin(async move {
                let next = Next::new(next_handler);
                mw(req, next).await
            }) as Pin<Box<dyn Future<Output = Response> + Send>>
        });
    }

    current
}
