use std::time::SystemTime;
use tonic::{Request, Response, Status};
use tracing::{info, warn, error};
use fastrand;

use crate::grpc::network::{
    p2p_service_server::P2PService,
    PeerConnectionRequest, PeerConnectionResponse, PeerDisconnectionRequest, PeerDisconnectionResponse,
    PeerListResponse, PeerInfo, BroadcastRequest, BroadcastResponse, SyncRequest, SyncResponse,
    BlockData, MessageType, TransactionInfo, TransactionType, TransactionStatus,
};
use crate::p2p::P2PManager;

pub struct P2PServiceImpl {
    p2p_manager: std::sync::Arc<P2PManager>,
}

impl P2PServiceImpl {
    pub fn new(p2p_manager: std::sync::Arc<P2PManager>) -> Self {
        Self { p2p_manager }
    }
}

#[tonic::async_trait]
impl P2PService for P2PServiceImpl {
    async fn connect_to_peer(&self, request: Request<PeerConnectionRequest>) -> Result<Response<PeerConnectionResponse>, Status> {
        let req = request.into_inner();
        info!("Received peer connection request to: {}", req.peer_address);

        // Parse peer address
        let addr = req.peer_address.parse()
            .map_err(|e| Status::invalid_argument(format!("Invalid peer address: {}", e)))?;

        // Connect to peer
        match self.p2p_manager.connect_to_peer(addr, req.peer_id).await {
            Ok(session_id) => {
                info!("Successfully connected to peer: {}", req.peer_id);
                let response = PeerConnectionResponse {
                    success: true,
                    peer_id: req.peer_id,
                    session_id,
                    error_message: String::new(),
                };
                Ok(Response::new(response))
            }
            Err(e) => {
                error!("Failed to connect to peer {}: {}", req.peer_id, e);
                let response = PeerConnectionResponse {
                    success: false,
                    peer_id: req.peer_id,
                    session_id: String::new(),
                    error_message: format!("Connection failed: {}", e),
                };
                Ok(Response::new(response))
            }
        }
    }

    async fn disconnect_from_peer(&self, request: Request<PeerDisconnectionRequest>) -> Result<Response<PeerDisconnectionResponse>, Status> {
        let req = request.into_inner();
        info!("Received peer disconnection request for: {}", req.peer_id);

        match self.p2p_manager.disconnect_from_peer(&req.peer_id).await {
            Ok(_) => {
                info!("Successfully disconnected from peer: {}", req.peer_id);
                let response = PeerDisconnectionResponse {
                    success: true,
                    error_message: String::new(),
                };
                Ok(Response::new(response))
            }
            Err(e) => {
                error!("Failed to disconnect from peer {}: {}", req.peer_id, e);
                let response = PeerDisconnectionResponse {
                    success: false,
                    error_message: format!("Disconnection failed: {}", e),
                };
                Ok(Response::new(response))
            }
        }
    }

    async fn get_peer_list(&self, _request: Request<()>) -> Result<Response<PeerListResponse>, Status> {
        info!("Received peer list request");

        let peers = self.p2p_manager.get_peer_list().await;
        let mut peer_infos = Vec::new();

        for peer in peers {
            let timestamp = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .map_err(|e| Status::internal(format!("Timestamp error: {}", e)))?;

            let mut metadata = std::collections::HashMap::new();
            metadata.insert("version".to_string(), peer.version.clone());
            metadata.insert("region".to_string(), "us-east-1".to_string());

            let peer_info = PeerInfo {
                peer_id: peer.id,
                address: peer.address.to_string(),
                is_connected: peer.is_connected,
                last_seen: Some(prost_types::Timestamp {
                    seconds: timestamp.as_secs() as i64,
                    nanos: timestamp.subsec_nanos() as i32,
                }),
                latency_ms: peer.latency_ms,
                version: peer.version,
                metadata,
            };
            peer_infos.push(peer_info);
        }

        let response = PeerListResponse {
            peers: peer_infos,
        };

        Ok(Response::new(response))
    }

    async fn broadcast_message(&self, request: Request<BroadcastRequest>) -> Result<Response<BroadcastResponse>, Status> {
        let req = request.into_inner();
        info!("Received broadcast request of type: {:?}", MessageType::try_from(req.message_type));

        // Broadcast message to peers
        match self.p2p_manager.broadcast_message(
            req.message_type,
            &req.payload,
            &req.target_peers,
            req.ttl,
        ).await {
            Ok((recipients_count, failed_peers)) => {
                info!("Message broadcasted to {} recipients", recipients_count);
                let response = BroadcastResponse {
                    success: true,
                    recipients_count,
                    failed_peers,
                    message_id: format!("msg_{}", fastrand::u64(..)),
                };
                Ok(Response::new(response))
            }
            Err(e) => {
                error!("Failed to broadcast message: {}", e);
                let response = BroadcastResponse {
                    success: false,
                    recipients_count: 0,
                    failed_peers: vec![],
                    message_id: String::new(),
                };
                Ok(Response::new(response))
            }
        }
    }

    async fn sync_chain(&self, request: Request<SyncRequest>) -> Result<Response<SyncResponse>, Status> {
        let req = request.into_inner();
        info!("Received chain sync request from block {} to {}", req.start_block, req.end_block);

        // TODO: Implement actual chain synchronization
        let timestamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map_err(|e| Status::internal(format!("Timestamp error: {}", e)))?;

        // Mock block data
        let blocks = vec![
            BlockData {
                block_number: req.start_block,
                block_hash: format!("0x{:064x}", fastrand::u64(..)),
                timestamp: Some(prost_types::Timestamp {
                    seconds: timestamp.as_secs() as i64,
                    nanos: timestamp.subsec_nanos() as i32,
                }),
                transactions: vec![
                    TransactionInfo {
                        transaction_hash: format!("0x{:064x}", fastrand::u64(..)),
                        transaction_type: TransactionType::Deposit as i32,
                        user_address: "0x1234567890123456789012345678901234567890".to_string(),
                        amount: "1000000".to_string(),
                        token_address: "0xtokenaddress".to_string(),
                        status: TransactionStatus::Confirmed as i32,
                        timestamp: Some(prost_types::Timestamp {
                            seconds: timestamp.as_secs() as i64,
                            nanos: timestamp.subsec_nanos() as i32,
                        }),
                        block_number: req.start_block,
                        gas_used: 21000,
                    },
                ],
                state_root: vec![0u8; 32],
            },
        ];

        let response = SyncResponse {
            success: true,
            blocks,
            total_blocks: (req.end_block - req.start_block + 1),
            sync_id: format!("sync_{}", fastrand::u64(..)),
        };

        Ok(Response::new(response))
    }
}
