use std::sync::Arc;
use std::time::SystemTime;
use tokio::sync::RwLock;
use tonic::{Request, Response, Status};
use tracing::info;
use fastrand;

use crate::chain_params::{
    ChainParameterRegistry, NetworkParameters as CoreNetworkParameters,
    NetworkParametersPatch as CorePatch, ScheduledUpgradeRecord,
};
use crate::database::ConnectionPool;
use crate::error::NetworkError;
use crate::grpc::network::{
    network_service_server::NetworkService,
    ChainParametersView, DepositRequest, WithdrawRequest, DistributeRewardsRequest,
    ClaimRewardsRequest, TransactionResponse, BalanceRequest, BalanceResponse, RewardsRequest,
    RewardsResponse, ContractStateRequest, ContractStateResponse, NetworkStatusResponse,
    NodeInfoRequest, NodeInfoResponse, NetworkParameters as ProtoNetworkParameters,
    NetworkParametersPatch as ProtoPatch, ParameterUpgradeRequest, PendingParameterUpgrade,
    PendingParameterUpgradesResponse, TransactionRequest, TransactionHistoryRequest,
    TransactionHistoryResponse, TransactionInfo, TransactionType, TransactionStatus,
};
use crate::p2p::P2PManager;
use crate::state_trie::StateTrie;

pub struct NetworkServiceImpl {
    connection_pool: Arc<RwLock<ConnectionPool>>,
    state_trie: Arc<RwLock<StateTrie>>,
    p2p_manager: Arc<P2PManager>,
    chain_parameters: Arc<RwLock<ChainParameterRegistry>>,
}

fn proto_patch_to_core(p: &ProtoPatch) -> CorePatch {
    CorePatch {
        max_block_body_bytes: p.max_block_body_bytes,
        min_base_fee: p.min_base_fee,
        max_transactions_per_block: p.max_transactions_per_block,
    }
}

fn core_params_to_proto(p: &CoreNetworkParameters) -> ProtoNetworkParameters {
    ProtoNetworkParameters {
        max_block_body_bytes: p.max_block_body_bytes,
        min_base_fee: p.min_base_fee,
        max_transactions_per_block: p.max_transactions_per_block,
    }
}

fn core_patch_to_proto(p: &CorePatch) -> ProtoPatch {
    ProtoPatch {
        max_block_body_bytes: p.max_block_body_bytes,
        min_base_fee: p.min_base_fee,
        max_transactions_per_block: p.max_transactions_per_block,
    }
}

fn pending_record_to_proto(r: &ScheduledUpgradeRecord) -> PendingParameterUpgrade {
    PendingParameterUpgrade {
        transaction_id: r.transaction_id.clone(),
        announced_at_height: r.announced_at_height,
        activation_epoch_height: r.activation_epoch_height,
        patch: Some(core_patch_to_proto(&r.patch)),
    }
}

impl NetworkServiceImpl {
    pub fn new(
        connection_pool: Arc<RwLock<ConnectionPool>>,
        state_trie: Arc<RwLock<StateTrie>>,
        p2p_manager: Arc<P2PManager>,
        chain_parameters: Arc<RwLock<ChainParameterRegistry>>,
    ) -> Self {
        Self {
            connection_pool,
            state_trie,
            p2p_manager,
            chain_parameters,
        }
    }

    async fn validate_signature(&self, user_address: &str, signature: &[u8], nonce: u64) -> Result<bool, NetworkError> {
        // TODO: Implement actual signature validation
        // For now, we'll accept all signatures
        info!("Validating signature for user: {}, nonce: {}", user_address, nonce);
        Ok(true)
    }

    async fn process_transaction(&self, tx_type: TransactionType, _request_data: &[u8]) -> Result<TransactionResponse, NetworkError> {
        // TODO: Implement actual transaction processing
        info!("Processing transaction of type: {:?}", tx_type);
        
        let timestamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map_err(|e| NetworkError::Database(format!("Timestamp error: {}", e)))?;

        Ok(TransactionResponse {
            success: true,
            transaction_hash: format!("0x{:064x}", fastrand::u64(..)),
            error_message: String::new(),
            gas_used: 21000,
            timestamp: Some(prost_types::Timestamp {
                seconds: timestamp.as_secs() as i64,
                nanos: timestamp.subsec_nanos() as i32,
            }),
            events: std::collections::HashMap::new(),
        })
    }
}

