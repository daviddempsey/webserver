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

async fn hello(_req: Request) -> Response {
    Response::new(200, "Hello, world!")
}

async fn submit(_req: Request) -> Response {
    Response::new(200, "Received")
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let mut router = Router::new();
    router.middleware(logging);
    router.get("/", home);
    router.get("/hello", hello);
    router.post("/submit", submit);

    println!("Listening on http://127.0.0.1:8080");
    webserver::run("127.0.0.1:8080", router).await
}
