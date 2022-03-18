use std::fs;
use encoding_rs::Encoding;
use hyper::Client;
use httpmock2::Response;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // let f = fs::File::open("data/vcr/www.jsonip.com/get.json").unwrap();
    // let res = serde_json::from_reader::<_, Vec<Response>>(&f).unwrap();
    // println!("{:?}", res);

    let https = hyper_rustls::HttpsConnectorBuilder::new()
        .with_native_roots()
        .https_only()
        .enable_http1()
        .build();
    let client = hyper::Client::builder().build(https);

    let mut r = client.request(hyper::Request::builder()
        .method(http::Method::GET)
        .uri("https://www.google.com")
        .body(hyper::Body::empty())
        .unwrap()).await.unwrap();
    let bytes = hyper::body::to_bytes(r.body_mut()).await?;
    let encoding = Encoding::for_label(&[]).unwrap_or(encoding_rs::UTF_8);
    let (text, _, _) = encoding.decode(&bytes);
    let r = text.to_string();
    println!("{}", r);
    Ok(())
}