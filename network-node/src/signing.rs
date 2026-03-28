use crate::error::{NetworkError, Result};
use crate::aws_kms_signer::{AwsKmsSigner, AwsKmsConfig};
use async_trait::async_trait;
use ed25519_dalek::PublicKey;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info, instrument, warn};

/// Abstract signing interface for different key management providers
#[async_trait]
pub trait Signer: Send + Sync {
    /// Get the public key for this signer
    async fn get_public_key(&self) -> Result<PublicKey>;
    
    /// Sign a message using the private key
    async fn sign(&self, message: &[u8]) -> Result<Vec<u8>>;
    
    /// Get the signer identifier/key ID
    async fn get_key_id(&self) -> Result<String>;
    
    /// Check if the signer is available and healthy
    async fn health_check(&self) -> Result<bool>;
}

/// Configuration for different signing providers
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SignerConfig {
    /// Local file-based signer (for development/testing)
    Local {
        key_path: String,
    },
    /// AWS KMS signer
    AwsKms {
        key_id: String,
        region: String,
        profile: Option<String>,
    },
    /// Hardware Security Module (HMS) signer
    Hsm {
        slot_id: u32,
        pin: String,
    },
}

/// Public key cache entry
#[derive(Debug, Clone)]
struct CacheEntry {
    public_key: PublicKey,
    cached_at: chrono::DateTime<chrono::Utc>,
    ttl: chrono::Duration,
}

impl CacheEntry {
    fn new(public_key: PublicKey, ttl_seconds: i64) -> Self {
        Self {
            public_key,
            cached_at: chrono::Utc::now(),
            ttl: chrono::Duration::seconds(ttl_seconds),
        }
    }
    
    fn is_expired(&self) -> bool {
        chrono::Utc::now() > self.cached_at + self.ttl
    }
}

/// Public key cache to reduce KMS calls
pub struct PublicKeyCache {
    cache: Arc<RwLock<HashMap<String, CacheEntry>>>,
    default_ttl_seconds: i64,
}

impl PublicKeyCache {
    pub fn new(default_ttl_seconds: i64) -> Self {
        Self {
            cache: Arc::new(RwLock::new(HashMap::new())),
            default_ttl_seconds,
        }
    }
    
    /// Get cached public key or fetch if not available/expired
    pub async fn get_or_fetch<F, Fut>(&self, key_id: &str, fetch_fn: F) -> Result<PublicKey>
    where
        F: FnOnce() -> Fut + Send,
        Fut: std::future::Future<Output = Result<PublicKey>> + Send,
    {
        // Check cache first
        {
            let cache = self.cache.read().await;
            if let Some(entry) = cache.get(key_id) {
                if !entry.is_expired() {
                    debug!("Using cached public key for key_id: {}", key_id);
                    return Ok(entry.public_key);
                } else {
                    debug!("Cached public key expired for key_id: {}", key_id);
                }
            }
        }
        
        // Fetch fresh public key
        debug!("Fetching fresh public key for key_id: {}", key_id);
        let public_key = fetch_fn().await?;
        
        // Update cache
        {
            let mut cache = self.cache.write().await;
            cache.insert(key_id.to_string(), CacheEntry::new(public_key, self.default_ttl_seconds));
        }
        
        Ok(public_key)
    }
    
    /// Invalidate cache entry for a specific key
    pub async fn invalidate(&self, key_id: &str) {
        let mut cache = self.cache.write().await;
        cache.remove(key_id);
        debug!("Invalidated cache entry for key_id: {}", key_id);
    }
    
    /// Clear all cache entries
    pub async fn clear(&self) {
        let mut cache = self.cache.write().await;
        cache.clear();
        debug!("Cleared all public key cache entries");
    }
    
    /// Get cache statistics
    pub async fn stats(&self) -> CacheStats {
        let cache = self.cache.read().await;
        let total_entries = cache.len();
        let expired_entries = cache
            .values()
            .filter(|entry| entry.is_expired())
            .count();
        
        CacheStats {
            total_entries,
            expired_entries,
            valid_entries: total_entries - expired_entries,
        }
    }
}

/// Cache statistics
#[derive(Debug, Clone)]
pub struct CacheStats {
    pub total_entries: usize,
    pub expired_entries: usize,
    pub valid_entries: usize,
}

/// Signer factory to create different types of signers based on configuration
pub struct SignerFactory;

