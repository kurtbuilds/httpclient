#[cfg(feature = "stream")]
use httpclient::{Client, ResponseExt};
#[cfg(feature = "stream")]
use futures::StreamExt;

#[cfg(feature = "stream")]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::new();
    
    // Make a request to an SSE endpoint or any streaming endpoint
    let response = client.get("https://httpbin.org/stream/3").send().await?;
    
    // Use the bytes_stream method to get a stream of bytes
    let mut stream = response.bytes_stream();
    
    // Process each chunk as it arrives
    while let Some(chunk) = stream.next().await {
        match chunk {
            Ok(bytes) => {
                let text = String::from_utf8_lossy(&bytes);
                println!("Received chunk: {}", text);
            }
            Err(e) => {
                eprintln!("Error reading chunk: {:?}", e);
                break;
            }
        }
    }
    
    Ok(())
}

#[cfg(not(feature = "stream"))]
fn main() {
    println!("This example requires the 'stream' feature to be enabled.");
    println!("Run with: cargo run --example stream --features stream");
}