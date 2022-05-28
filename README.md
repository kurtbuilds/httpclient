<div id="top"></div>

<p align="center">
<a href="https://github.com/kurtbuilds/httpclient/graphs/contributors">
    <img src="https://img.shields.io/github/contributors/kurtbuilds/httpclient.svg?style=flat-square" alt="GitHub Contributors" />
</a>
<a href="https://github.com/kurtbuilds/httpclient/stargazers">
    <img src="https://img.shields.io/github/stars/kurtbuilds/httpclient.svg?style=flat-square" alt="Stars" />
</a>
<a href="https://github.com/kurtbuilds/httpclient/actions">
    <img src="https://img.shields.io/github/workflow/status/kurtbuilds/httpclient/test?style=flat-square" alt="Build Status" />
</a>
<a href="https://crates.io/crates/httpclient">
    <img src="https://img.shields.io/crates/d/httpclient?style=flat-square" alt="Downloads" />
</a>
<a href="https://crates.io/crates/httpclient">
    <img src="https://img.shields.io/crates/v/httpclient?style=flat-square" alt="Crates.io" />
</a>

</p>


# HttpClient

`httpclient` is a user-friendly http client in Rust, similar to `reqwest` and many others. 

`httpclient` is under active development and is alpha quality softare. While we make effort not to change public APIs, we do not currently provide stability guarantees.

### Why not `reqwest`?

- `reqwest` objects are not serde-serializable. Having them serializable enables record/replay functionality.
- `reqwest` uses it's own custom types. `httpclient` tries to stay close to the `http` library, where we directly re-use, or have simple newtypes around, `http` structs.
- `reqwest` does not have middleware. `httpclient` provides powerful middleware for request recording, logging, retry, and other functionality. This functionality is user extensible.

# Roadmap

- [ ] Hide secrets in Recorder. Hash & Eq checks for requests must respect hidden values.
- [ ] Ensure it builds on wasm32-unknown-unknown
