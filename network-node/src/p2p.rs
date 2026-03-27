use crate::error::Result;
use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

pub type NodeId = [u8; 32];

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PeerInfo {
    pub id: NodeId,
    pub address: SocketAddr,
    pub last_seen: chrono::DateTime<chrono::Utc>,
}

pub struct KademliaRoutingTable {
    local_id: NodeId,
    buckets: Vec<BTreeMap<NodeId, PeerInfo>>,
}

impl KademliaRoutingTable {
    pub fn new(local_id: NodeId) -> Self {
        let mut buckets = Vec::with_capacity(256);
        for _ in 0..256 {
            buckets.push(BTreeMap::new());
        }
        Self { local_id, buckets }
    }

    /// Calculate XOR distance between two IDs
    pub fn xor_distance(id1: &NodeId, id2: &NodeId) -> [u8; 32] {
        let mut distance = [0u8; 32];
        for i in 0..32 {
            distance[i] = id1[i] ^ id2[i];
        }
        distance
    }

    /// Find the bucket index for a given ID
    fn bucket_index(&self, id: &NodeId) -> usize {
        let distance = Self::xor_distance(&self.local_id, id);
        for i in 0..32 {
            if distance[i] != 0 {
                return (i * 8) + (7 - (distance[i] as f32).log2() as usize);
            }
        }
        255
    }

    /// Add or update a peer in the routing table
    pub fn update(&mut self, peer: PeerInfo) {
        let index = self.bucket_index(&peer.id);
        let bucket = &mut self.buckets[index];

        if bucket.len() < 20 {
            // K-bucket size
            bucket.insert(peer.id, peer);
        } else {
            // In a real implementation: ping the oldest peer and replace if dead
            debug!("Bucket {} is full, ignoring new peer", index);
        }
    }

    /// Find the K closest nodes to a target ID
    pub fn find_closest(&self, target: &NodeId, k: usize) -> Vec<PeerInfo> {
        let mut all_peers = Vec::new();
        for bucket in &self.buckets {
            for peer in bucket.values() {
                all_peers.push(peer.clone());
            }
        }

        all_peers.sort_by(|a, b| {
            let dist_a = Self::xor_distance(&a.id, target);
            let dist_b = Self::xor_distance(&b.id, target);
            dist_a.cmp(&dist_b)
        });

        all_peers.into_iter().take(k).collect()
    }
}

pub struct P2PManager {
    routing_table: Arc<RwLock<KademliaRoutingTable>>,
    local_id: NodeId,
}

impl P2PManager {
    pub fn new(local_id: NodeId) -> Self {
        Self {
            routing_table: Arc::new(RwLock::new(KademliaRoutingTable::new(local_id))),
            local_id,
        }
    }

    /// Background worker to maintain routing table
    pub async fn start_maintenance(&self) {
        let routing_table = self.routing_table.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(30));
            loop {
                interval.tick().await;
                info!("Performing Kademlia maintenance: Pinging peers...");
                // In real implementation:
                // 1. Get all nodes
                // 2. Ping them
                // 3. Remove inactive ones
            }
        });
    }

    /// Bootstrap the node from a single seed identity
    pub async fn bootstrap(&self, seed_address: SocketAddr) -> Result<()> {
        info!("Bootstrapping from seed peer: {}", seed_address);
        // 1. Update routing table with seed
        // 2. Perform FIND_NODE for our own ID to find neighbors
        Ok(())
    }

    /// Handle PING RPC
    pub fn handle_ping(&self, from: PeerInfo) -> Result<()> {
        debug!("Received PING from {:?}", from.address);
        // Update routing table
        Ok(())
    }
}
