use autonomi::data::DataAddress;
use autonomi::{Bytes, Client};

pub const ENVIRONMENTS: [&str; 3] = ["local", "autonomi", "alpha"];
pub const DEFAULT_ENVIRONMENT: &str = "autonomi";

#[derive(Clone)]
pub struct Server {
    client: Client,
}

impl Server {
    pub async fn new(environment: &str) -> Result<Self, String> {
        println!("Initializing client with environment: {environment:?}");

        let client = init_client(environment).await?;
        println!("Client initialized for streaming");

        Ok(Self { client })
    }

    pub async fn stream_data(
        &self,
        address: &str,
    ) -> Result<impl Iterator<Item = Result<Bytes, String>> + use<>, String> {
        println!("Starting to stream data from address: {address}");

        // Parse the address
        let data_address =
            DataAddress::from_hex(address).map_err(|e| format!("Invalid address format: {e}"))?;

        // Start streaming
        let stream = self
            .client
            .data_stream_public(&data_address)
            .await
            .map_err(|e| format!("Failed to start streaming: {e}"))?;

        // Convert GetError to String for easier error handling
        Ok(stream.map(|chunk_result| chunk_result.map_err(|e| e.to_string())))
    }
}

async fn init_client(environment: &str) -> Result<Client, String> {
    let res = match environment {
        "local" => Client::init_local().await,
        "alpha" => Client::init_alpha().await,
        _ => Client::init().await, // "autonomi"
    };
    res.map_err(|e| {
        println!("Error initializing client: {e}");
        format!("Error initializing client: {e}")
    })
}
