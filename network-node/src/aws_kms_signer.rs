use crate::error::{NetworkError, Result};
use crate::signing::Signer;
use async_trait::async_trait;
use aws_config::meta::region::RegionProviderChain;
use aws_config::BehaviorVersion;
use aws_sdk_kms::primitives::Blob;
use aws_sdk_kms::Client as KmsClient;
use aws_types::region::Region;
use ed25519_dalek::PublicKey;
use std::time::Duration;
use tokio::time::timeout;
use tracing::{debug, info, instrument, warn};

/// AWS KMS signer configuration
#[derive(Debug, Clone)]
pub struct KmsConfig {
    pub key_id: String,
    pub region: String,
    pub profile: Option<String>,
    pub endpoint_url: Option<String>,
    pub timeout_ms: u64,
    pub max_retries: u32,
    pub retry_delay_ms: u64,
}

impl Default for KmsConfig {
    fn default() -> Self {
        Self {
            key_id: String::new(),
            region: "us-east-1".to_string(),
            profile: None,
            endpoint_url: None,
            timeout_ms: 30000, // 30 seconds default timeout
            max_retries: 3,
            retry_delay_ms: 1000, // 1 second default retry delay
        }
    }
}

/// AWS KMS signer implementation for Stellar (Ed25519)
pub struct KmsSigner {
    client: KmsClient,
    config: KmsConfig,
}

impl KmsSigner {
    /// Create a new AWS KMS signer
    pub async fn new(key_id: String, region: String, profile: Option<String>) -> Result<Self> {
        let config = KmsConfig {
            key_id,
            region,
            profile,
            ..Default::default()
        };
        
        Self::with_config(config).await
    }
    
    /// Create a new AWS KMS signer with custom configuration
    pub async fn with_config(config: KmsConfig) -> Result<Self> {
        info!("Initializing AWS KMS signer with key_id: {}", config.key_id);
        
        // Configure AWS SDK
        let region_provider = RegionProviderChain::first_try(Some(Region::new(config.region.clone())))
            .or_default_provider();
        
        let mut config_loader = aws_config::defaults(BehaviorVersion::latest())
            .region(region_provider);
        
        if let Some(profile) = &config.profile {
            config_loader = config_loader.profile_name(profile);
        }
        
        if let Some(endpoint_url) = &config.endpoint_url {
            config_loader = config_loader.endpoint_url(endpoint_url);
        }
        
        let sdk_config = config_loader
            .load()
            .await;
        
        let client = KmsClient::new(&sdk_config);
        
        let signer = Self {
            client,
            config,
        };
        
        // Test connection
        signer.health_check().await?;
        
        info!("AWS KMS signer initialized successfully");
        Ok(signer)
    }

    /// Sign a message hash (32 bytes)
    async fn attempt_sign(&self, message_hash: &[u8]) -> Result<Vec<u8>> {
        debug!("Signing message hash with AWS KMS");
        
        let sign_request = self.client
            .sign()
            .key_id(&self.config.key_id)
            .message(Blob::new(message_hash))
            .signing_algorithm(aws_sdk_kms::types::SigningAlgorithmSpec::Ed25519)
            .message_type(aws_sdk_kms::types::MessageType::Raw);
        
        let sign_response = timeout(
            Duration::from_millis(self.config.timeout_ms),
            sign_request.send()
        )
        .await
        .map_err(|_| NetworkError::KmsTimeout(format!("Signing operation timed out after {}ms", self.config.timeout_ms)))?
        .map_err(|e| self.handle_kms_error(e.into()))?;
        
        let signature_bytes = sign_response
            .signature()
            .ok_or_else(|| NetworkError::Kms("No signature returned from KMS".to_string()))?
            .as_ref()
            .to_vec();
        
        debug!("Successfully signed message, signature length: {} bytes", signature_bytes.len());
        Ok(signature_bytes)
    }

    /// Retrieve and parse the Ed25519 public key from KMS
    async fn attempt_get_public_key(&self) -> Result<PublicKey> {
        debug!("Retrieving public key from AWS KMS");
        
        let response = timeout(
            Duration::from_millis(self.config.timeout_ms),
            self.client.get_public_key().key_id(&self.config.key_id).send()
        )
        .await
        .map_err(|_| NetworkError::KmsTimeout(format!("Public key retrieval timed out after {}ms", self.config.timeout_ms)))?
        .map_err(|e| self.handle_kms_error(e.into()))?;
        
        let spki_bytes = response
            .public_key()
            .ok_or_else(|| NetworkError::Kms("No public key returned from KMS".to_string()))?
            .as_ref();
        
        // For Ed25519, the SPKI is 44 bytes. The last 32 bytes are the raw public key.
        if spki_bytes.len() != 44 {
             return Err(NetworkError::Kms(format!("Unexpected SPKI length for Ed25519: {}. Expected 44 bytes.", spki_bytes.len())));
        }

        let public_key_bytes = &spki_bytes[12..];
        let public_key = PublicKey::from_bytes(public_key_bytes)
            .map_err(|e| NetworkError::Kms(format!("Invalid public key format: {}", e)))?;
        
        Ok(public_key)
    }

    /// Create a new AWS KMS signer with a specific client (useful for testing)
    pub fn with_client(client: KmsClient, config: KmsConfig) -> Self {
        Self { client, config }
    }

    /// Handle KMS errors
    fn handle_kms_error(&self, error: aws_sdk_kms::Error) -> NetworkError {
        match error {
            aws_sdk_kms::Error::NotFoundException(e) => NetworkError::Kms(format!("Key not found: {}", e)),
            aws_sdk_kms::Error::LimitExceededException(e) => NetworkError::KmsRateLimit(e.to_string()),
            aws_sdk_kms::Error::DependencyTimeoutException(e) => NetworkError::KmsTimeout(e.to_string()),
            _ => NetworkError::Kms(format!("KMS error: {}", error)),
        }
    }
}

#[async_trait]
impl Signer for KmsSigner {
    async fn get_public_key(&self) -> Result<PublicKey> {
        self.attempt_get_public_key().await
    }
    
    async fn sign(&self, message: &[u8]) -> Result<Vec<u8>> {
        // Stellar signs the transaction hash.
        // If the message is already a hash, we use it directly.
        // For simplicity, we assume the input is the hash to be signed.
        self.attempt_sign(message).await
    }
    
    async fn get_key_id(&self) -> Result<String> {
        Ok(self.config.key_id.clone())
    }
    
    async fn health_check(&self) -> Result<bool> {
        match self.attempt_get_public_key().await {
            Ok(_) => Ok(true),
            Err(_) => Ok(false),
        }
    }
}
