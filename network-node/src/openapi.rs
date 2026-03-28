use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;
use axum::Router;

use crate::grpc::gateway::{
    DepositRequest, WithdrawRequest, DistributeRewardsRequest, ClaimRewardsRequest,
    TransactionResponse, BalanceRequest, BalanceResponse, RewardsRequest, RewardsResponse,
    ContractStateRequest, ContractStateResponse, NetworkStatusResponse, NodeInfoRequest,
    NodeInfoResponse, TransactionRequest, TransactionHistoryRequest, TransactionHistoryResponse,
    HealthCheckResponse,
};

#[derive(OpenApi)]
#[openapi(
    paths(
        crate::gateway::deposit,
        crate::gateway::withdraw,
        crate::gateway::distribute_rewards,
        crate::gateway::claim_rewards,
        crate::gateway::get_balance,
        crate::gateway::get_rewards,
        crate::gateway::get_contract_state,
        crate::gateway::get_transaction,
        crate::gateway::get_transaction_history,
        crate::gateway::get_network_status,
        crate::gateway::get_node_info,
        crate::gateway::check_health,
    ),
    components(
        schemas(
            DepositRequest,
            WithdrawRequest,
            DistributeRewardsRequest,
            ClaimRewardsRequest,
            TransactionResponse,
            BalanceRequest,
            BalanceResponse,
            RewardsRequest,
            RewardsResponse,
            ContractStateRequest,
            ContractStateResponse,
            NetworkStatusResponse,
            NodeInfoRequest,
            NodeInfoResponse,
            TransactionRequest,
            TransactionHistoryRequest,
            TransactionHistoryResponse,
            HealthCheckResponse,
        )
    ),
    tags(
        (name = "contract", description = "Contract interaction operations"),
        (name = "query", description = "Query operations"),
        (name = "network", description = "Network status operations"),
        (name = "health", description = "Health check operations"),
    ),
    info(
        title = "Axionvera Network API",
        description = "gRPC/JSON-RPC Bridge for Axionvera Network Contract Interaction",
        version = "1.0.0",
        contact(
            name = "Axionvera Team",
            email = "team@axionvera.com"
        ),
        license(
            name = "Apache 2.0",
            url = "https://www.apache.org/licenses/LICENSE-2.0"
        )
    ),
    servers(
        (url = "https://api.axionvera.com", description = "Production server"),
        (url = "https://testnet-api.axionvera.com", description = "Testnet server"),
        (url = "http://localhost:8081", description = "Development server"),
    ),
    external_docs(
        description = "Axionvera Network Documentation",
        url = "https://docs.axionvera.com"
    )
)]
pub struct ApiDoc;

pub fn create_swagger_ui() -> Router {
    SwaggerUi::new("/swagger-ui")
        .url("/api-docs/openapi.json", ApiDoc::openapi())
        .into()
}