impl SignerFactory {
    /// Create a signer based on the provided configuration
    pub async fn create_signer(config: SignerConfig) -> Result<Arc<dyn Signer>> {
        match config {
            SignerConfig::Local { key_path } => {
                info!("Creating local file-based signer with key_path: {}", key_path);
                let signer = LocalSigner::new(&key_path).await?;
                Ok(Arc::new(signer))
            }
            SignerConfig::AwsKms { key_id, region, profile } => {
                info!("Creating AWS KMS signer with key_id: {}, region: {}", key_id, region);
                let signer = AwsKmsSigner::new(key_id, region, profile).await?;
                Ok(Arc::new(signer))
            }
            SignerConfig::Hsm { slot_id, pin } => {
                info!("Creating HSM signer with slot_id: {}", slot_id);
                let signer = HsmSigner::new(slot_id, pin).await?;
                Ok(Arc::new(signer))
            }
        }
    }
}

/// Local file-based signer implementation
pub struct LocalSigner {
    key_pair: ed25519_dalek::Keypair,
    key_id: String,
}

impl LocalSigner {
    pub async fn new(key_path: &str) -> Result<Self> {
        // In a real implementation, this would load the key from file
        // For now, we'll create a new key pair for demonstration
        let mut csprng = rand::rngs::OsRng;
        let key_pair = ed25519_dalek::Keypair::generate(&mut csprng);
        
        Ok(Self {
            key_pair,
            key_id: format!("local:{}", key_path),
        })
    }
}

#[async_trait]
impl Signer for LocalSigner {
    async fn get_public_key(&self) -> Result<PublicKey> {
        Ok(self.key_pair.public)
    }
    
    async fn sign(&self, message: &[u8]) -> Result<Vec<u8>> {
        let signature = self.key_pair.sign(message);
        Ok(signature.to_bytes().to_vec())
    }
    
    async fn get_key_id(&self) -> Result<String> {
        Ok(self.key_id.clone())
    }
    
    async fn health_check(&self) -> Result<bool> {
        Ok(true) // Local signer is always healthy
    }
}

/// Placeholder HSM signer implementation
pub struct HsmSigner {
    slot_id: u32,
    key_id: String,
}

impl HsmSigner {
    pub async fn new(slot_id: u32, pin: String) -> Result<Self> {
        Ok(Self {
            slot_id,
            key_id: format!("hsm:{}", slot_id),
        })
    }
}

#[async_trait]
impl Signer for HsmSigner {
    async fn get_public_key(&self) -> Result<PublicKey> {
        Err(NetworkError::NotImplemented("HSM public key retrieval not implemented".to_string()))
    }
    
    async fn sign(&self, _message: &[u8]) -> Result<Vec<u8>> {
        Err(NetworkError::NotImplemented("HSM signing not implemented".to_string()))
    }
    
    async fn get_key_id(&self) -> Result<String> {
        Ok(self.key_id.clone())
    }
    
    async fn health_check(&self) -> Result<bool> {
        Ok(false) // HSM not implemented
    }
}

/// Signing service that manages multiple signers and provides caching
pub struct SigningService {
    signers: Arc<RwLock<HashMap<String, Arc<dyn Signer>>>>,
    public_key_cache: PublicKeyCache,
    default_signer_key: Option<String>,
}

impl SigningService {
    pub fn new(cache_ttl_seconds: i64) -> Self {
        Self {
            signers: Arc::new(RwLock::new(HashMap::new())),
            public_key_cache: PublicKeyCache::new(cache_ttl_seconds),
            default_signer_key: None,
        }
    }
    
    /// Add a signer to the service
    pub async fn add_signer(&mut self, key_id: String, signer: Arc<dyn Signer>) -> Result<()> {
        let mut signers = self.signers.write().await;
        
        // Perform health check before adding
        if !signer.health_check().await? {
            warn!("Signer health check failed for key_id: {}", key_id);
            return Err(NetworkError::Signer(format!("Signer health check failed for key_id: {}", key_id)));
        }
        
        signers.insert(key_id.clone(), signer);
        
        // Set as default if it's the first signer
        if self.default_signer_key.is_none() {
            self.default_signer_key = Some(key_id.clone());
        }
        
        info!("Added signer with key_id: {}", key_id);
        Ok(())
    }
    
    /// Get a signer by key ID
    pub async fn get_signer(&self, key_id: &str) -> Result<Arc<dyn Signer>> {
        let signers = self.signers.read().await;
        signers
            .get(key_id)
            .cloned()
            .ok_or_else(|| NetworkError::Signer(format!("Signer not found for key_id: {}", key_id)))
    }
    
