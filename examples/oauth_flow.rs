//! client_secret.json is the file you get from Google Cloud Console
use httpclient::InMemoryResponseExt;
use serde_json::Value;
use httpclient::oauth2::{AccessType, Initialize, OAuth2Flow, PromptType};
use text_io::read;

#[tokio::main]
async fn main() {
    httpclient::init_shared_client(httpclient::Client::new()
        .with_middleware(httpclient::Logger)
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

    let url = flow.create_authorization_url(Initialize {
        scope: vec![
            "https://www.googleapis.com/auth/gmail.modify",
            "https://mail.google.com/",
        ].join(" "),
        access_type: AccessType::Offline,
        state: None,
        prompt: PromptType::Consent,
    });

    println!("Visit this URL to begin the flow:\n{}", url);
    println!("\nPaste the URL you're redirected to below:");

    let line: String = read!("{}\n");

    let params = flow.extract_code(&line).unwrap();

    // if we had passed in the state, now would be the time to check it

    let res = flow.exchange(params.code).await.unwrap();

    let middleware = flow.middleware_from_exchange(res).unwrap();

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
