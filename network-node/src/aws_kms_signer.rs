use crate::error::{NetworkError, Result};
use crate::signing::Signer;
use async_trait::async_trait;
use aws_config::meta::region::RegionProviderChain;
use aws_config::{BehaviorVersion, ConfigLoader};
use aws_sdk_kms::primitives::Blob;
use aws_sdk_kms::Client as KmsClient;
use aws_smithy_runtime::client::http::hyper_014::HyperClientBuilder;
use aws_smithy_runtime_api::client::http::SharedHttpClient;
use aws_types::region::Region;
use ed25519_dalek::PublicKey;
use sha2::{Digest, Sha256};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::timeout;
use tracing::{debug, error, info, instrument, warn};

/// AWS KMS signer configuration
#[derive(Debug, Clone)]
pub struct AwsKmsConfig {
    pub key_id: String,
    pub region: String,
    pub profile: Option<String>,
    pub endpoint_url: Option<String>,
    pub timeout_ms: u64,
    pub max_retries: u32,
    pub retry_delay_ms: u64,
}

impl Default for AwsKmsConfig {
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

/// AWS KMS signer implementation
pub struct AwsKmsSigner {
    client: KmsClient,
    config: AwsKmsConfig,
    http_client: SharedHttpClient,
}

impl AwsKmsSigner {
    /// Create a new AWS KMS signer
    pub async fn new(key_id: String, region: String, profile: Option<String>) -> Result<Self> {
        let config = AwsKmsConfig {
            key_id,
            region,
            profile,
            ..Default::default()
        };
        
        Self::with_config(config).await
    }
    
    /// Create a new AWS KMS signer with custom configuration
    pub async fn with_config(config: AwsKmsConfig) -> Result<Self> {
        info!("Initializing AWS KMS signer with key_id: {}", config.key_id);
        
        // Create custom HTTP client with timeout configuration
        let http_client = HyperClientBuilder::new()
            .hyper_builder(
                hyper::Client::builder()
                    .pool_idle_timeout(Duration::from_secs(30))
                    .pool_max_idle_per_host(10)
                    .http2_keep_alive_interval(Duration::from_secs(30))
                    .http2_keep_alive_timeout(Duration::from_secs(10))
            )
            .build_https()
            .map_err(|e| NetworkError::Kms(format!("Failed to create HTTP client: {}", e)))?;
        
        let shared_http_client = SharedHttpClient::new(http_client);
        
        // Configure AWS SDK
        let region_provider = RegionProviderChain::first_try(Some(Region::new(config.region.clone())))
            .or_default_provider();
        
        let mut config_loader = aws_config::defaults(BehaviorVersion::latest())
            .region(region_provider)
            .http_client(shared_http_client.clone());
        
        // Set profile if provided
        if let Some(profile) = &config.profile {
            config_loader = config_loader.profile_name(profile);
        }
        
        // Set custom endpoint if provided (useful for testing with LocalStack)
        if let Some(endpoint_url) = &config.endpoint_url {
            config_loader = config_loader.endpoint_url(endpoint_url);
        }
        
        let sdk_config = config_loader
            .load()
            .await
            .map_err(|e| NetworkError::Kms(format!("Failed to load AWS config: {}", e)))?;
        
        // Create KMS client
        let client = aws_sdk_kms::Client::new(&sdk_config);
        
        let signer = Self {
            client,
            config,
            http_client: shared_http_client,
        };
        
        // Test the connection by performing a health check
        signer.health_check().await?;
        
        info!("AWS KMS signer initialized successfully");
        Ok(signer)
    }
    
    /// Sign a message with retry logic and error handling
    async fn sign_with_retry(&self, message: &[u8]) -> Result<Vec<u8>> {
        let mut last_error = None;
        
        for attempt in 1..=self.config.max_retries {
            match self.attempt_sign(message).await {
                Ok(signature) => {
                    if attempt > 1 {
                        info!("AWS KMS signing succeeded on attempt {}", attempt);
                    }
                    return Ok(signature);
                }
                Err(e) => {
                    last_error = Some(e.clone());
                    
                    // Check if we should retry
                    if !self.should_retry(&e) || attempt == self.config.max_retries {
                        break;
                    }
                    
                    warn!(
                        "AWS KMS signing attempt {} failed, retrying in {}ms: {}",
                        attempt, self.config.retry_delay_ms, e
                    );
                    
                    tokio::time::sleep(Duration::from_millis(self.config.retry_delay_ms)).await;
                }
            }
        }
        
        Err(last_error.unwrap_or_else(|| NetworkError::Kms("All retry attempts failed".to_string())))
    }
    
