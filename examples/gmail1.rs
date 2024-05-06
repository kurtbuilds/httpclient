use httpclient::InMemoryResponseExt;

#[tokio::main]
async fn main() {
    let client = httpclient::Client::new()
        .with_middleware(httpclient::middleware::Recorder::new())
        ;
    let res = client.get("https://www.jsonip.com/")
        .header("secret", "foo")
        .await
        .unwrap();
    let res = res.text().unwrap();
    println!("{}", res);
}