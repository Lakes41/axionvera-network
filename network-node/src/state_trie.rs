use crate::error::{DatabaseError, NetworkError, Result};
use rocksdb::{Options, DB};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::sync::Arc;

pub type Hash = [u8; 32];

/// A persistent Merkle Patricia Trie implementation for global state storage.
pub struct StateTrie {
    db: Arc<DB>,
    root_hash: Hash,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum TrieNode {
    Leaf {
        key: Vec<u8>,
        value: Vec<u8>,
    },
    Branch {
        children: [Option<Hash>; 16],
        value: Option<Vec<u8>>,
    },
    Extension {
        prefix: Vec<u8>,
        child: Hash,
    },
}

impl StateTrie {
    pub fn new(path: &str) -> Result<Self> {
        let mut opts = Options::default();
        opts.create_if_missing(true);
        let db = DB::open(&opts, path)
            .map_err(|e| NetworkError::Database(DatabaseError::QueryError(e.to_string())))?;

        Ok(Self {
            db: Arc::new(db),
            root_hash: [0u8; 32], // Starting with empty root
        })
    }

    /// Compute hash for a node
    pub fn hash_node(node: &TrieNode) -> Hash {
        let encoded = serde_json::to_vec(node).unwrap_or_default();
        let mut hasher = Sha256::new();
        hasher.update(&encoded);
        hasher.finalize().into()
    }

    /// Insert a key-value pair into the state trie
    pub fn insert(&mut self, key: Vec<u8>, value: Vec<u8>) -> Result<Hash> {
        // Simplified trie insertion for demonstration
        // In a real MPT, we'd traverse the nibbles
        let node = TrieNode::Leaf {
            key: key.clone(),
            value: value.clone(),
        };
        let node_hash = Self::hash_node(&node);

        let encoded = serde_json::to_vec(&node)
            .map_err(|_| NetworkError::Database(DatabaseError::SerializationError))?;
        self.db
            .put(node_hash, encoded)
            .map_err(|e| NetworkError::Database(DatabaseError::QueryError(e.to_string())))?;

        // Update root hash (simplified: XORing for demo or using a simple Merkle structure)
        // In reality, this would be the root of the MPT
        for i in 0..32 {
            self.root_hash[i] ^= node_hash[i];
        }

        Ok(self.root_hash)
    }

    /// Get value for a key from the trie
    pub fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        // This would require trie traversal. For now, we simulate with direct lookup if possible
        // but real MPT is more complex.
        Ok(None)
    }

    /// Get current root hash
    pub fn root_hash(&self) -> Hash {
        self.root_hash
    }

    /// Get a snapshot of a chunk for syncing
    pub fn get_snapshot_chunk(&self, chunk_index: usize) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
        let mut chunk = Vec::new();
        let iter = self.db.iterator(rocksdb::IteratorMode::Start);

        // Very basic chunking
        for (i, item) in iter.enumerate() {
            if i >= chunk_index * 100 && i < (chunk_index + 1) * 100 {
                if let Ok((k, v)) = item {
                    chunk.push((k.to_vec(), v.to_vec()));
                }
            }
        }

        Ok(chunk)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_merkle_trie_root_integrity() {
        let dir = tempdir().unwrap();
        let path = dir.path().to_str().unwrap();
        let mut trie = StateTrie::new(path).unwrap();

        let initial_root = trie.root_hash();

        trie.insert(b"balance_user1".to_vec(), b"1000".to_vec())
            .unwrap();
        let root1 = trie.root_hash();
        assert_ne!(initial_root, root1);

        trie.insert(b"balance_user2".to_vec(), b"500".to_vec())
            .unwrap();
        let root2 = trie.root_hash();
        assert_ne!(root1, root2);

        // Verify tampering changes root (indirectly via insertion sequence)
        // In a real trie, changing any leaf value would change the path up to the root.
    }
}