    /// Attempt to sign a message (single attempt)
    async fn attempt_sign(&self, message: &[u8]) -> Result<Vec<u8>> {
        // Hash the message first (KMS expects the hash for signing operations)
        let mut hasher = Sha256::new();
        hasher.update(message);
        let message_hash = hasher.finalize();
        
        debug!("Signing message hash: {:x}", message_hash);
        
        // Create signing request
        let sign_request = self.client
            .sign()
            .key_id(&self.config.key_id)
            .message(Blob::new(message_hash))
            .signing_algorithm(aws_sdk_kms::types::SigningAlgorithmSpec::EcdsaSha256)
            .message_type(aws_sdk_kms::types::MessageType::Digest);
        
        // Execute with timeout
        let sign_response = timeout(
            Duration::from_millis(self.config.timeout_ms),
            sign_request.send()
        )
        .await
        .map_err(|_| NetworkError::KmsTimeout(format!("Signing operation timed out after {}ms", self.config.timeout_ms)))?
        .map_err(|e| self.handle_kms_error(e))?;
        
        // Extract signature
        let signature_bytes = sign_response
            .signature()
            .ok_or_else(|| NetworkError::Kms("No signature returned from KMS".to_string()))?
            .as_ref()
            .to_vec();
        
        debug!("Successfully signed message, signature length: {} bytes", signature_bytes.len());
        Ok(signature_bytes)
    }
    
    /// Get public key with retry logic
    async fn get_public_key_with_retry(&self) -> Result<PublicKey> {
        let mut last_error = None;
        
        for attempt in 1..=self.config.max_retries {
            match self.attempt_get_public_key().await {
                Ok(public_key) => {
                    if attempt > 1 {
                        info!("AWS KMS public key retrieval succeeded on attempt {}", attempt);
                    }
                    return Ok(public_key);
                }
                Err(e) => {
                    last_error = Some(e.clone());
                    
                    // Check if we should retry
                    if !self.should_retry(&e) || attempt == self.config.max_retries {
                        break;
                    }
                    
                    warn!(
                        "AWS KMS public key retrieval attempt {} failed, retrying in {}ms: {}",
                        attempt, self.config.retry_delay_ms, e
                    );
                    
                    tokio::time::sleep(Duration::from_millis(self.config.retry_delay_ms)).await;
                }
            }
        }
        
        Err(last_error.unwrap_or_else(|| NetworkError::Kms("All retry attempts failed".to_string())))
    }
    
    /// Attempt to get public key (single attempt)
    async fn attempt_get_public_key(&self) -> Result<PublicKey> {
        debug!("Retrieving public key from AWS KMS");
        
        // Create get public key request
        let get_public_key_request = self.client
            .get_public_key()
            .key_id(&self.config.key_id);
        
        // Execute with timeout
        let response = timeout(
            Duration::from_millis(self.config.timeout_ms),
            get_public_key_request.send()
        )
        .await
        .map_err(|_| NetworkError::KmsTimeout(format!("Public key retrieval timed out after {}ms", self.config.timeout_ms)))?
        .map_err(|e| self.handle_kms_error(e))?;
        
        // Extract public key bytes
        let public_key_bytes = response
            .public_key()
            .ok_or_else(|| NetworkError::Kms("No public key returned from KMS".to_string()))?
            .as_ref()
            .to_vec();
        
        // Convert to ed25519 public key
        let public_key = PublicKey::from_bytes(&public_key_bytes)
            .map_err(|e| NetworkError::Kms(format!("Invalid public key format: {}", e)))?;
        
        debug!("Successfully retrieved public key from AWS KMS");
        Ok(public_key)
    }
    
