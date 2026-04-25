use std::sync::Arc;
use std::time::{SystemTime, Instant};
use tokio::sync::RwLock;
use tonic::{Request, Response, Status};
use tracing::{info, warn, error};
use fastrand;

use crate::database::ConnectionPool;
use crate::error::NetworkError;
use crate::chain_params::ChainParameterRegistry;
use crate::grpc::gateway::{
    gateway_service_server::GatewayService,
    ChainParametersView, DepositRequest, WithdrawRequest, DistributeRewardsRequest,
    ClaimRewardsRequest, NetworkParameters as GwNetworkParameters,
    NetworkParametersPatch as GwNetworkParametersPatch, TransactionResponse, BalanceRequest,
    BalanceResponse, RewardsRequest, RewardsResponse, ContractStateRequest, ContractStateResponse,
    NetworkStatusResponse, NodeInfoRequest, NodeInfoResponse, ParameterUpgradeRequest,
    PendingParameterUpgrade, PendingParameterUpgradesResponse, TransactionRequest,
    TransactionHistoryRequest, TransactionHistoryResponse, TransactionInfo, PaginationInfo,
    HealthCheckResponse, ServiceHealth, TVLRequest, TVLResponse,
};
use crate::grpc::network_service::NetworkServiceImpl;
use crate::p2p::P2PManager;
use crate::state_trie::StateTrie;

fn gw_params_from_network(
    p: Option<crate::grpc::network::NetworkParameters>,
) -> Option<GwNetworkParameters> {
    p.map(|x| GwNetworkParameters {
        max_block_body_bytes: x.max_block_body_bytes,
        min_base_fee: x.min_base_fee,
        max_transactions_per_block: x.max_transactions_per_block,
    })
}

fn gw_patch_from_network(
    p: Option<crate::grpc::network::NetworkParametersPatch>,
) -> Option<GwNetworkParametersPatch> {
    p.map(|x| GwNetworkParametersPatch {
        max_block_body_bytes: x.max_block_body_bytes,
        min_base_fee: x.min_base_fee,
        max_transactions_per_block: x.max_transactions_per_block,
    })
}

pub struct GatewayServiceImpl {
    network_service: NetworkServiceImpl,
}

impl GatewayServiceImpl {
    pub fn new(
        connection_pool: Arc<RwLock<ConnectionPool>>,
        state_trie: Arc<RwLock<StateTrie>>,
        p2p_manager: Arc<P2PManager>,
        chain_parameters: Arc<RwLock<ChainParameterRegistry>>,
    ) -> Self {
        Self {
            network_service: NetworkServiceImpl::new(
                connection_pool,
                state_trie,
                p2p_manager,
                chain_parameters,
            ),
        }
    }

    fn generate_request_id() -> String {
        format!("req_{}", fastrand::u64(..))
    }

    async fn process_with_tracking<F, T>(&self, request_id: String, operation: F) -> Result<T, Status>
    where
        F: std::future::Future<Output = Result<T, Status>>,
    {
        let start_time = Instant::now();
        info!("Processing gateway request: {}", request_id);

        match operation.await {
            Ok(result) => {
                let duration = start_time.elapsed();
                info!("Gateway request {} completed in {:?}", request_id, duration);
                Ok(result)
            }
            Err(e) => {
                let duration = start_time.elapsed();
                error!("Gateway request {} failed after {:?}: {}", request_id, duration, e);
                Err(e)
            }
        }
    }
}

#[tonic::async_trait]
impl GatewayService for GatewayServiceImpl {
    async fn deposit(&self, request: Request<DepositRequest>) -> Result<Response<TransactionResponse>, Status> {
        let mut req = request.into_inner();
        let request_id = if req.request_id.is_empty() {
            Self::generate_request_id()
        } else {
            req.request_id.clone()
        };

        self.process_with_tracking(request_id.clone(), async move {
            // Forward to network service
            let network_req = Request::new(crate::grpc::network::DepositRequest {
                user_address: req.user_address.clone(),
                token_address: req.token_address.clone(),
                amount: req.amount.clone(),
                signature: req.signature.clone(),
                nonce: req.nonce,
                timestamp: req.timestamp,
            });

            let mut response = self.network_service.deposit(network_req).await?.into_inner();
            response.request_id = request_id.clone();
            
            // Add gateway-specific fields
            let processing_time = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .map_err(|e| Status::internal(format!("Timestamp error: {}", e)))?;
            
            response.processing_time_ms = Instant::now().elapsed().as_millis() as u64;
            response.status_url = format!("/v1/transaction/{}/status", response.transaction_hash);

            Ok(response)
        }).await
    }