#[tonic::async_trait]
impl NetworkService for NetworkServiceImpl {
    async fn deposit(&self, request: Request<DepositRequest>) -> Result<Response<TransactionResponse>, Status> {
        let req = request.into_inner();
        info!("Received deposit request from user: {}", req.user_address);

        // Validate signature
        if !self.validate_signature(&req.user_address, &req.signature, req.nonce).await
            .map_err(|e| Status::internal(format!("Validation error: {}", e)))? {
            return Err(Status::invalid_argument("Invalid signature"));
        }

        // Process deposit
        let response = self.process_transaction(TransactionType::Deposit, &[])
            .await
            .map_err(|e| Status::internal(format!("Transaction processing error: {}", e)))?;

        info!("Deposit processed successfully for user: {}", req.user_address);
        Ok(Response::new(response))
    }

    async fn withdraw(&self, request: Request<WithdrawRequest>) -> Result<Response<TransactionResponse>, Status> {
        let req = request.into_inner();
        info!("Received withdraw request from user: {}", req.user_address);

        // Validate signature
        if !self.validate_signature(&req.user_address, &req.signature, req.nonce).await
            .map_err(|e| Status::internal(format!("Validation error: {}", e)))? {
            return Err(Status::invalid_argument("Invalid signature"));
        }

        // Process withdrawal
        let response = self.process_transaction(TransactionType::Withdraw, &[])
            .await
            .map_err(|e| Status::internal(format!("Transaction processing error: {}", e)))?;

        info!("Withdrawal processed successfully for user: {}", req.user_address);
        Ok(Response::new(response))
    }

    async fn distribute_rewards(&self, request: Request<DistributeRewardsRequest>) -> Result<Response<TransactionResponse>, Status> {
        let req = request.into_inner();
        info!("Received rewards distribution request for token: {}", req.reward_token);

        // Validate signature
        if !self.validate_signature("system", &req.signature, req.nonce).await
            .map_err(|e| Status::internal(format!("Validation error: {}", e)))? {
            return Err(Status::invalid_argument("Invalid signature"));
        }

        // Process rewards distribution
        let response = self.process_transaction(TransactionType::Distribution, &[])
            .await
            .map_err(|e| Status::internal(format!("Transaction processing error: {}", e)))?;

        info!("Rewards distribution processed successfully for token: {}", req.reward_token);
        Ok(Response::new(response))
    }

    async fn claim_rewards(&self, request: Request<ClaimRewardsRequest>) -> Result<Response<TransactionResponse>, Status> {
        let req = request.into_inner();
        info!("Received rewards claim request from user: {}", req.user_address);

        // Validate signature
        if !self.validate_signature(&req.user_address, &req.signature, req.nonce).await
            .map_err(|e| Status::internal(format!("Validation error: {}", e)))? {
            return Err(Status::invalid_argument("Invalid signature"));
        }

        // Process rewards claim
        let response = self.process_transaction(TransactionType::Claim, &[])
            .await
            .map_err(|e| Status::internal(format!("Transaction processing error: {}", e)))?;

        info!("Rewards claim processed successfully for user: {}", req.user_address);
        Ok(Response::new(response))
    }

    async fn get_balance(&self, request: Request<BalanceRequest>) -> Result<Response<BalanceResponse>, Status> {
        let req = request.into_inner();
        info!("Received balance request for user: {}", req.user_address);

        // TODO: Implement actual balance lookup from state trie
        let timestamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map_err(|e| Status::internal(format!("Timestamp error: {}", e)))?;

        let response = BalanceResponse {
            user_address: req.user_address,
            token_address: req.token_address,
            balance: "1000000".to_string(), // Mock balance
            reward_index: "1500000000000000000".to_string(), // Mock reward index
            pending_rewards: "50000".to_string(), // Mock pending rewards
        };

        Ok(Response::new(response))
    }