    /// Determine if an error is retryable
    fn should_retry(&self, error: &NetworkError) -> bool {
        match error {
            NetworkError::KmsTimeout(_) => true,
            NetworkError::Kms(ref msg) if msg.contains("Throttling") => true,
            NetworkError::Kms(ref msg) if msg.contains("Rate exceeded") => true,
            NetworkError::Kms(ref msg) if msg.contains("Internal failure") => true,
            NetworkError::Kms(ref msg) if msg.contains("Service unavailable") => true,
            NetworkError::Kms(ref msg) if msg.contains("Request timeout") => true,
            _ => false,
        }
    }
    
    /// Handle AWS KMS specific errors and convert them to NetworkError
    fn handle_kms_error(&self, error: aws_sdk_kms::Error) -> NetworkError {
        match error {
            aws_sdk_kms::Error::NotFoundException(msg) => {
                NetworkError::Kms(format!("KMS key not found: {}", msg))
            }
            aws_sdk_kms::Error::DisabledException(msg) => {
                NetworkError::Kms(format!("KMS key is disabled: {}", msg))
            }
            aws_sdk_kms::Error::KeyUnavailableException(msg) => {
                NetworkError::Kms(format!("KMS key is unavailable: {}", msg))
            }
            aws_sdk_kms::Error::InvalidKeyUsageException(msg) => {
                NetworkError::Kms(format!("Invalid key usage for signing: {}", msg))
            }
            aws_sdk_kms::Error::InvalidGrantTokenException(msg) => {
                NetworkError::Kms(format!("Invalid grant token: {}", msg))
            }
            aws_sdk_kms::Error::KmsInternalException(msg) => {
                NetworkError::Kms(format!("KMS internal error: {}", msg))
            }
            aws_sdk_kms::Error::KmsInvalidStateException(msg) => {
                NetworkError::Kms(format!("KMS invalid state: {}", msg))
            }
            aws_sdk_kms::Error::DependencyTimeoutException(msg) => {
                NetworkError::KmsTimeout(format!("KMS dependency timeout: {}", msg))
            }
            aws_sdk_kms::Error::LimitExceededException(msg) => {
                NetworkError::KmsRateLimit(format!("KMS rate limit exceeded: {}", msg))
            }
            _ => NetworkError::Kms(format!("KMS error: {}", error)),
        }
    }
}

#[async_trait]
impl Signer for AwsKmsSigner {
    async fn get_public_key(&self) -> Result<PublicKey> {
        self.get_public_key_with_retry().await
    }
    
    async fn sign(&self, message: &[u8]) -> Result<Vec<u8>> {
        self.sign_with_retry(message).await
    }
    
    async fn get_key_id(&self) -> Result<String> {
        Ok(self.config.key_id.clone())
    }
    
    async fn health_check(&self) -> Result<bool> {
        debug!("Performing health check for AWS KMS signer");
        
        match self.attempt_get_public_key().await {
            Ok(_) => {
                debug!("AWS KMS signer health check passed");
                Ok(true)
            }
            Err(e) => {
                warn!("AWS KMS signer health check failed: {}", e);
                Ok(false)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_aws_kms_config_default() {
        let config = AwsKmsConfig::default();
        assert_eq!(config.region, "us-east-1");
        assert_eq!(config.timeout_ms, 30000);
        assert_eq!(config.max_retries, 3);
        assert_eq!(config.retry_delay_ms, 1000);
    }
    
    #[tokio::test]
    async fn test_should_retry() {
        let config = AwsKmsConfig::default();
        let signer = AwsKmsSigner {
            client: aws_sdk_kms::Client::from_conf(
                aws_sdk_kms::Config::builder()
                    .region(Region::new("us-east-1"))
                    .build()
            ),
            config,
            http_client: SharedHttpClient::new(
                HyperClientBuilder::new().build_https().unwrap()
            ),
        };
        
        // Test retryable errors
        assert!(signer.should_retry(&NetworkError::KmsTimeout("test".to_string())));
        assert!(signer.should_retry(&NetworkError::Kms("Throttling exception".to_string())));
        assert!(signer.should_retry(&NetworkError::Kms("Rate exceeded".to_string())));
        assert!(signer.should_retry(&NetworkError::Kms("Internal failure".to_string())));
        
        // Test non-retryable errors
        assert!(!signer.should_retry(&NetworkError::Kms("Key not found".to_string())));
        assert!(!signer.should_retry(&NetworkError::Crypto("Invalid signature".to_string())));
    }
}
