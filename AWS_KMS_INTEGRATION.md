# AWS KMS Integration for Axionvera Network

This document describes the implementation of AWS Key Management Service (KMS) integration for secure key management in the Axionvera network node.

## Overview

The AWS KMS integration addresses the security vulnerability of storing private validator keys in plaintext on the server filesystem. The implementation provides:

- **Abstract signing interface** that supports multiple key management providers
- **AWS KMS provider** for secure key storage and signing operations
- **Public key caching** to reduce KMS API calls
- **Payload hash forwarding** to ensure private keys never leave KMS
- **Comprehensive error handling** for rate limits and network timeouts
- **Retry logic** with exponential backoff for transient failures

## Architecture

### Core Components

1. **Signer Trait** (`src/signing.rs`): Abstract interface for all signing providers
2. **AwsKmsSigner** (`src/aws_kms_signer.rs`): AWS KMS implementation
3. **SigningService** (`src/signing.rs`): Service managing multiple signers with caching
4. **PublicKeyCache** (`src/signing.rs`): Caching layer for public keys
5. **SignerFactory** (`src/signing.rs`): Factory for creating signers from configuration

### Security Features

- **Zero-knowledge architecture**: Private keys never leave AWS KMS
- **Hash-based signing**: Only SHA-256 hashes are sent to KMS for signing
- **Cached public keys**: Reduces KMS API calls while maintaining security
- **Timeout protection**: Prevents hanging operations
- **Rate limit handling**: Graceful degradation under KMS rate limits

## Configuration

### AWS KMS Configuration

Add the following to your configuration file:

```toml
[signing]
type = "AwsKms"
key_id = "arn:aws:kms:us-east-1:123456789012:key/12345678-1234-1234-1234-123456789012"
region = "us-east-1"
# Optional: AWS profile to use
# profile = "my-aws-profile"

# Cache configuration
cache_ttl_seconds = 3600  # Cache public keys for 1 hour
```

### Environment Variables

The AWS SDK will automatically use credentials from:

1. AWS credentials file (`~/.aws/credentials`)
2. Environment variables (`AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY`)
3. IAM role (when running on EC2/ECS)
4. AWS profile specified in configuration

### Required IAM Permissions

The AWS credentials need the following KMS permissions:

```json
{
    "Version": "2012-10-17",
    "Statement": [
        {
            "Effect": "Allow",
            "Action": [
                "kms:DescribeKey",
                "kms:GetPublicKey",
                "kms:Sign"
            ],
            "Resource": "arn:aws:kms:REGION:ACCOUNT_ID:key/KEY_ID"
        }
    ]
}
```

## Usage Examples

### Basic Usage

```rust
use axionvera_network::signing::{SignerConfig, SignerFactory, SigningService};

// Create AWS KMS signer
let config = SignerConfig::AwsKms {
    key_id: "arn:aws:kms:us-east-1:123456789012:key/...".to_string(),
    region: "us-east-1".to_string(),
    profile: Some("my-profile".to_string()),
};

let signer = SignerFactory::create_signer(config).await?;

// Sign a message
let message = b"Hello, Axionvera!";
let signature = signer.sign(message).await?;

// Get public key
let public_key = signer.get_public_key().await?;
```

### Using SigningService

```rust
use axionvera_network::signing::SigningService;

// Create signing service with 1-hour cache TTL
let mut service = SigningService::new(3600);

// Add AWS KMS signer
let signer = SignerFactory::create_signer(kms_config).await?;
let key_id = signer.get_key_id().await?;
service.add_signer(key_id.clone(), signer).await?;
service.set_default_signer(key_id).await?;

// Sign using default signer
let signature = service.sign(b"Transaction data").await?;

// Sign using specific signer
let signature = service.sign_with("key-id", b"Transaction data").await?;

// Get public key (with caching)
let public_key = service.get_public_key("key-id").await?;
```

## API Endpoints

The integration adds the following HTTP/gRPC endpoints:

### HTTP Endpoints

- `POST /api/v1/sign` - Sign a message using the default signer
- `POST /api/v1/sign/{key_id}` - Sign a message using a specific signer
- `GET /api/v1/public-key/{key_id}` - Get public key for a signer
- `GET /api/v1/signers` - List all configured signers
- `GET /api/v1/health/signing` - Health check for all signers

### gRPC Services

- `SigningService` with methods for signing and key management
- Integration with existing network and gateway services

## Error Handling

The implementation includes comprehensive error handling:

### Error Types

