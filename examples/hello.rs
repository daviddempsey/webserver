#[tokio::main]
async fn main() -> std::io::Result<()> {
    println!("Listening on http://127.0.0.1:8080");
    webserver::run("127.0.0.1:8080").await
}
