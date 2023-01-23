#[tokio::main]
async fn main() {
    let client = httpclient::Client::new()
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