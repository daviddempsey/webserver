use serde::{Deserialize, Serialize};
use webserver::{Request, Response, Router};

fn setup_router() -> Router {
    let mut router = Router::new();
    router.get("/", |_req| async { Response::new(200, "Welcome!") });
    router.get("/hello/:name", |req: Request| async move {
        let name = req.param("name").unwrap_or("stranger");
        Response::new(200, format!("Hello, {name}!"))
    });
    router.get("/search", |req: Request| async move {
        let q = req.query("q").unwrap_or("none");
        Response::new(200, q.to_string())
    });
    router.post("/echo", |req: Request| async move {
        Response::new(200, req.body_str().to_string())
    });
    router.post("/json", |req: Request| async move {
        let input: CreateUser = req.json()?;
        Ok::<Response, webserver::Error>(Response::json(201, &User { id: 1, name: input.name }))
    });
    router
}

#[derive(Serialize, Deserialize)]
struct User {
    id: u64,
    name: String,
}

#[derive(Deserialize)]
struct CreateUser {
    name: String,
}

#[tokio::test]
async fn test_basic_get() {
    let router = setup_router();
    let resp = webserver::test::get(&router, "/").await;
    assert_eq!(resp.status, 200);
    assert_eq!(resp.body, "Welcome!");
}

#[tokio::test]
async fn test_path_params() {
    let router = setup_router();
    let resp = webserver::test::get(&router, "/hello/Dave").await;
    assert_eq!(resp.status, 200);
    assert_eq!(resp.body, "Hello, Dave!");
}

#[tokio::test]
async fn test_query_string() {
    let router = setup_router();
    let resp = webserver::test::get(&router, "/search?q=rust").await;
    assert_eq!(resp.status, 200);
    assert_eq!(resp.body, "rust");
}

#[tokio::test]
async fn test_not_found() {
    let router = setup_router();
    let resp = webserver::test::get(&router, "/nope").await;
    assert_eq!(resp.status, 404);
}

#[tokio::test]
async fn test_post_body() {
    let router = setup_router();
    let resp = webserver::test::post(&router, "/echo", "hello").await;
    assert_eq!(resp.status, 200);
    assert_eq!(resp.body, "hello");
}

#[tokio::test]
async fn test_json_request() {
    let router = setup_router();
    let client = webserver::test::TestClient::new(&router);
    let resp = client
        .request("POST", "/json")
        .json(&serde_json::json!({"name": "Alice"}))
        .send()
        .await;
    assert_eq!(resp.status, 201);
    let user: User = serde_json::from_str(&resp.body).unwrap();
    assert_eq!(user.name, "Alice");
}

#[tokio::test]
async fn test_json_error() {
    let router = setup_router();
    let resp = webserver::test::post(&router, "/json", "not json").await;
    assert_eq!(resp.status, 400);
    assert!(resp.body.contains("error"));
}

#[tokio::test]
async fn test_wrong_method() {
    let router = setup_router();
    let resp = webserver::test::post(&router, "/", "").await;
    assert_eq!(resp.status, 404);
}