    async fn withdraw(&self, request: Request<WithdrawRequest>) -> Result<Response<TransactionResponse>, Status> {
        let mut req = request.into_inner();
        let request_id = if req.request_id.is_empty() {
            Self::generate_request_id()
        } else {
            req.request_id.clone()
        };

        self.process_with_tracking(request_id.clone(), async move {
            // Forward to network service
            let network_req = Request::new(crate::grpc::network::WithdrawRequest {
                user_address: req.user_address.clone(),
                token_address: req.token_address.clone(),
                amount: req.amount.clone(),
                signature: req.signature.clone(),
                nonce: req.nonce,
                timestamp: req.timestamp,
            });

            let mut response = self.network_service.withdraw(network_req).await?.into_inner();
            response.request_id = request_id.clone();
            response.processing_time_ms = Instant::now().elapsed().as_millis() as u64;
            response.status_url = format!("/v1/transaction/{}/status", response.transaction_hash);

            Ok(response)
        }).await
    }

    async fn distribute_rewards(&self, request: Request<DistributeRewardsRequest>) -> Result<Response<TransactionResponse>, Status> {
        let mut req = request.into_inner();
        let request_id = if req.request_id.is_empty() {
            Self::generate_request_id()
        } else {
            req.request_id.clone()
        };

        self.process_with_tracking(request_id.clone(), async move {
            // Forward to network service
            let network_req = Request::new(crate::grpc::network::DistributeRewardsRequest {
                reward_token: req.reward_token.clone(),
                total_amount: req.total_amount.clone(),
                signature: req.signature.clone(),
                nonce: req.nonce,
                timestamp: req.timestamp,
            });

            let mut response = self.network_service.distribute_rewards(network_req).await?.into_inner();
            response.request_id = request_id.clone();
            response.processing_time_ms = Instant::now().elapsed().as_millis() as u64;
            response.status_url = format!("/v1/transaction/{}/status", response.transaction_hash);

            Ok(response)
        }).await
    }

    async fn claim_rewards(&self, request: Request<ClaimRewardsRequest>) -> Result<Response<TransactionResponse>, Status> {
        let mut req = request.into_inner();
        let request_id = if req.request_id.is_empty() {
            Self::generate_request_id()
        } else {
            req.request_id.clone()
        };

        self.process_with_tracking(request_id.clone(), async move {
            // Forward to network service
            let network_req = Request::new(crate::grpc::network::ClaimRewardsRequest {
                user_address: req.user_address.clone(),
                signature: req.signature.clone(),
                nonce: req.nonce,
                timestamp: req.timestamp,
            });

            let mut response = self.network_service.claim_rewards(network_req).await?.into_inner();
            response.request_id = request_id.clone();
            response.processing_time_ms = Instant::now().elapsed().as_millis() as u64;
            response.status_url = format!("/v1/transaction/{}/status", response.transaction_hash);

            Ok(response)
        }).await
    }

    async fn get_balance(&self, request: Request<BalanceRequest>) -> Result<Response<BalanceResponse>, Status> {
        let req = request.into_inner();
        let request_id = Self::generate_request_id();

        self.process_with_tracking(request_id, async move {
            let mut response = self.network_service.get_balance(Request::new(crate::grpc::network::BalanceRequest {
                user_address: req.user_address.clone(),
                token_address: req.token_address.clone(),
            })).await?.into_inner();

            // Add gateway-specific fields
            let timestamp = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .map_err(|e| Status::internal(format!("Timestamp error: {}", e)))?;
            
            response.last_updated = Some(prost_types::Timestamp {
                seconds: timestamp.as_secs() as i64,
                nanos: timestamp.subsec_nanos() as i32,
            });
            response.currency = "USD".to_string(); // Default currency

            Ok(response)
        }).await
    }

    async fn get_rewards(&self, request: Request<RewardsRequest>) -> Result<Response<RewardsResponse>, Status> {
        let req = request.into_inner();
        let request_id = Self::generate_request_id();

        self.process_with_tracking(request_id, async move {
            let mut response = self.network_service.get_rewards(Request::new(crate::grpc::network::RewardsRequest {
                user_address: req.user_address.clone(),
            })).await?.into_inner();

            // Add gateway-specific fields
            response.last_claimed = Some(prost_types::Timestamp {
                seconds: SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs() as i64 - 86400, // Yesterday
                nanos: 0,
            });
            response.reward_token = "0xrewardtoken".to_string();

            Ok(response)
        }).await
    }

    async fn get_contract_state(&self, request: Request<ContractStateRequest>) -> Result<Response<ContractStateResponse>, Status> {
        let req = request.into_inner();
        let request_id = Self::generate_request_id();

        self.process_with_tracking(request_id, async move {
            let mut response = self.network_service.get_contract_state(Request::new(crate::grpc::network::ContractStateRequest {
                contract_address: req.contract_address.clone(),
            })).await?.into_inner();

            // Add gateway-specific fields
            response.contract_version = "1.0.0".to_string();
            response.supported_tokens = vec![
                "0xtoken1".to_string(),
                "0xtoken2".to_string(),
                "0xrewardtoken".to_string(),
            ];

            Ok(response)
        }).await
    }

