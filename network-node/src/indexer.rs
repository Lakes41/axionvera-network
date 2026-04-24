use std::sync::Arc;
use std::time::Duration;
use tracing::{info, error, debug, warn, instrument};
use sqlx::{Postgres, Transaction};
use crate::stellar_service::{StellarService, Ledger};
use crate::database::ConnectionPool;
use crate::error::{NetworkError, Result};
use serde_json::json;

pub struct EventIndexer {
    stellar_service: Arc<StellarService>,
    connection_pool: ConnectionPool,
    contract_address: String,
    poll_interval: Duration,
}

impl EventIndexer {
    pub fn new(
        stellar_service: Arc<StellarService>,
        connection_pool: ConnectionPool,
        contract_address: String,
        poll_interval_secs: u64,
    ) -> Self {
        Self {
            stellar_service,
            connection_pool,
            contract_address,
            poll_interval: Duration::from_secs(poll_interval_secs),
        }
    }

    pub async fn start(&self) -> Result<()> {
        info!("Starting event indexer for contract: {}", self.contract_address);
        
        loop {
            if let Err(e) = self.process_next_batch().await {
                error!("Error processing event batch: {}", e);
                tokio::time::sleep(Duration::from_secs(5)).await;
                continue;
            }
            tokio::time::sleep(self.poll_interval).await;
        }
    }

    #[instrument(skip(self))]
    async fn process_next_batch(&self) -> Result<()> {
        let pool = self.connection_pool.get_pool();
        
        // 1. Read the last processed ledger from DB
        let last_ledger: i32 = sqlx::query_scalar("SELECT last_processed_ledger FROM indexer_state WHERE id = 1")
            .fetch_one(pool)
            .await
            .map_err(|e| NetworkError::Internal(format!("Failed to fetch last ledger: {}", e)))?;

        // 2. Poll RPC for the latest ledger
        let latest_ledger = self.stellar_service.get_latest_ledger().await?;
        let latest_sequence = latest_ledger.sequence as i32;

        if last_ledger >= latest_sequence {
            debug!("No new ledgers to process. Last: {}, Latest: {}", last_ledger, latest_sequence);
            return Ok(());
        }

        let start_ledger = last_ledger + 1;
        let end_ledger = (start_ledger + 10).min(latest_sequence); // Process up to 10 ledgers at a time

        info!("Processing ledgers from {} to {}", start_ledger, end_ledger);

        for sequence in start_ledger..=end_ledger {
            self.process_ledger(sequence as u32).await?;
        }

        Ok(())
    }

    async fn process_ledger(&self, sequence: u32) -> Result<()> {
        // In a real implementation, we would fetch events for this ledger from Horizon/Soroban RPC
        // Since we don't have a real Soroban RPC client yet, we'll simulate finding some events
        
        // Simulate fetching events
        let events = self.simulate_fetch_events(sequence).await;
        
        let pool = self.connection_pool.get_pool();
        let mut tx = pool.begin().await
            .map_err(|e| NetworkError::Internal(format!("Failed to begin transaction: {}", e)))?;

        for event in events {
            // Idempotent insert: the composite unique constraint (transaction_hash + event_index)
            // ensures we don't double-count events if we re-process a ledger.
            sqlx::query(
                "INSERT INTO transactions (transaction_hash, event_index, ledger_sequence, event_type, data)
                 VALUES ($1, $2, $3, $4, $5)
                 ON CONFLICT (transaction_hash, event_index) DO NOTHING"
            )
            .bind(&event.transaction_hash)
            .bind(event.event_index)
            .bind(event.ledger_sequence as i32)
            .bind(&event.event_type)
            .bind(&event.data)
            .execute(&mut *tx)
            .await
            .map_err(|e| NetworkError::Internal(format!("Failed to insert event: {}", e)))?;
        }

        // Update the cursor
        sqlx::query("UPDATE indexer_state SET last_processed_ledger = $1, updated_at = CURRENT_TIMESTAMP WHERE id = 1")
            .bind(sequence as i32)
            .execute(&mut *tx)
            .await
            .map_err(|e| NetworkError::Internal(format!("Failed to update indexer state: {}", e)))?;

        tx.commit().await
            .map_err(|e| NetworkError::Internal(format!("Failed to commit transaction: {}", e)))?;

        debug!("Successfully processed ledger {}", sequence);
        Ok(())
    }

    async fn simulate_fetch_events(&self, sequence: u32) -> Vec<IndexedEvent> {
        // This is a placeholder for real Soroban event fetching
        // In a real app, you'd use Horizon or a dedicated Soroban RPC to get events
        let mut events = Vec::new();
        
        // Only "find" events occasionally for simulation
        if sequence % 5 == 0 {
            events.push(IndexedEvent {
                transaction_hash: format!("tx_{:x}", fastrand::u128(..)),
                event_index: 0,
                ledger_sequence: sequence,
                event_type: "Deposit".to_string(),
                data: json!({
                    "user": "GB..." ,
                    "amount": "1000"
                }),
            });
        }
        
        events
    }
}

struct IndexedEvent {
    transaction_hash: String,
    event_index: i32,
    ledger_sequence: u32,
    event_type: String,
    data: serde_json::Value,
}
