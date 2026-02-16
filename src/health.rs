//! Health check endpoint with parallel checks and latency tracking.
//!
//! # Example
//!
//! ```
//! use std::future::Future;
//! use std::pin::Pin;
//! use webserver::Router;
//! use webserver::health::{HealthCheck, HealthStatus, health_handler};
//!
//! struct DbCheck;
//!
//! impl HealthCheck for DbCheck {
//!     fn name(&self) -> &str { "database" }
//!     fn check(&self) -> Pin<Box<dyn Future<Output = HealthStatus> + Send>> {
//!         Box::pin(async {
//!             HealthStatus { healthy: true, detail: None }
//!         })
//!     }
//! }
//!
//! let mut router = Router::new();
//! router.get("/health", health_handler(vec![Box::new(DbCheck)]));
//! ```
//!
//! The handler runs all checks in parallel and returns:
//! - **200** with `{"status": "healthy", "checks": {...}}` if all pass
//! - **503** with `{"status": "unhealthy", "checks": {...}}` if any fail
//!
//! Each check entry includes `status`, `latency_ms`, and an optional `detail`.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use serde::Serialize;

use crate::{Request, Response};

/// A named health check that runs asynchronously.
pub trait HealthCheck: Send + Sync + 'static {
    /// Unique name for this check (appears as a key in the JSON response).
    fn name(&self) -> &str;
    /// Run the check and return its status.
    fn check(&self) -> Pin<Box<dyn Future<Output = HealthStatus> + Send>>;
}

/// Result of a single health check.
pub struct HealthStatus {
    /// Whether the check passed.
    pub healthy: bool,
    /// Optional human-readable detail (e.g. error message). Omitted from
    /// JSON if `None`.
    pub detail: Option<String>,
}

#[derive(Serialize)]
struct CheckResult {
    status: &'static str,
    latency_ms: u128,
    #[serde(skip_serializing_if = "Option::is_none")]
    detail: Option<String>,
}

#[derive(Serialize)]
struct HealthResponse {
    status: &'static str,
    checks: std::collections::HashMap<String, CheckResult>,
}

/// Create a handler that runs the given checks in parallel and returns a
/// JSON health report.
///
/// See the [module-level docs](self) for full usage.
pub fn health_handler(
    checks: Vec<Box<dyn HealthCheck>>,
) -> impl Fn(Request) -> Pin<Box<dyn Future<Output = Response> + Send>> + Send + Sync + 'static {
    let checks = Arc::new(checks);

    move |_req: Request| {
        let checks = Arc::clone(&checks);

        Box::pin(async move {
            let mut tasks = tokio::task::JoinSet::new();

            for check in checks.iter() {
                let name = check.name().to_string();
                let fut = check.check();
                tasks.spawn(async move {
                    let start = tokio::time::Instant::now();
                    let status = fut.await;
                    let latency_ms = start.elapsed().as_millis();
                    let result = CheckResult {
                        status: if status.healthy { "healthy" } else { "unhealthy" },
                        latency_ms,
                        detail: status.detail,
                    };
                    (name, result)
                });
            }

            let mut results = Vec::with_capacity(tasks.len());
            while let Some(Ok(result)) = tasks.join_next().await {
                results.push(result);
            }

            let mut all_healthy = true;
            let mut check_map = std::collections::HashMap::new();
            for (name, result) in results {
                if result.status == "unhealthy" {
                    all_healthy = false;
                }
                check_map.insert(name, result);
            }

            let response = HealthResponse {
                status: if all_healthy { "healthy" } else { "unhealthy" },
                checks: check_map,
            };

            let code = if all_healthy { 200 } else { 503 };
            Response::json(code, &response)
        }) as Pin<Box<dyn Future<Output = Response> + Send>>
    }
}
