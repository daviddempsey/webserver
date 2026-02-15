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

async fn user_post(req: Request) -> Response {
    let user_id = req.param("user_id").unwrap_or("?");
    let post_id = req.param("post_id").unwrap_or("?");
    Response::new(200, format!("User {user_id}, Post {post_id}"))
}

async fn headers(req: Request) -> Response {
    let ua = req.header("user-agent").unwrap_or("unknown");
    Response::new(200, format!("Your User-Agent: {ua}"))
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let mut router = Router::new();
    router.middleware(logging);
    router.get("/", home);
    router.get("/hello/:name", greet);
    router.get("/users/:user_id/posts/:post_id", user_post);
    router.get("/headers", headers);

    println!("Listening on http://127.0.0.1:8080");
    webserver::run("127.0.0.1:8080", router).await
}
