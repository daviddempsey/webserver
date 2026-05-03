use serde::{Deserialize, Serialize};
use std::time::Instant;
use tokio::sync::watch;
use webserver::{Error, Next, Request, Response, Router};

// Consumer-extended runtime config: holds webserver's required fields plus
// whatever the application needs to hot-reload.
struct AppRuntime {
    base: webserver::RuntimeConfig,
    greeting: String,
}

impl AsRef<webserver::RuntimeConfig> for AppRuntime {
    fn as_ref(&self) -> &webserver::RuntimeConfig {
        &self.base
    }
}

async fn logging(req: Request, next: Next) -> Response {
    let start = Instant::now();
    let method = req.method.clone();
    let path = req.path.clone();

    let resp = next.run(req).await;

    println!("{method} {path} -> {} ({:?})", resp.status, start.elapsed());
    resp
}

async fn greet(req: Request) -> Response {
    let name = req.param("name").unwrap_or("stranger");
    Response::new(200, format!("Hello, {name}!"))
}

#[derive(Serialize)]
struct User {
    id: u64,
    name: String,
}

async fn get_user(req: Request) -> Response {
    let id: u64 = req.param("id").and_then(|s| s.parse().ok()).unwrap_or(0);
    let user = User {
        id,
        name: format!("User {id}"),
    };
    Response::json(200, &user)
}

#[derive(Deserialize)]
struct CreateUser {
    name: String,
}

async fn search(req: Request) -> Response {
    let q = req.query("q").unwrap_or("");
    let page = req.query("page").unwrap_or("1");
    Response::json(200, &serde_json::json!({"query": q, "page": page}))
}

async fn create_user(req: Request) -> Result<Response, Error> {
    let input: CreateUser = req.json()?;
    let user = User {
        id: 1,
        name: input.name,
    };
    Ok(Response::json(201, &user))
}

async fn fail(_req: Request) -> Result<Response, Error> {
    Err(Error::not_found("this resource doesn't exist"))
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let install = webserver::InstallConfig::default();
    let (_runtime_tx, runtime_rx) = watch::channel(AppRuntime {
        base: webserver::RuntimeConfig::default(),
        greeting: "Welcome!".into(),
    });

    let mut router = Router::new();
    router.middleware(logging);

    // Handlers that need access to consumer-extended runtime fields capture
    // the receiver. Snapshot the field value before any await.
    let home_runtime = runtime_rx.clone();
    router.get("/", move |_req| {
        let runtime = home_runtime.clone();
        async move {
            let greeting = runtime.borrow().greeting.clone();
            Response::new(200, greeting)
        }
    });

    router.get("/hello/:name", greet);
    router.get("/users/:id", get_user);
    router.get("/search", search);
    router.post("/users", create_user);
    router.get("/fail", fail);
    router.static_dir("/static", "examples/public");

    println!("Listening on http://{}", install.addr);
    webserver::run(install, runtime_rx, router).await
}