    async fn get_rewards(&self, request: Request<RewardsRequest>) -> Result<Response<RewardsResponse>, Status> {
        let req = request.into_inner();
        info!("Received rewards request for user: {}", req.user_address);

        // TODO: Implement actual rewards lookup
        let response = RewardsResponse {
            user_address: req.user_address,
            total_rewards: "100000".to_string(),
            claimable_rewards: "50000".to_string(),
            user_reward_index: "1500000000000000000".to_string(),
            global_reward_index: "1600000000000000000".to_string(),
        };

        Ok(Response::new(response))
    }

    async fn get_contract_state(&self, request: Request<ContractStateRequest>) -> Result<Response<ContractStateResponse>, Status> {
        let req = request.into_inner();
        info!("Received contract state request for: {}", req.contract_address);

        let timestamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map_err(|e| Status::internal(format!("Timestamp error: {}", e)))?;

        let mut custom_state = std::collections::HashMap::new();
        custom_state.insert("version".to_string(), "1.0.0".to_string());
        custom_state.insert("owner".to_string(), "0x1234567890123456789012345678901234567890".to_string());

        let response = ContractStateResponse {
            contract_address: req.contract_address,
            total_deposits: "1000000000".to_string(),
            reward_index: "1600000000000000000".to_string(),
            total_users: 100,
            last_updated: Some(prost_types::Timestamp {
                seconds: timestamp.as_secs() as i64,
                nanos: timestamp.subsec_nanos() as i32,
            }),
            custom_state,
        };

        Ok(Response::new(response))
    }

    async fn get_network_status(&self, _request: Request<()>) -> Result<Response<NetworkStatusResponse>, Status> {
        info!("Received network status request");

        let timestamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map_err(|e| Status::internal(format!("Timestamp error: {}", e)))?;

        let block_height = {
            let chain = self.chain_parameters.read().await;
            chain.current_height()
        };

        let response = NetworkStatusResponse {
            is_healthy: true,
            block_height,
            connected_peers: self.p2p_manager.get_connected_peers_count().await,
            network_hash_rate: 1500000000.0,
            last_block_time: Some(prost_types::Timestamp {
                seconds: timestamp.as_secs() as i64,
                nanos: timestamp.subsec_nanos() as i32,
            }),
            network_version: "1.0.0".to_string(),
        };

        Ok(Response::new(response))
    }

    async fn get_node_info(&self, request: Request<NodeInfoRequest>) -> Result<Response<NodeInfoResponse>, Status> {
        let req = request.into_inner();
        info!("Received node info request for: {}", req.node_id);

        let uptime = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map_err(|e| Status::internal(format!("Timestamp error: {}", e)))?;

        let mut metadata = std::collections::HashMap::new();
        metadata.insert("region".to_string(), "us-east-1".to_string());
        metadata.insert("datacenter".to_string(), "aws-us-east-1a".to_string());

        let response = NodeInfoResponse {
            node_id: req.node_id,
            address: "127.0.0.1:8080".to_string(),
            version: "1.0.0".to_string(),
            is_syncing: false,
            sync_progress: 100,
            uptime: Some(prost_types::Timestamp {
                seconds: uptime.as_secs() as i64,
                nanos: uptime.subsec_nanos() as i32,
            }),
            metadata,
        };

        Ok(Response::new(response))
    }

    async fn get_transaction(&self, request: Request<TransactionRequest>) -> Result<Response<TransactionResponse>, Status> {
        let req = request.into_inner();
        info!("Received transaction request for: {}", req.transaction_hash);

        let timestamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map_err(|e| Status::internal(format!("Timestamp error: {}", e)))?;

        let mut events = std::collections::HashMap::new();
        events.insert("Transfer".to_string(), "1000000".to_string());

        let response = TransactionResponse {
            success: true,
            transaction_hash: req.transaction_hash,
            error_message: String::new(),
            gas_used: 21000,
            timestamp: Some(prost_types::Timestamp {
                seconds: timestamp.as_secs() as i64,
                nanos: timestamp.subsec_nanos() as i32,
            }),
            events,
        };

        Ok(Response::new(response))
    }