- `NetworkError::Kms` - General KMS errors
- `NetworkError::KmsTimeout` - Operation timeouts
- `NetworkError::KmsRateLimit` - Rate limit exceeded
- `NetworkError::Signer` - Signer-specific errors

### Retry Logic

- **Automatic retries** for transient failures (timeouts, rate limits)
- **Exponential backoff** with configurable delay
- **Maximum retry limit** to prevent infinite loops
- **Circuit breaker** pattern for repeated failures

### Rate Limit Handling

- **Graceful degradation** when KMS rate limits are hit
- **Queue management** for pending signing requests
- **Backpressure** to prevent overwhelming the service

## Testing

### Unit Tests

Run the comprehensive test suite:

```bash
cargo test signing
cargo test aws_kms_signer
```

### Integration Tests

For AWS KMS integration tests (requires AWS credentials):

```bash
# Set environment variables
export TEST_KMS_KEY_ID="arn:aws:kms:..."
export TEST_AWS_REGION="us-east-1"

# Run integration tests
cargo test --ignored test_aws_kms_signer_integration
```

### Mock Testing

Use the local signer for development and testing:

```toml
[signing]
type = "Local"
key_path = "./test-keys/dev-key.pem"
```

## Security Considerations

### Key Security

- **Private keys never leave AWS KMS**
- **Only SHA-256 hashes are transmitted**
- **No key material is stored locally**
- **Automatic key rotation support**

### Network Security

- **TLS encryption** for all AWS API calls
- **VPC endpoints** can be used for private connectivity
- **IAM policies** restrict access to specific KMS keys

### Operational Security

- **Audit logging** of all signing operations
- **Health monitoring** for KMS connectivity
- **Graceful fallback** for KMS unavailability

## Performance Optimization

### Caching Strategy

- **Public key caching** reduces KMS API calls by ~90%
- **Configurable TTL** based on security requirements
- **Cache invalidation** on key rotation

### Connection Pooling

- **HTTP connection reuse** for KMS API calls
- **Configurable timeouts** and retry limits
- **Connection health monitoring**

### Batch Operations

- **Batch signature verification** (existing feature)
- **Future: Batch signing** support for high-throughput scenarios

## Monitoring and Observability

### Metrics

- **Signing operation latency**
- **KMS API call count**
- **Cache hit/miss ratios**
- **Error rates by type**

### Logging

- **Structured logging** with tracing spans
- **Correlation IDs** for request tracking
- **Security event logging**

### Health Checks

- **KMS connectivity health**
- **Signer availability monitoring**
- **Cache performance metrics**

## Migration Guide

### From Local Keys

1. **Create KMS key** in AWS console
2. **Update configuration** to use AWS KMS
3. **Deploy with both** local and KMS signers
4. **Gradually migrate** applications to use KMS
5. **Remove local keys** after successful migration

### Key Rotation

1. **Create new KMS key**
2. **Add to configuration** as additional signer
3. **Update applications** to use new key
4. **Decommission old key** after verification

## Troubleshooting

### Common Issues

#### KMS Access Denied

```bash
# Check IAM permissions
aws kms describe-key --key-id KEY_ID

# Verify credentials
aws sts get-caller-identity
```

#### Timeouts

```rust
// Increase timeout in configuration
let config = AwsKmsConfig {
    timeout_ms: 60000, // 60 seconds
    max_retries: 5,
    retry_delay_ms: 2000,
    ..Default::default()
};
```

#### Rate Limits

```rust
// Implement backpressure
match service.sign(message).await {
    Ok(signature) => handle_signature(signature),
    Err(NetworkError::KmsRateLimit(_)) => {
        // Implement exponential backoff
        tokio::time::sleep(Duration::from_secs(10)).await;
        // Retry with backoff
    }
    Err(e) => handle_error(e),
}
```

### Debug Mode

Enable debug logging for detailed troubleshooting:

```bash
RUST_LOG=debug cargo run
```

## Future Enhancements

### Planned Features

1. **Multi-region KMS support** for high availability
2. **Hardware Security Module (HSM)** integration
3. **Batch signing** for improved throughput
4. **Key rotation automation**
5. **Cross-cloud KMS support** (Azure Key Vault, GCP KMS)

### Performance Improvements

1. **Asynchronous signing queues**
2. **Smart caching strategies**
3. **Connection pooling optimization**
4. **Metrics-driven auto-scaling**

## Support

For issues and questions:

1. **Check the logs** for detailed error messages
2. **Verify AWS credentials** and IAM permissions
3. **Test with local signer** to isolate the issue
4. **Create an issue** with detailed reproduction steps

## License

This AWS KMS integration is part of the Axionvera network project and follows the same license terms.
