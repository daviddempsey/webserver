use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use serde::Serialize;

use crate::{Request, Response};

pub trait HealthCheck: Send + Sync + 'static {
    fn name(&self) -> &str;
    fn check(&self) -> Pin<Box<dyn Future<Output = HealthStatus> + Send>>;
}

pub struct HealthStatus {
    pub healthy: bool,
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
