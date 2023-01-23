use http::{HeaderMap, StatusCode};
use serde_json::json;
use httpclient::{InMemoryBody, InMemoryResponse, Request};

#[tokio::main]
async fn main() {
    let mut client = httpclient::Client::new()
        .with_middleware(httpclient::middleware::RecorderMiddleware::new())
        ;
    let res = client.get("https://www.jsonip.com/")
        .header("secret", "foo")
        // .send_awaiting_body()
        .await
        .unwrap();
    let res = res.text().await.unwrap();
    println!("{}", res);
}