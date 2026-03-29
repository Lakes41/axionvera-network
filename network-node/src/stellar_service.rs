use std::sync::Arc;
use tracing::{debug, error, info};
use serde::{Deserialize, Serialize};
use crate::config::HorizonProvider;
use crate::error::NetworkError;
use crate::horizon_client::HorizonClient;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Account {
    pub account_id: String,
    pub balance: String,
    pub sequence: String,
    pub flags: AccountFlags,
    pub thresholds: Thresholds,
    pub signers: Vec<Signer>,
    pub data: std::collections::HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountFlags {
    pub auth_required: bool,
    pub auth_revocable: bool,
    pub auth_immutable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Thresholds {
    pub low_threshold: u8,
    pub med_threshold: u8,
    pub high_threshold: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Signer {
    pub key: String,
    pub weight: u8,
    #[serde(rename = "type")]
    pub signer_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transaction {
    pub hash: String,
    pub ledger: u32,
    pub created_at: String,
    pub source_account: String,
    pub fee_paid: u32,
    pub operation_count: u32,
    pub memo: Option<String>,
    pub signatures: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ledger {
    pub id: String,
    pub sequence: u32,
    pub closed_at: String,
    pub total_coins: String,
    pub fee_pool: String,
    pub base_fee: u32,
    pub base_reserve: String,
    pub max_tx_set_size: u32,
    pub protocol_version: u32,
}

pub struct StellarService {
    horizon_client: Arc<HorizonClient>,
}

impl StellarService {
    pub fn new(horizon_client: Arc<HorizonClient>) -> Self {
        Self { horizon_client }
    }

    /// Get account information from Horizon
    pub async fn get_account(&self, account_id: &str) -> Result<Account, NetworkError> {
        debug!("Getting account information for: {}", account_id);

        let operation = |provider: HorizonProvider| {
            let account_id = account_id.to_string();
            Box::pin(async move {
                let url = format!("{}/accounts/{}", provider.url.trim_end_matches('/'), account_id);
                let client = reqwest::Client::new();
                
                let response = client
                    .get(&url)
                    .header("Content-Type", "application/json")
                    .send()
                    .await?;

                if response.status().is_success() {
                    let account: Account = response.json().await?;
                    Ok(account)
                } else {
                    let error_text = response.text().await.unwrap_or_default();
                    Err(anyhow::anyhow!("HTTP {}: {}", response.status(), error_text))
                }
            })
        };

        self.horizon_client
            .execute_request(operation)
            .await
            .map_err(|e| {
                error!("Failed to get account {}: {}", account_id, e);
                e
            })
    }

    /// Get transaction information
    pub async fn get_transaction(&self, transaction_hash: &str) -> Result<Transaction, NetworkError> {
        debug!("Getting transaction information for hash: {}", transaction_hash);

        let operation = |provider: HorizonProvider| {
            let tx_hash = transaction_hash.to_string();
            Box::pin(async move {
                let url = format!("{}/transactions/{}", provider.url.trim_end_matches('/'), tx_hash);
                let client = reqwest::Client::new();
                
                let response = client
                    .get(&url)
                    .header("Content-Type", "application/json")
                    .send()
                    .await?;

                if response.status().is_success() {
                    let transaction: Transaction = response.json().await?;
                    Ok(transaction)
                } else {
                    let error_text = response.text().await.unwrap_or_default();
                    Err(anyhow::anyhow!("HTTP {}: {}", response.status(), error_text))
                }
            })
        };

        self.horizon_client
            .execute_request(operation)
            .await
            .map_err(|e| {
                error!("Failed to get transaction {}: {}", transaction_hash, e);
                e
            })
    }

    /// Get ledger information
    pub async fn get_ledger(&self, sequence: u32) -> Result<Ledger, NetworkError> {
        debug!("Getting ledger information for sequence: {}", sequence);

        let operation = |provider: HorizonProvider| {
            let ledger_sequence = sequence;
            Box::pin(async move {
                let url = format!("{}/ledgers/{}", provider.url.trim_end_matches('/'), ledger_sequence);
                let client = reqwest::Client::new();
                
                let response = client
                    .get(&url)
                    .header("Content-Type", "application/json")
                    .send()
                    .await?;

                if response.status().is_success() {
                    let ledger: Ledger = response.json().await?;
                    Ok(ledger)
                } else {
                    let error_text = response.text().await.unwrap_or_default();
                    Err(anyhow::anyhow!("HTTP {}: {}", response.status(), error_text))
                }
            })
        };

        self.horizon_client
            .execute_request(operation)
            .await
            .map_err(|e| {
                error!("Failed to get ledger {}: {}", sequence, e);
                e
            })
    }

    /// Get the latest ledger
    pub async fn get_latest_ledger(&self) -> Result<Ledger, NetworkError> {
        debug!("Getting latest ledger information");

        let operation = |provider: HorizonProvider| {
            Box::pin(async move {
                let url = format!("{}/ledgers?order=desc&limit=1", provider.url.trim_end_matches('/'));
                let client = reqwest::Client::new();
                
                let response = client
                    .get(&url)
                    .header("Content-Type", "application/json")
                    .send()
                    .await?;

                if response.status().is_success() {
                    let ledger_response: serde_json::Value = response.json().await?;
                    
                    if let Some(embedded) = ledger_response.get("_embedded") {
                        if let Some(records) = embedded.get("records") {
                            if let Some(ledger) = records.as_array().and_then(|arr| arr.first()) {
                                let ledger: Ledger = serde_json::from_value(ledger.clone())?;
                                return Ok(ledger);
                            }
                        }
                    }
                    
                    Err(anyhow::anyhow!("Invalid ledger response format"))
                } else {
                    let error_text = response.text().await.unwrap_or_default();
                    Err(anyhow::anyhow!("HTTP {}: {}", response.status(), error_text))
                }
            })
        };

        self.horizon_client
            .execute_request(operation)
            .await
            .map_err(|e| {
                error!("Failed to get latest ledger: {}", e);
                e
            })
    }

    /// Submit a transaction to the network
    pub async fn submit_transaction(&self, transaction_xdr: &str) -> Result<Transaction, NetworkError> {
        debug!("Submitting transaction to network");

        let operation = |provider: HorizonProvider| {
            let xdr = transaction_xdr.to_string();
            Box::pin(async move {
                let url = format!("{}/transactions", provider.url.trim_end_matches('/'));
                let client = reqwest::Client::new();
                
                let mut params = std::collections::HashMap::new();
                params.insert("tx", xdr);
                
                let response = client
                    .post(&url)
                    .header("Content-Type", "application/x-www-form-urlencoded")
                    .form(&params)
                    .send()
                    .await?;

                if response.status().is_success() {
                    let transaction: Transaction = response.json().await?;
                    Ok(transaction)
                } else {
                    let error_text = response.text().await.unwrap_or_default();
                    Err(anyhow::anyhow!("HTTP {}: {}", response.status(), error_text))
                }
            })
        };

        self.horizon_client
            .execute_request(operation)
            .await
            .map_err(|e| {
                error!("Failed to submit transaction: {}", e);
                e
            })
    }

    /// Get current Horizon provider status
    pub async fn get_provider_status(&self) -> Result<Vec<crate::horizon_client::ProviderStatus>, NetworkError> {
        Ok(self.horizon_client.get_provider_statuses().await)
    }

    /// Force switch to next provider (useful for testing)
    pub async fn switch_provider(&self) -> Result<HorizonProvider, NetworkError> {
        info!("Force switching to next Horizon provider");
        self.horizon_client.switch_provider().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::HorizonConfig;

    #[tokio::test]
    async fn test_stellar_service_creation() {
        let config = HorizonConfig::default();
        let horizon_client = Arc::new(crate::horizon_client::HorizonClient::new(config));
        let stellar_service = StellarService::new(horizon_client);
        
        // Test that we can get provider status
        let status = stellar_service.get_provider_status().await.unwrap();
        assert!(!status.is_empty());
    }

    #[tokio::test]
    async fn test_provider_switch() {
        let config = HorizonConfig::default();
        let horizon_client = Arc::new(crate::horizon_client::HorizonClient::new(config));
        let stellar_service = StellarService::new(horizon_client);
        
        // Test provider switching
        let provider = stellar_service.switch_provider().await;
        assert!(provider.is_ok());
    }
}
