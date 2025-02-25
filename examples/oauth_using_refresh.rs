//! client_secret.json is the file you get from Google Cloud Console
use httpclient::InMemoryResponseExt;
use serde_json::Value;
use httpclient::oauth2::OAuth2Flow;

#[tokio::main]
async fn main() {
    httpclient::init_shared_client(httpclient::Client::new()
        .with_middleware(httpclient::middleware::Logger)
    );
    let cred = std::fs::read_to_string("../client_secret.json").unwrap();
    let mut cred: Value = serde_json::from_str(&cred).unwrap();
    let cred = cred.as_object_mut().unwrap().remove("web").unwrap();
    let flow = OAuth2Flow {
        client_id: cred["client_id"].as_str().unwrap().to_string(),
        client_secret: cred["client_secret"].as_str().unwrap().to_string(),
        init_endpoint: "https://accounts.google.com/o/oauth2/v2/auth".to_string(),
        exchange_endpoint: "https://oauth2.googleapis.com/token".to_string(),
        refresh_endpoint: "https://oauth2.googleapis.com/token".to_string(),
        redirect_uri: cred["redirect_uris"][0].as_str().unwrap().to_string(),
    };
    let access = std::env::var("ACCESS_TOKEN").expect("ACCESS_TOKEN is missing");
    let refresh = std::env::var("REFRESH_TOKEN").expect("REFRESH_TOKEN is missing");

    let mut middleware = flow.bearer_middleware(access, refresh);
    middleware.callback(|refresh| {
        println!("Received updated access token: {}", refresh.access_token);
    });

    let client = httpclient::Client::new()
        .with_middleware(middleware)
        .with_middleware(httpclient::middleware::Logger);

    let user_id = "me";
    let url = format!("https://gmail.googleapis.com/gmail/v1/users/{user_id}/threads");
    //unread_msgs = GMAIL.users().messages().list(user_id='me',labelIds=[label_id_one, label_id_two]).execute()

    let res = client.get(&url)
        .query("labelIds", "INBOX")
        .await
        .map_err(|e| e.into_text())
        .unwrap();
    let json: Value = res.json().unwrap();
    println!("{}", json);
}