    /// Get the default signer
    pub async fn get_default_signer(&self) -> Result<Arc<dyn Signer>> {
        let default_key = self.default_signer_key
            .as_ref()
            .ok_or_else(|| NetworkError::Signer("No default signer configured".to_string()))?;
        
        self.get_signer(default_key).await
    }
    
    /// Set the default signer
    pub async fn set_default_signer(&mut self, key_id: String) -> Result<()> {
        let signers = self.signers.read().await;
        if !signers.contains_key(&key_id) {
            return Err(NetworkError::Signer(format!("Signer not found for key_id: {}", key_id)));
        }
        
        self.default_signer_key = Some(key_id);
        Ok(())
    }
    
    /// Sign a message using the default signer
    #[instrument(skip(self, message), fields(message_len = message.len()))]
    pub async fn sign(&self, message: &[u8]) -> Result<Vec<u8>> {
        let signer = self.get_default_signer().await?;
        signer.sign(message).await
    }
    
    /// Sign a message using a specific signer
    #[instrument(skip(self, message), fields(key_id, message_len = message.len()))]
    pub async fn sign_with(&self, key_id: &str, message: &[u8]) -> Result<Vec<u8>> {
        let signer = self.get_signer(key_id).await?;
        signer.sign(message).await
    }
    
    /// Get public key for a signer (with caching)
    #[instrument(skip(self), fields(key_id))]
    pub async fn get_public_key(&self, key_id: &str) -> Result<PublicKey> {
        let signer = self.get_signer(key_id).await?;
        
        self.public_key_cache
            .get_or_fetch(key_id, || signer.get_public_key())
            .await
    }
    
    /// Get public key for the default signer (with caching)
    pub async fn get_default_public_key(&self) -> Result<PublicKey> {
        let default_key = self.default_signer_key
            .as_ref()
            .ok_or_else(|| NetworkError::Signer("No default signer configured".to_string()))?;
        
        self.get_public_key(default_key).await
    }
    
    /// Invalidate cache for a specific signer
    pub async fn invalidate_cache(&self, key_id: &str) {
        self.public_key_cache.invalidate(key_id).await;
    }
    
    /// Get cache statistics
    pub async fn get_cache_stats(&self) -> CacheStats {
        self.public_key_cache.stats().await
    }
    
    /// Get all registered signer IDs
    pub async fn list_signers(&self) -> Vec<String> {
        let signers = self.signers.read().await;
        signers.keys().cloned().collect()
    }
    
    /// Perform health check on all signers
    pub async fn health_check_all(&self) -> HashMap<String, bool> {
        let signers = self.signers.read().await;
        let mut results = HashMap::new();
        
        for (key_id, signer) in signers.iter() {
            match signer.health_check().await {
                Ok(healthy) => {
                    results.insert(key_id.clone(), healthy);
                    if !healthy {
                        warn!("Signer health check failed for key_id: {}", key_id);
                    }
                }
                Err(e) => {
                    error!("Health check error for signer {}: {}", key_id, e);
                    results.insert(key_id.clone(), false);
                }
            }
        }
        
        results
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::sleep;
    
    #[tokio::test]
    async fn test_public_key_cache() {
        let cache = PublicKeyCache::new(60); // 60 seconds TTL
        
        // Test cache miss and fetch
        let key_id = "test_key";
        let public_key = cache
            .get_or_fetch(key_id, || async {
                let mut csprng = rand::rngs::OsRng;
                let key_pair = ed25519_dalek::Keypair::generate(&mut csprng);
                Ok(key_pair.public)
            })
            .await
            .unwrap();
        
        // Test cache hit
        let cached_key = cache
            .get_or_fetch(key_id, || async {
                panic!("Should not call fetch function on cache hit");
            })
            .await
            .unwrap();
        
        assert_eq!(public_key, cached_key);
    }
    
    #[tokio::test]
    async fn test_signing_service() {
        let mut service = SigningService::new(60);
        
        // Add a local signer
        let signer = LocalSigner::new("test_key.pem").await.unwrap();
        service.add_signer("test_signer".to_string(), Arc::new(signer)).await.unwrap();
        
        // Test signing
        let message = b"Hello, World!";
        let signature = service.sign_with("test_signer", message).await.unwrap();
        
        // Verify signature
        let public_key = service.get_public_key("test_signer").await.unwrap();
        let signature_obj = ed25519_dalek::Signature::from_bytes(&signature).unwrap();
        
        assert!(public_key.verify(message, &signature_obj).is_ok());
    }
}

#[cfg(test)]
mod signing_tests;