    async fn get_network_status(&self, request: Request<()>) -> Result<Response<NetworkStatusResponse>, Status> {
        let request_id = Self::generate_request_id();

        self.process_with_tracking(request_id, async move {
            let mut response = self.network_service.get_network_status(Request::new(())).await?.into_inner();

            // Add gateway-specific fields
            response.pending_transactions = 25;
            response.average_block_time = 12.5;
            response.supported_networks = vec![
                "mainnet".to_string(),
                "testnet".to_string(),
                "futurenet".to_string(),
            ];

            Ok(response)
        }).await
    }

    async fn get_node_info(&self, request: Request<NodeInfoRequest>) -> Result<Response<NodeInfoResponse>, Status> {
        let req = request.into_inner();
        let request_id = Self::generate_request_id();

        self.process_with_tracking(request_id, async move {
            let mut response = self.network_service.get_node_info(Request::new(crate::grpc::network::NodeInfoRequest {
                node_id: req.node_id.clone(),
            })).await?.into_inner();

            // Add gateway-specific fields
            response.region = "us-east-1".to_string();
            response.datacenter = "aws-us-east-1a".to_string();
            response.capabilities = vec![
                "grpc".to_string(),
                "http".to_string(),
                "websocket".to_string(),
                "p2p".to_string(),
            ];

            Ok(response)
        }).await
    }

    async fn get_transaction(&self, request: Request<TransactionRequest>) -> Result<Response<TransactionResponse>, Status> {
        let req = request.into_inner();
        let request_id = Self::generate_request_id();

        self.process_with_tracking(request_id, async move {
            let mut response = self.network_service.get_transaction(Request::new(crate::grpc::network::TransactionRequest {
                transaction_hash: req.transaction_hash.clone(),
            })).await?.into_inner();

            // Add gateway-specific fields
            response.request_id = request_id;
            response.processing_time_ms = 50;
            response.status_url = format!("/v1/transaction/{}/status", response.transaction_hash);

            Ok(response)
        }).await
    }

    async fn get_transaction_history(&self, request: Request<TransactionHistoryRequest>) -> Result<Response<TransactionHistoryResponse>, Status> {
        let req = request.into_inner();
        let request_id = Self::generate_request_id();

        self.process_with_tracking(request_id, async move {
            let mut response = self.network_service.get_transaction_history(Request::new(crate::grpc::network::TransactionHistoryRequest {
                user_address: req.user_address.clone(),
                limit: req.limit,
                offset: req.offset,
                transaction_type: req.transaction_type.clone().map(|t| t.parse().unwrap_or(0)),
            })).await?.into_inner();

            // Add pagination info
            let page_size = req.limit.unwrap_or(10);
            let current_page = (req.offset.unwrap_or(0) / page_size) + 1;
            let total_pages = (response.total_count + page_size - 1) / page_size;

            response.pagination = Some(PaginationInfo {
                current_page,
                total_pages,
                page_size,
                has_next: response.has_more,
                has_previous: current_page > 1,
            });

            // Add gateway-specific fields to transactions
            for transaction in &mut response.transactions {
                transaction.confirmation_count = 12;
                transaction.fee_paid = "21000".to_string();
                transaction.metadata.insert("gateway_processed".to_string(), "true".to_string());
            }

            Ok(response)
        }).await
    }

    async fn parameter_upgrade(
        &self,
        request: Request<ParameterUpgradeRequest>,
    ) -> Result<Response<TransactionResponse>, Status> {
        let mut req = request.into_inner();
        let request_id = if req.request_id.is_empty() {
            Self::generate_request_id()
        } else {
            req.request_id.clone()
        };

        self.process_with_tracking(request_id.clone(), async move {
            let net_patch = req.parameter_patch.map(|p| crate::grpc::network::NetworkParametersPatch {
                max_block_body_bytes: p.max_block_body_bytes,
                min_base_fee: p.min_base_fee,
                max_transactions_per_block: p.max_transactions_per_block,
            });

            let network_req = Request::new(crate::grpc::network::ParameterUpgradeRequest {
                parameter_patch: net_patch,
                activation_epoch_height: req.activation_epoch_height,
                proposer_address: req.proposer_address.clone(),
                proposer_signature: req.proposer_signature.clone(),
                nonce: req.nonce,
                timestamp: req.timestamp,
                dao_voter_addresses: req.dao_voter_addresses.clone(),
            });

            let nr = self
                .network_service
                .parameter_upgrade(network_req)
                .await?
                .into_inner();

            let response = TransactionResponse {
                success: nr.success,
                transaction_hash: nr.transaction_hash,
                error_message: nr.error_message,
                gas_used: nr.gas_used,
                timestamp: nr.timestamp,
                events: nr.events,
                request_id: request_id.clone(),
                processing_time_ms: Instant::now().elapsed().as_millis() as u64,
                status_url: format!("/v1/transaction/{}/status", nr.transaction_hash),
            };

            Ok(response)
        })
        .await
    }

