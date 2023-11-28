<div id="top"></div>

<p align="center">
<a href="https://github.com/kurtbuilds/httpclient/graphs/contributors">
    <img src="https://img.shields.io/github/contributors/kurtbuilds/httpclient.svg?style=flat-square" alt="GitHub Contributors" />
</a>
<a href="https://github.com/kurtbuilds/httpclient/stargazers">
    <img src="https://img.shields.io/github/stars/kurtbuilds/httpclient.svg?style=flat-square" alt="Stars" />
</a>
<a href="https://github.com/kurtbuilds/httpclient/actions">
    <img src="https://img.shields.io/github/actions/workflow/status/kurtbuilds/httpclient/test.yaml?style=flat-square" alt="Build Status" />
</a>
<a href="https://crates.io/crates/httpclient">
    <img src="https://img.shields.io/crates/d/httpclient?style=flat-square" alt="Downloads" />
</a>
<a href="https://crates.io/crates/httpclient">
    <img src="https://img.shields.io/crates/v/httpclient?style=flat-square" alt="Crates.io" />
</a>

</p>

# HttpClient

`httpclient` is a user-friendly http client in Rust. Where possible, it closely mimics the `reqwest` API. Why build a 
new http client?

- `httpclient::{Request, Response}` objects are serde-serializable, which enables record/replay functionality. See
the example below to see it in action.
- `httpclient` provides an API for user-extensible middleware. Built-in middleware includes redirect, retry, logging, 
and record/replay.
- `httpclient` provides a built-in `Error` type that can return the Http request, which includes the status code, headers,
and response body.
- `httpclient` provides convenience methods that `reqwest` does not support. The most important is the `IntoFuture`
implementation, which awaits both the request and the response body, which simplifies the scenario where you want to return
the request body even in error cases.

### Note on Http 1.0

`http` was recently upgraded to 1.0. However, `hyper_rustls` still depends on `0.2.x`. We are waiting for `hyper_rustls`
before bumping our own dependency.

```rust
#[tokio::main]
async fn main() {
    let mut client = httpclient::Client::new()
        // With this middleware, the script will only make the request once. After that, it replays from the filesystem
        // and does not hit the remote server. The middleware has different modes to ignore recordings (to force refresh)
        // and to prevent new requests (for running a test suite).
        // The recordings are sanitized to hide secrets.
        .with_middleware(httpclient::middleware::RecorderMiddleware::new())
        ;
    let res = client.get("https://www.jsonip.com/")
        .header("secret", "foo")
        .await
        .unwrap();
    let res = res.text().unwrap();
    
    let res = client.get("https://www.jsonip.com/")
        .header("secret", "foo")
        .send()
        .await
        .unwrap();
    // By using `send()`, we now can separately await the request body.
    let res = res.text().await.unwrap();
}
```
# Roadmap

- [x] Hide secrets in Recorder. Hash & Eq checks for requests must respect hidden values.
- [x] Ensure it builds on wasm32-unknown-unknown
- [ ] Sanitize "sessid" in json
