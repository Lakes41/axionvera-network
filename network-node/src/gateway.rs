use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::Json,
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use utoipa::ToSchema;

use crate::grpc::gateway::{
    DepositRequest, WithdrawRequest, DistributeRewardsRequest, ClaimRewardsRequest,
    TransactionResponse, BalanceRequest, BalanceResponse, RewardsRequest, RewardsResponse,
    ContractStateRequest, ContractStateResponse, NetworkStatusResponse, NodeInfoRequest,
    NodeInfoResponse, TransactionRequest, TransactionHistoryRequest, TransactionHistoryResponse,
    HealthCheckResponse,
};
use crate::grpc::GatewayServiceImpl;
use crate::error::NetworkError;

#[derive(Debug, Deserialize, ToSchema)]
pub struct PaginationQuery {
    pub limit: Option<u64>,
    pub offset: Option<u64>,
    pub transaction_type: Option<String>,
    pub status: Option<String>,
    pub start_date: Option<String>,
    pub end_date: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct BalanceQuery {
    pub user_address: String,
    pub token_address: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct RewardsQuery {
    pub user_address: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct ContractStateQuery {
    pub contract_address: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct NodeInfoQuery {
    pub node_id: String,
}

/// Deposit tokens into the vault
#[utoipa::path(
    post,
    path = "/v1/contract/deposit",
    tag = "contract",
    request_body = DepositRequest,
    responses(
        (status = 200, description = "Deposit successful", body = TransactionResponse),
        (status = 400, description = "Invalid request"),
        (status = 500, description = "Internal server error")
    )
)]
#[tracing::instrument(skip(gateway_service, request), fields(request_id = %uuid::Uuid::new_v4()))]
pub async fn deposit(
    State(gateway_service): State<GatewayServiceImpl>,
    Json(request): Json<DepositRequest>,
) -> Result<Json<TransactionResponse>, StatusCode> {
    match gateway_service.deposit(axum::extract::Request::new(request)).await {
        Ok(response) => Ok(Json(response.into_inner())),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

/// Withdraw tokens from the vault
#[utoipa::path(
    post,
    path = "/v1/contract/withdraw",
    tag = "contract",
    request_body = WithdrawRequest,
    responses(
        (status = 200, description = "Withdrawal successful", body = TransactionResponse),
        (status = 400, description = "Invalid request"),
        (status = 500, description = "Internal server error")
    )
)]
#[tracing::instrument(skip(gateway_service, request), fields(request_id = %uuid::Uuid::new_v4()))]
pub async fn withdraw(
    State(gateway_service): State<GatewayServiceImpl>,
    Json(request): Json<WithdrawRequest>,
) -> Result<Json<TransactionResponse>, StatusCode> {
    match gateway_service.withdraw(axum::extract::Request::new(request)).await {
        Ok(response) => Ok(Json(response.into_inner())),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

/// Distribute rewards to all vault users
#[utoipa::path(
    post,
    path = "/v1/contract/distribute-rewards",
    tag = "contract",
    request_body = DistributeRewardsRequest,
    responses(
        (status = 200, description = "Rewards distributed successfully", body = TransactionResponse),
        (status = 400, description = "Invalid request"),
        (status = 500, description = "Internal server error")
    )
)]
#[tracing::instrument(skip(gateway_service, request), fields(request_id = %uuid::Uuid::new_v4()))]
pub async fn distribute_rewards(
    State(gateway_service): State<GatewayServiceImpl>,
    Json(request): Json<DistributeRewardsRequest>,
) -> Result<Json<TransactionResponse>, StatusCode> {
    match gateway_service.distribute_rewards(axum::extract::Request::new(request)).await {
        Ok(response) => Ok(Json(response.into_inner())),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

/// Claim pending rewards
#[utoipa::path(
    post,
    path = "/v1/contract/claim-rewards",
    tag = "contract",
    request_body = ClaimRewardsRequest,
    responses(
        (status = 200, description = "Rewards claimed successfully", body = TransactionResponse),
        (status = 400, description = "Invalid request"),
        (status = 500, description = "Internal server error")
    )
)]
#[tracing::instrument(skip(gateway_service, request), fields(request_id = %uuid::Uuid::new_v4()))]
pub async fn claim_rewards(
    State(gateway_service): State<GatewayServiceImpl>,
    Json(request): Json<ClaimRewardsRequest>,
) -> Result<Json<TransactionResponse>, StatusCode> {
    match gateway_service.claim_rewards(axum::extract::Request::new(request)).await {
        Ok(response) => Ok(Json(response.into_inner())),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

/// Get user balance
#[utoipa::path(
    get,
    path = "/v1/query/balance",
    tag = "query",
    params(BalanceQuery),
    responses(
        (status = 200, description = "Balance retrieved successfully", body = BalanceResponse),
        (status = 400, description = "Invalid request"),
        (status = 500, description = "Internal server error")
    )
)]
#[tracing::instrument(skip(gateway_service), fields(request_id = %uuid::Uuid::new_v4(), user_public_key = %query.user_address))]
pub async fn get_balance(
    State(gateway_service): State<GatewayServiceImpl>,
    Query(query): Query<BalanceQuery>,
) -> Result<Json<BalanceResponse>, StatusCode> {
    let request = BalanceRequest {
        user_address: query.user_address,
        token_address: query.token_address,
    };
    
    match gateway_service.get_balance(axum::extract::Request::new(request)).await {
        Ok(response) => Ok(Json(response.into_inner())),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

/// Get user rewards
#[utoipa::path(
    get,
    path = "/v1/query/rewards",
    tag = "query",
    params(RewardsQuery),
    responses(
        (status = 200, description = "Rewards retrieved successfully", body = RewardsResponse),
        (status = 400, description = "Invalid request"),
        (status = 500, description = "Internal server error")
    )
)]
#[tracing::instrument(skip(gateway_service), fields(request_id = %uuid::Uuid::new_v4(), user_public_key = %query.user_address))]
pub async fn get_rewards(
    State(gateway_service): State<GatewayServiceImpl>,
    Query(query): Query<RewardsQuery>,
) -> Result<Json<RewardsResponse>, StatusCode> {
    let request = RewardsRequest {
        user_address: query.user_address,
    };
    
    match gateway_service.get_rewards(axum::extract::Request::new(request)).await {
        Ok(response) => Ok(Json(response.into_inner())),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

/// Get contract state
#[utoipa::path(
    get,
    path = "/v1/query/contract-state",
    tag = "query",
    params(ContractStateQuery),
    responses(
        (status = 200, description = "Contract state retrieved successfully", body = ContractStateResponse),
        (status = 400, description = "Invalid request"),
        (status = 500, description = "Internal server error")
    )
)]
#[tracing::instrument(skip(gateway_service), fields(request_id = %uuid::Uuid::new_v4()))]
pub async fn get_contract_state(
    State(gateway_service): State<GatewayServiceImpl>,
    Query(query): Query<ContractStateQuery>,
) -> Result<Json<ContractStateResponse>, StatusCode> {
    let request = ContractStateRequest {
        contract_address: query.contract_address,
    };
    
    match gateway_service.get_contract_state(axum::extract::Request::new(request)).await {
        Ok(response) => Ok(Json(response.into_inner())),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

/// Get transaction details
#[utoipa::path(
    get,
    path = "/v1/transaction/{transaction_hash}",
    tag = "query",
    params(
        ("transaction_hash" = String, Path, description = "Transaction hash")
    ),
    responses(
        (status = 200, description = "Transaction retrieved successfully", body = TransactionResponse),
        (status = 404, description = "Transaction not found"),
        (status = 500, description = "Internal server error")
    )
)]
#[tracing::instrument(skip(gateway_service), fields(request_id = %uuid::Uuid::new_v4()))]
pub async fn get_transaction(
    State(gateway_service): State<GatewayServiceImpl>,
    Path(transaction_hash): Path<String>,
) -> Result<Json<TransactionResponse>, StatusCode> {
    let request = TransactionRequest {
        transaction_hash,
    };
    
    match gateway_service.get_transaction(axum::extract::Request::new(request)).await {
        Ok(response) => Ok(Json(response.into_inner())),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

/// Get transaction history
#[utoipa::path(
    get,
    path = "/v1/transaction/history",
    tag = "query",
    params(PaginationQuery),
    responses(
        (status = 200, description = "Transaction history retrieved successfully", body = TransactionHistoryResponse),
        (status = 400, description = "Invalid request"),
        (status = 500, description = "Internal server error")
    )
)]
#[tracing::instrument(skip(gateway_service), fields(request_id = %uuid::Uuid::new_v4()))]
pub async fn get_transaction_history(
    State(gateway_service): State<GatewayServiceImpl>,
    Query(query): Query<PaginationQuery>,
) -> Result<Json<TransactionHistoryResponse>, StatusCode> {
    let request = TransactionHistoryRequest {
        user_address: "".to_string(), // This should be extracted from auth context
        limit: query.limit,
        offset: query.offset,
        transaction_type: query.transaction_type,
        status: query.status,
        start_date: None, // Parse from string if needed
        end_date: None,   // Parse from string if needed
    };
    
    match gateway_service.get_transaction_history(axum::extract::Request::new(request)).await {
        Ok(response) => Ok(Json(response.into_inner())),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

/// Get network status
#[utoipa::path(
    get,
    path = "/v1/network/status",
    tag = "network",
    responses(
        (status = 200, description = "Network status retrieved successfully", body = NetworkStatusResponse),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn get_network_status(
    State(gateway_service): State<GatewayServiceImpl>,
) -> Result<Json<NetworkStatusResponse>, StatusCode> {
    match gateway_service.get_network_status(axum::extract::Request::new(())).await {
        Ok(response) => Ok(Json(response.into_inner())),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

/// Get node information
#[utoipa::path(
    get,
    path = "/v1/node/info",
    tag = "network",
    params(NodeInfoQuery),
    responses(
        (status = 200, description = "Node information retrieved successfully", body = NodeInfoResponse),
        (status = 404, description = "Node not found"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn get_node_info(
    State(gateway_service): State<GatewayServiceImpl>,
    Query(query): Query<NodeInfoQuery>,
) -> Result<Json<NodeInfoResponse>, StatusCode> {
    let request = NodeInfoRequest {
        node_id: query.node_id,
    };
    
    match gateway_service.get_node_info(axum::extract::Request::new(request)).await {
        Ok(response) => Ok(Json(response.into_inner())),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

/// Health check endpoint
#[utoipa::path(
    get,
    path = "/v1/health",
    tag = "health",
    responses(
        (status = 200, description = "Health check successful", body = HealthCheckResponse),
        (status = 503, description = "Service unavailable")
    )
)]
pub async fn check_health(
    State(gateway_service): State<GatewayServiceImpl>,
) -> Result<Json<HealthCheckResponse>, StatusCode> {
    match gateway_service.check_health(axum::extract::Request::new(())).await {
        Ok(response) => Ok(Json(response.into_inner())),
        Err(_) => Err(StatusCode::SERVICE_UNAVAILABLE),
    }
}

pub fn create_gateway_router() -> Router<GatewayServiceImpl> {
    Router::new()
        .route("/v1/contract/deposit", post(deposit))
        .route("/v1/contract/withdraw", post(withdraw))
        .route("/v1/contract/distribute-rewards", post(distribute_rewards))
        .route("/v1/contract/claim-rewards", post(claim_rewards))
        .route("/v1/query/balance", get(get_balance))
        .route("/v1/query/rewards", get(get_rewards))
        .route("/v1/query/contract-state", get(get_contract_state))
        .route("/v1/transaction/:transaction_hash", get(get_transaction))
        .route("/v1/transaction/history", get(get_transaction_history))
        .route("/v1/network/status", get(get_network_status))
        .route("/v1/node/info", get(get_node_info))
        .route("/v1/health", get(check_health))
}
