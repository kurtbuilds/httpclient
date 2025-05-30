use std::io::IsTerminal;
use std::time::Duration;
use httpclient::InMemoryResponseExt;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .compact()
        .without_time()
        .with_ansi(std::io::stdin().is_terminal())
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    let client = httpclient::Client::new().middleware(httpclient::Retry::new())
        .middleware(httpclient::TotalTimeout::new(Duration::from_secs(3)));
    let res = client.get("http://localhost:3000").await.unwrap();
    let res = res.text().unwrap();
    println!("{}", res);
}
