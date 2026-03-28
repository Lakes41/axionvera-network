#[cfg(test)]
mod signing_tests {
    use super::*;
    use crate::signing::{SignerConfig, SigningService, SignerFactory, LocalSigner};
    use ed25519_dalek::{PublicKey, Signature, Verifier};
    use tokio::time::sleep;
    use std::time::Duration;

    #[tokio::test]
    async fn test_local_signer_creation() {
        let signer = LocalSigner::new("test_key.pem").await.unwrap();
        let public_key = signer.get_public_key().await.unwrap();
        
        // Test signing
        let message = b"Hello, World!";
        let signature = signer.sign(message).await.unwrap();
        
        // Verify signature
        let signature_obj = Signature::from_bytes(&signature).unwrap();
        assert!(public_key.verify(message, &signature_obj).is_ok());
        
        // Test key ID
        let key_id = signer.get_key_id().await.unwrap();
        assert_eq!(key_id, "local:test_key.pem");
        
        // Test health check
        assert!(signer.health_check().await.unwrap());
    }

    #[tokio::test]
    async fn test_signing_service_with_local_signer() {
        let mut service = SigningService::new(60); // 60 seconds TTL
        
        // Add a local signer
        let signer = LocalSigner::new("test_key.pem").await.unwrap();
        service.add_signer("test_signer".to_string(), Arc::new(signer)).await.unwrap();
        
        // Test signing with specific signer
        let message = b"Test message for signing service";
        let signature = service.sign_with("test_signer", message).await.unwrap();
        
        // Verify signature using service
        let public_key = service.get_public_key("test_signer").await.unwrap();
        let signature_obj = Signature::from_bytes(&signature).unwrap();
        assert!(public_key.verify(message, &signature_obj).is_ok());
        
        // Test default signer functionality
        service.set_default_signer("test_signer".to_string()).await.unwrap();
        let default_signature = service.sign(message).await.unwrap();
        let default_signature_obj = Signature::from_bytes(&default_signature).unwrap();
        assert!(public_key.verify(message, &default_signature_obj).is_ok());
    }

    #[tokio::test]
    async fn test_public_key_caching() {
        let service = SigningService::new(2); // 2 seconds TTL for testing
        
        // Add a local signer
        let signer = LocalSigner::new("cache_test_key.pem").await.unwrap();
        service.add_signer("cache_test_signer".to_string(), Arc::new(signer)).await.unwrap();
        
        // First call should fetch from signer
        let public_key1 = service.get_public_key("cache_test_signer").await.unwrap();
        
        // Second call should use cache (within TTL)
        let public_key2 = service.get_public_key("cache_test_signer").await.unwrap();
        assert_eq!(public_key1, public_key2);
        
        // Check cache stats
        let stats = service.get_cache_stats().await;
        assert_eq!(stats.total_entries, 1);
        assert_eq!(stats.valid_entries, 1);
        
        // Wait for cache to expire
        sleep(Duration::from_secs(3)).await;
        
        // Next call should fetch from signer again
        let public_key3 = service.get_public_key("cache_test_signer").await.unwrap();
        assert_eq!(public_key1, public_key3);
    }

    #[tokio::test]
    async fn test_signer_factory_local_config() {
        let config = SignerConfig::Local {
            key_path: "factory_test_key.pem".to_string(),
        };
        
        let signer = SignerFactory::create_signer(config).await.unwrap();
        let public_key = signer.get_public_key().await.unwrap();
        
        // Test signing
        let message = b"Factory test message";
        let signature = signer.sign(message).await.unwrap();
        
        // Verify signature
        let signature_obj = Signature::from_bytes(&signature).unwrap();
        assert!(public_key.verify(message, &signature_obj).is_ok());
    }

    #[tokio::test]
    async fn test_multiple_signers() {
        let mut service = SigningService::new(300); // 5 minutes TTL
        
        // Create multiple signers
        let signer1 = LocalSigner::new("multi_test_key1.pem").await.unwrap();
        let signer2 = LocalSigner::new("multi_test_key2.pem").await.unwrap();
        
        // Add signers
        service.add_signer("signer1".to_string(), Arc::new(signer1)).await.unwrap();
        service.add_signer("signer2".to_string(), Arc::new(signer2)).await.unwrap();
        
        // Test that they have different keys
        let key1 = service.get_public_key("signer1").await.unwrap();
        let key2 = service.get_public_key("signer2").await.unwrap();
        assert_ne!(key1.as_bytes(), key2.as_bytes());
        
        // Test signing with different signers
        let message = b"Multi-signer test";
        let sig1 = service.sign_with("signer1", message).await.unwrap();
        let sig2 = service.sign_with("signer2", message).await.unwrap();
        
        // Signatures should be different
        assert_ne!(sig1, sig2);
        
        // Both signatures should verify with their respective keys
        let sig1_obj = Signature::from_bytes(&sig1).unwrap();
        let sig2_obj = Signature::from_bytes(&sig2).unwrap();
        assert!(key1.verify(message, &sig1_obj).is_ok());
        assert!(key2.verify(message, &sig2_obj).is_ok());
        
        // Test listing signers
        let signer_list = service.list_signers().await;
        assert_eq!(signer_list.len(), 2);
        assert!(signer_list.contains(&"signer1".to_string()));
        assert!(signer_list.contains(&"signer2".to_string()));
    }