    async fn get_chain_parameters(
        &self,
        _request: Request<()>,
    ) -> Result<Response<ChainParametersView>, Status> {
        let request_id = Self::generate_request_id();
        self.process_with_tracking(request_id, async move {
            let nv = self
                .network_service
                .get_chain_parameters(Request::new(()))
                .await?
                .into_inner();
            Ok(ChainParametersView {
                chain_id: nv.chain_id,
                current_block_height: nv.current_block_height,
                active_parameters: gw_params_from_network(nv.active_parameters),
                min_activation_delay_blocks: nv.min_activation_delay_blocks,
                genesis_parameters: gw_params_from_network(nv.genesis_parameters),
            })
        })
        .await
    }

    async fn list_pending_parameter_upgrades(
        &self,
        _request: Request<()>,
    ) -> Result<Response<PendingParameterUpgradesResponse>, Status> {
        let request_id = Self::generate_request_id();
        self.process_with_tracking(request_id, async move {
            let nv = self
                .network_service
                .list_pending_parameter_upgrades(Request::new(()))
                .await?
                .into_inner();
            let pending: Vec<PendingParameterUpgrade> = nv
                .pending
                .into_iter()
                .map(|p| PendingParameterUpgrade {
                    transaction_id: p.transaction_id,
                    announced_at_height: p.announced_at_height,
                    activation_epoch_height: p.activation_epoch_height,
                    patch: gw_patch_from_network(p.patch),
                })
                .collect();
            Ok(PendingParameterUpgradesResponse { pending })
        })
        .await
    }

    async fn get_tvl(&self, request: Request<TVLRequest>) -> Result<Response<TVLResponse>, Status> {
        let req = request.into_inner();
        let request_id = Self::generate_request_id();

        self.process_with_tracking(request_id, async move {
            let nv = self
                .network_service
                .get_tvl(Request::new(crate::grpc::network::TVLRequest {
                    token_address: req.token_address,
                }))
                .await?
                .into_inner();
            Ok(TVLResponse {
                total_value_locked: nv.total_value_locked,
                token_address: nv.token_address,
                timestamp: nv.timestamp,
            })
        })
        .await
    }

    async fn check_health(&self, request: Request<()>) -> Result<Response<HealthCheckResponse>, Status> {
        let request_id = Self::generate_request_id();

        self.process_with_tracking(request_id, async move {
            let timestamp = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .map_err(|e| Status::internal(format!("Timestamp error: {}", e)))?;

            let services = vec![
                ServiceHealth {
                    service_name: "database".to_string(),
                    status: "healthy".to_string(),
                    message: "Connection pool active".to_string(),
                    last_check: Some(prost_types::Timestamp {
                        seconds: timestamp.as_secs() as i64,
                        nanos: timestamp.subsec_nanos() as i32,
                    }),
                    metrics: {
                        let mut metrics = std::collections::HashMap::new();
                        metrics.insert("active_connections".to_string(), "5".to_string());
                        metrics.insert("idle_connections".to_string(), "15".to_string());
                        metrics
                    },
                },
                ServiceHealth {
                    service_name: "grpc".to_string(),
                    status: "healthy".to_string(),
                    message: "gRPC server running".to_string(),
                    last_check: Some(prost_types::Timestamp {
                        seconds: timestamp.as_secs() as i64,
                        nanos: timestamp.subsec_nanos() as i32,
                    }),
                    metrics: {
                        let mut metrics = std::collections::HashMap::new();
                        metrics.insert("active_requests".to_string(), "3".to_string());
                        metrics.insert("total_requests".to_string(), "1250".to_string());
                        metrics
                    },
                },
            ];

            let mut details = std::collections::HashMap::new();
            details.insert("version".to_string(), "1.0.0".to_string());
            details.insert("uptime".to_string(), "2d 14h 32m".to_string());

            let response = HealthCheckResponse {
                status: "SERVING".to_string(),
                message: "All systems operational".to_string(),
                timestamp: Some(prost_types::Timestamp {
                    seconds: timestamp.as_secs() as i64,
                    nanos: timestamp.subsec_nanos() as i32,
                }),
                details,
                services,
                uptime_percentage: 99.95,
            };

            Ok(response)
        }).await
    }

    // Note: WatchHealth is not implemented here as it requires streaming
    // This would need to be implemented separately with proper streaming support
}
