use std::sync::Arc;

use tokio::sync::RwLock;
use tonic::{Request, Response, Status};
use tracing::{error, info};

use crate::database::ConnectionPool;
use crate::grpc::network::{
    vault_service_server::VaultService, GetTvlRequest, GetTvlResponse,
};

pub struct VaultServiceImpl {
    connection_pool: Arc<RwLock<ConnectionPool>>,
}

impl VaultServiceImpl {
    pub fn new(connection_pool: Arc<RwLock<ConnectionPool>>) -> Self {
        Self { connection_pool }
    }
}

#[tonic::async_trait]
impl VaultService for VaultServiceImpl {
    async fn get_tvl(
        &self,
        _request: Request<GetTvlRequest>,
    ) -> Result<Response<GetTvlResponse>, Status> {
        info!("Received vault TVL request");

        let pool = {
            let connection_pool = self.connection_pool.read().await;
            connection_pool.get_pool().clone()
        };

        let tvl: String = sqlx::query_scalar(
            "SELECT COALESCE(SUM(amount), 0)::TEXT FROM deposits",
        )
        .fetch_one(&pool)
        .await
        .map_err(|err| {
            error!("Failed to fetch vault TVL: {}", err);
            match err {
                sqlx::Error::RowNotFound => Status::not_found("vault deposits not found"),
                _ => Status::internal("failed to fetch vault TVL"),
            }
        })?;

        Ok(Response::new(GetTvlResponse { tvl }))
    }
}
