use serde::{Deserialize, Serialize};
use std::time::Instant;
use webserver::{Next, Request, Response, Router};

async fn logging(req: Request, next: Next) -> Response {
    let start = Instant::now();
    let method = req.method.clone();
    let path = req.path.clone();

    let resp = next.run(req).await;

    println!("{method} {path} -> {} ({:?})", resp.status, start.elapsed());
    resp
}

async fn home(_req: Request) -> Response {
    Response::new(200, "Welcome!")
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

async fn create_user(req: Request) -> Response {
    match req.json::<CreateUser>() {
        Ok(input) => {
            let user = User {
                id: 1,
                name: input.name,
            };
            Response::json(201, &user)
        }
        Err(_) => Response::json(400, &serde_json::json!({"error": "invalid JSON"})),
    }
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let mut router = Router::new();
    router.middleware(logging);
    router.get("/", home);
    router.get("/hello/:name", greet);
    router.get("/users/:id", get_user);
    router.get("/search", search);
    router.post("/users", create_user);
    router.static_dir("/static", "examples/public");

    println!("Listening on http://127.0.0.1:8080");
    webserver::run("127.0.0.1:8080", router).await
}
