use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tokio::time;
use tracing::{debug, error, info, instrument};

use crate::database::ConnectionPool;
use crate::stellar_service::StellarService;
use crate::error::NetworkError;

#[derive(Debug, Serialize, Deserialize)]
struct GetEventsRequest {
    jsonrpc: String,
    id: u32,
    method: String,
    params: GetEventsParams,
}

#[derive(Debug, Serialize, Deserialize)]
struct GetEventsParams {
    #[serde(rename = "startLedger")]
    start_ledger: u32,
    filters: Vec<EventFilter>,
}

#[derive(Debug, Serialize, Deserialize)]
struct EventFilter {
    #[serde(rename = "type")]
    event_type: String,
    #[serde(rename = "contractIds")]
    contract_ids: Vec<String>,
    topics: Vec<Vec<String>>,
}

#[derive(Debug, Serialize, Deserialize)]
struct GetEventsResponse {
    result: Option<EventsResult>,
    error: Option<serde_json::Value>,
}

#[derive(Debug, Serialize, Deserialize)]
struct EventsResult {
    events: Vec<SorobanEvent>,
    #[serde(rename = "latestLedger")]
    latest_ledger: u32,
}

#[derive(Debug, Serialize, Deserialize)]
struct SorobanEvent {
    #[serde(rename = "type")]
    event_type: String,
    ledger: u32,
    #[serde(rename = "contractId")]
    contract_id: String,
    id: String,
    topic: Vec<String>,
    value: SorobanEventValue,
}

#[derive(Debug, Serialize, Deserialize)]
struct SorobanEventValue {
    xdr: String,
}

pub struct EventIndexer {
    stellar_service: Arc<StellarService>,
    connection_pool: ConnectionPool,
    contract_id: String,
    polling_interval_secs: u64,
}

impl EventIndexer {
    pub fn new(
        stellar_service: Arc<StellarService>,
        connection_pool: ConnectionPool,
        contract_id: String,
        polling_interval_secs: u64,
    ) -> Self {
        Self {
            stellar_service,
            connection_pool,
            contract_id,
            polling_interval_secs,
        }
    }

    #[instrument(skip(self))]
    pub async fn start(&self) -> Result<(), NetworkError> {
        info!("Starting Soroban Event Indexer for contract: {}", self.contract_id);
        
        let client = Client::new();
        let rpc_url = std::env::var("SOROBAN_RPC_URL").unwrap_or_else(|_| "https://soroban-testnet.stellar.org".to_string());
        let mut interval = time::interval(Duration::from_secs(self.polling_interval_secs));
        let mut current_ledger: u32 = 0;

        loop {
            interval.tick().await;
            
            let filter = EventFilter {
                event_type: "contract".to_string(),
                contract_ids: vec![self.contract_id.clone()],
                topics: vec![vec!["AxionveraVault".to_string()]],
            };

            let req_body = GetEventsRequest {
                jsonrpc: "2.0".to_string(),
                id: 1,
                method: "getEvents".to_string(),
                params: GetEventsParams {
                    start_ledger: current_ledger,
                    filters: vec![filter],
                },
            };

            match client.post(&rpc_url).json(&req_body).send().await {
                Ok(response) => {
                    if response.status().is_success() {
                        match response.json::<GetEventsResponse>().await {
                            Ok(rpc_response) => {
                                if let Some(res) = rpc_response.result {
                                    for event in res.events {
                                        info!(
                                            event_id = %event.id,
                                            ledger = event.ledger,
                                            "Parsed AxionveraVault Soroban event"
                                        );
                                        // Ensure sensitive XDR is truncated/omitted from INFO logs
                                        debug!(xdr = %event.value.xdr, "Event XDR payload");
                                    }
                                    current_ledger = res.latest_ledger + 1;
                                } else if let Some(err) = rpc_response.error {
                                    error!(error = ?err, "RPC error returned");
                                }
                            }
                            Err(e) => error!("Failed to parse RPC response: {}", e),
                        }
                    } else {
                        error!("RPC request failed with status: {}", response.status());
                    }
                }
                Err(e) => error!("Failed to connect to Soroban RPC: {}", e),
            }
        }
    }
}