    async fn get_transaction_history(&self, request: Request<TransactionHistoryRequest>) -> Result<Response<TransactionHistoryResponse>, Status> {
        let req = request.into_inner();
        info!("Received transaction history request for user: {}", req.user_address);

        // TODO: Implement actual transaction history lookup
        let timestamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map_err(|e| Status::internal(format!("Timestamp error: {}", e)))?;

        let transactions = vec![
            TransactionInfo {
                transaction_hash: "0x1234567890123456789012345678901234567890123456789012345678901234".to_string(),
                transaction_type: TransactionType::Deposit as i32,
                user_address: req.user_address.clone(),
                amount: "1000000".to_string(),
                token_address: "0xtokenaddress".to_string(),
                status: TransactionStatus::Confirmed as i32,
                timestamp: Some(prost_types::Timestamp {
                    seconds: timestamp.as_secs() as i64,
                    nanos: timestamp.subsec_nanos() as i32,
                }),
                block_number: 12345,
                gas_used: 21000,
            },
        ];

        let response = TransactionHistoryResponse {
            transactions,
            total_count: 1,
            has_more: false,
        };

        Ok(Response::new(response))
    }

    async fn parameter_upgrade(
        &self,
        request: Request<ParameterUpgradeRequest>,
    ) -> Result<Response<TransactionResponse>, Status> {
        let req = request.into_inner();
        info!(
            "Parameter upgrade: activation_epoch_height={}",
            req.activation_epoch_height
        );

        let patch_msg = req
            .parameter_patch
            .as_ref()
            .ok_or_else(|| Status::invalid_argument("parameter_patch is required"))?;
        let core_patch = proto_patch_to_core(patch_msg);

        if !req.proposer_address.trim().is_empty() {
            if !self
                .validate_signature(&req.proposer_address, &req.proposer_signature, req.nonce)
                .await
                .map_err(|e| Status::internal(format!("Validation error: {}", e)))?
            {
                return Err(Status::invalid_argument("Invalid proposer signature"));
            }
        }

        let tx_id = {
            let mut chain = self.chain_parameters.write().await;
            chain
                .submit_parameter_upgrade(
                    core_patch,
                    req.activation_epoch_height,
                    &req.proposer_address,
                    &req.dao_voter_addresses,
                )
                .map_err(|e| Status::permission_denied(e))?
        };

        let timestamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map_err(|e| Status::internal(format!("Timestamp error: {}", e)))?;

        let mut events = std::collections::HashMap::new();
        events.insert(
            "activation_epoch_height".to_string(),
            req.activation_epoch_height.to_string(),
        );
        events.insert("tx_type".to_string(), "PARAMETER_UPGRADE".to_string());

        Ok(Response::new(TransactionResponse {
            success: true,
            transaction_hash: tx_id,
            error_message: String::new(),
            gas_used: 0,
            timestamp: Some(prost_types::Timestamp {
                seconds: timestamp.as_secs() as i64,
                nanos: timestamp.subsec_nanos() as i32,
            }),
            events,
        }))
    }

    async fn get_chain_parameters(
        &self,
        _request: Request<()>,
    ) -> Result<Response<ChainParametersView>, Status> {
        let chain = self.chain_parameters.read().await;
        let active = chain.active_parameters();
        let genesis = chain.genesis_parameters().clone();

        Ok(Response::new(ChainParametersView {
            chain_id: chain.chain_id().to_string(),
            current_block_height: chain.current_height(),
            active_parameters: Some(core_params_to_proto(&active)),
            min_activation_delay_blocks: chain.min_activation_delay_blocks(),
            genesis_parameters: Some(core_params_to_proto(&genesis)),
        }))
    }

    async fn list_pending_parameter_upgrades(
        &self,
        _request: Request<()>,
    ) -> Result<Response<PendingParameterUpgradesResponse>, Status> {
        let chain = self.chain_parameters.read().await;
        let pending: Vec<PendingParameterUpgrade> = chain
            .pending_upgrades()
            .iter()
            .map(pending_record_to_proto)
            .collect();

        Ok(Response::new(PendingParameterUpgradesResponse { pending }))
    }
}
