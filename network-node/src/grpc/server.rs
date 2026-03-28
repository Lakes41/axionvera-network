use std::sync::Arc;
use std::net::SocketAddr;
use tokio::sync::RwLock;
use tonic::transport::{Server, Certificate, Identity, ServerTlsConfig};
use tonic_web::GrpcWebLayer;
use tower::ServiceBuilder;
use tracing::{info, error, warn};

use crate::config::NetworkConfig;
use crate::database::ConnectionPool;
use crate::error::NetworkError;
use crate::signing::SigningService;
use crate::grpc::{
    NetworkServiceImpl, GatewayServiceImpl, HealthServiceImpl, P2PServiceImpl,
    network::network_service_server::NetworkServiceServer,
    network::health_service_server::HealthServiceServer,
    network::p2p_service_server::P2PServiceServer,
    gateway::gateway_service_server::GatewayServiceServer,
};
use crate::state_trie::StateTrie;
use crate::p2p::P2PManager;

pub struct GrpcServer {
    config: NetworkConfig,
    connection_pool: Arc<RwLock<ConnectionPool>>,
    state_trie: Arc<RwLock<StateTrie>>,
    p2p_manager: Arc<P2PManager>,
    signing_service: Arc<SigningService>,
}

impl GrpcServer {
    pub fn new(
        config: NetworkConfig,
        connection_pool: Arc<RwLock<ConnectionPool>>,
        state_trie: Arc<RwLock<StateTrie>>,
        p2p_manager: Arc<P2PManager>,
        signing_service: Arc<SigningService>,
    ) -> Self {
        Self {
            config,
            connection_pool,
            state_trie,
            p2p_manager,
            signing_service,
        }
    }
    
    /// Get a reference to the signing service
    pub fn signing_service(&self) -> &Arc<SigningService> {
        &self.signing_service
    }

    pub async fn start(&self) -> Result<(), NetworkError> {
        let addr: SocketAddr = self.config.grpc_bind_address
            .parse()
            .map_err(|e| NetworkError::Config(format!("Invalid gRPC bind address: {}", e)))?;

        info!("Starting gRPC server on {}", addr);

        // Create service implementations
        let network_service = NetworkServiceImpl::new(
            self.connection_pool.clone(),
            self.state_trie.clone(),
            self.p2p_manager.clone(),
        );

        let gateway_service = GatewayServiceImpl::new(
            self.connection_pool.clone(),
            self.state_trie.clone(),
            self.p2p_manager.clone(),
        );

        let health_service = HealthServiceImpl::new(self.connection_pool.clone());
        let p2p_service = P2PServiceImpl::new(self.p2p_manager.clone());

        // Build the gRPC server with middleware
        let mut server = Server::builder()
            .add_service(
                NetworkServiceServer::new(network_service)
                    .max_decoding_message_size(4 * 1024 * 1024) // 4MB max message size
            )
            .add_service(
                GatewayServiceServer::new(gateway_service)
                    .max_decoding_message_size(4 * 1024 * 1024)
            )
            .add_service(
                HealthServiceServer::new(health_service)
                    .max_decoding_message_size(1024 * 1024) // 1MB for health checks
            )
            .add_service(
                P2PServiceServer::new(p2p_service)
                    .max_decoding_message_size(8 * 1024 * 1024) // 8MB for P2P messages
            );

        // Add gRPC-Web support for browser clients
        server = server.add_service(
            GatewayServiceServer::new(GatewayServiceImpl::new(
                self.connection_pool.clone(),
                self.state_trie.clone(),
                self.p2p_manager.clone(),
            ))
            .accept_compressed(tonic::codec::CompressionEncoding::Gzip)
            .send_compressed(tonic::codec::CompressionEncoding::Gzip)
        );

        // Configure TLS if certificates are provided
        if let (Some(cert_path), Some(key_path)) = (&self.config.tls_cert_path, &self.config.tls_key_path) {
            info!("Configuring TLS for gRPC server");
            
            let cert = std::fs::read_to_string(cert_path)
                .map_err(|e| NetworkError::Config(format!("Failed to read TLS certificate: {}", e)))?;
            let key = std::fs::read_to_string(key_path)
                .map_err(|e| NetworkError::Config(format!("Failed to read TLS private key: {}", e)))?;

            let identity = Identity::from_pem(cert, key);
            let tls_config = ServerTlsConfig::new()
                .identity(identity);

            server = server.tls_config(tls_config)
                .map_err(|e| NetworkError::Config(format!("Failed to configure TLS: {}", e)))?;
        }

        // Add reflection service for development
        #[cfg(debug_assertions)]
        {
            use tonic_reflection::server::{ServerReflection, ServerReflectionServer};
            let reflection_service = ServerReflectionServer::new(ServerReflection::new());
            server = server.add_service(reflection_service);
            info!("gRPC reflection service enabled");
        }

        // Apply interceptors for logging and metrics
        server = server.intercept_fn(|req| {
            info!("gRPC request: method={:?}, metadata={:?}", req.method(), req.metadata());
            Ok(req)
        });

        // Start the server
        let server_future = server.serve_with_shutdown(addr, async {
            tokio::signal::ctrl_c()
                .await
                .expect("Failed to install CTRL+C signal handler");
            info!("Received shutdown signal, stopping gRPC server");
        });

        info!("gRPC server started successfully on {}", addr);

        match server_future.await {
            Ok(_) => {
                info!("gRPC server stopped gracefully");
                Ok(())
            }
            Err(e) => {
                error!("gRPC server error: {}", e);
                Err(NetworkError::Server(format!("gRPC server failed: {}", e)))
            }
        }
    }

    pub async fn start_with_health_check(&self) -> Result<(), NetworkError> {
        // Start health check service in a separate task
        let health_service = HealthServiceImpl::new(self.connection_pool.clone());
        let health_addr: SocketAddr = "0.0.0.0:50051"
            .parse()
            .map_err(|e| NetworkError::Config(format!("Invalid health check address: {}", e)))?;

        tokio::spawn(async move {
            info!("Starting gRPC health check service on {}", health_addr);
            
            if let Err(e) = Server::builder()
                .add_service(HealthServiceServer::new(health_service))
                .serve(health_addr)
                .await
            {
                error!("Health check service error: {}", e);
            }
        });

        // Start main gRPC server
        self.start().await
    }
}

// gRPC Gateway for HTTP/JSON-RPC interface
pub struct GrpcGateway {
    config: NetworkConfig,
    grpc_address: String,
}

impl GrpcGateway {
    pub fn new(config: NetworkConfig, grpc_address: String) -> Self {
        Self {
            config,
            grpc_address,
        }
    }

    pub async fn start(&self) -> Result<(), NetworkError> {
        info!("Starting gRPC Gateway for HTTP/JSON-RPC interface");

        // TODO: Implement grpc-gateway HTTP reverse proxy
        // This would typically use grpc-gateway or a custom HTTP-to-gRPC proxy
        
        warn!("gRPC Gateway HTTP interface not yet implemented");
        info!("Use the gRPC endpoint directly: {}", self.grpc_address);

        Ok(())
    }
}
