



// #[derive(Clone, Copy)]
// struct Proxy;

// impl hyper::service::Service for Proxy {
//     type Request = server::Request;
//     type Response = server::Response;
//     type Error = hyper::Error;
//     type Future = FutureResult<server::Response, hyper::Error>;
//
//     fn call(&self, req: server::Request) -> Self::Future {
//         let client = Client::new();
//         let mut client_req = server_request_to_client_request(&req);
//         client_req.set_body(req.body());
//
//         client.request(client_req)
//             .and_then(|res| {
//                 let mut resp: server::Response = server::Response::new();
//                 resp = resp.with_headers(res.headers().clone());
//                 resp.set_status(res.status().clone());
//                 resp.set_body(res.body());
//                 futures::future::ok(resp)
//             })
//     }
// }


pub fn run_proxy(_port: u16) {
    // let addr = "127.0.0.1:1337".parse().unwrap();
    //
    // let server = Http::new().bind(&addr, || Ok(Proxy)).unwrap();
    // println!("Listening on http://{} with 1 thread.",
    //          server.local_addr().unwrap());
    // server.run().unwrap();
}