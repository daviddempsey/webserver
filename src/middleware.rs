use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use crate::{Request, Response};

pub(crate) type Handler =
    Arc<dyn Fn(Request) -> Pin<Box<dyn Future<Output = Response> + Send>> + Send + Sync>;

pub type MiddlewareFn =
    Arc<dyn Fn(Request, Next) -> Pin<Box<dyn Future<Output = Response> + Send>> + Send + Sync>;

/// Handle to the next handler in the middleware chain.
///
/// Received by middleware functions. Call [`Next::run`] to pass the request
/// to the next middleware or the final handler. To short-circuit, return a
/// [`Response`] without calling `run`.
///
/// ```
/// # use webserver::{Request, Response, Next};
/// async fn auth(req: Request, next: Next) -> Response {
///     if req.header("authorization").is_none() {
///         return Response::new(401, "Unauthorized");
///     }
///     next.run(req).await
/// }
/// ```
pub struct Next {
    inner: Handler,
}

impl Next {
    pub(crate) fn new(inner: Handler) -> Self {
        Self { inner }
    }

    /// Pass the request to the next handler and await its response.
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