    #[tokio::test]
    async fn test_health_checks() {
        let mut service = SigningService::new(300);
        
        // Add a healthy signer
        let healthy_signer = LocalSigner::new("health_test_key.pem").await.unwrap();
        service.add_signer("healthy".to_string(), Arc::new(healthy_signer)).await.unwrap();
        
        // Test health check
        let health_results = service.health_check_all().await;
        assert_eq!(health_results.len(), 1);
        assert_eq!(health_results.get("healthy"), Some(&true));
    }

    #[tokio::test]
    async fn test_cache_invalidation() {
        let service = SigningService::new(300);
        
        // Add a signer
        let signer = LocalSigner::new("invalidate_test_key.pem").await.unwrap();
        service.add_signer("invalidate_test".to_string(), Arc::new(signer)).await.unwrap();
        
        // Cache the public key
        let public_key1 = service.get_public_key("invalidate_test").await.unwrap();
        
        // Verify it's cached
        let stats = service.get_cache_stats().await;
        assert_eq!(stats.total_entries, 1);
        
        // Invalidate cache
        service.invalidate_cache("invalidate_test").await;
        
        // Cache should be empty now
        let stats = service.get_cache_stats().await;
        assert_eq!(stats.total_entries, 0);
        
        // Next call should fetch fresh
        let public_key2 = service.get_public_key("invalidate_test").await.unwrap();
        assert_eq!(public_key1, public_key2);
    }

    #[tokio::test]
    async fn test_error_handling() {
        let service = SigningService::new(300);
        
        // Test getting non-existent signer
        let result = service.get_signer("non_existent").await;
        assert!(result.is_err());
        
        // Test signing with non-existent signer
        let result = service.sign_with("non_existent", b"test").await;
        assert!(result.is_err());
        
        // Test getting public key for non-existent signer
        let result = service.get_public_key("non_existent").await;
        assert!(result.is_err());
        
        // Test default signer when none is set
        let result = service.sign(b"test").await;
        assert!(result.is_err());
        
        let result = service.get_default_public_key().await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_signer_config_serialization() {
        let config = SignerConfig::Local {
            key_path: "/path/to/key.pem".to_string(),
        };
        
        let serialized = serde_json::to_string(&config).unwrap();
        let deserialized: SignerConfig = serde_json::from_str(&serialized).unwrap();
        
        match deserialized {
            SignerConfig::Local { key_path } => {
                assert_eq!(key_path, "/path/to/key.pem");
            }
            _ => panic!("Expected Local signer config"),
        }
    }

    // Integration test for AWS KMS (would require actual AWS credentials)
    // This test is disabled by default and should only be run in CI/CD with proper credentials
    #[ignore]
    #[tokio::test]
    async fn test_aws_kms_signer_integration() {
        // This test requires:
        // 1. AWS credentials configured
        // 2. A KMS key with proper permissions
        // 3. The key ID should be set as an environment variable
        
        let key_id = std::env::var("TEST_KMS_KEY_ID")
            .expect("TEST_KMS_KEY_ID environment variable must be set");
        
        let region = std::env::var("TEST_AWS_REGION")
            .unwrap_or_else(|_| "us-east-1".to_string());
        
        let config = SignerConfig::AwsKms {
            key_id: key_id.clone(),
            region,
            profile: None,
        };
        
        let signer = SignerFactory::create_signer(config).await.unwrap();
        
        // Test getting key ID
        let retrieved_key_id = signer.get_key_id().await.unwrap();
        assert_eq!(retrieved_key_id, key_id);
        
        // Test health check
        assert!(signer.health_check().await.unwrap());
        
        // Test public key retrieval
        let public_key = signer.get_public_key().await.unwrap();
        assert!(!public_key.as_bytes().is_empty());
        
        // Test signing
        let message = b"AWS KMS integration test message";
        let signature = signer.sign(message).await.unwrap();
        assert!(!signature.is_empty());
        
        // Note: AWS KMS signatures use different format than ed25519-dalek
        // In a real implementation, you'd need to handle the AWS KMS signature format
    }
}
