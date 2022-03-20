# HttpClient

`httpclient` is a user-friendly http client in Rust, similar to `reqwest` and many others. 

`httpclient` is under active development and is alpha quality softare. While we make effort not to change public APIs, we do not currently provide stability guarantees.

### Why not `reqwest`?

- `reqwest` objects are not serde-serializable. Having them serializable
- `reqwest` uses it's own custom types. `httpclient` tries to stay close to the `http` library, where we directly re-use, or have simple newtypes around, `http` structs.
- `reqwest` does not have middleware. `httpclient` provides powerful middleware for request recording, logging, retry, and other functionality. This functionality is user extensible.
