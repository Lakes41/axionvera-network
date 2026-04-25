pub mod network {
    tonic::include_proto!("axionvera.network");
}

pub mod gateway {
    tonic::include_proto!("axionvera.gateway");
}

pub mod network_service;
pub mod gateway_service;
pub mod health_service;
pub mod p2p_service;
pub mod vault_service;

pub use network_service::NetworkServiceImpl;
pub use gateway_service::GatewayServiceImpl;
pub use health_service::HealthServiceImpl;
pub use p2p_service::P2PServiceImpl;
pub use vault_service::VaultServiceImpl;
