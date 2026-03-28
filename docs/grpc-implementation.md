# gRPC/JSON-RPC Bridge Implementation

This document describes the implementation of the gRPC/JSON-RPC bridge for the Axionvera Network, addressing Issue #319.

## Overview

The gRPC/JSON-RPC bridge provides a high-performance, efficient communication layer for contract interactions, replacing traditional REST endpoints for better performance in high-frequency scenarios.

## Architecture

### Core Components

1. **Protocol Buffers (.proto files)**
   - `proto/network.proto` - Core network service definitions
   - `proto/gateway.proto` - HTTP/JSON-RPC gateway definitions with OpenAPI annotations

2. **gRPC Server Implementation**
   - `src/grpc/network_service.rs` - Core network service implementation
   - `src/grpc/gateway_service.rs` - Gateway service with HTTP compatibility
   - `src/grpc/health_service.rs` - Health monitoring service
   - `src/grpc/p2p_service.rs` - Peer-to-peer communication service
   - `src/grpc/server.rs` - gRPC server configuration and startup

3. **HTTP Gateway**
   - `src/gateway.rs` - HTTP endpoints with OpenAPI documentation
   - `src/openapi.rs` - Swagger/OpenAPI documentation setup

## Services

### 1. NetworkService

Core contract interaction service providing:

**Contract Operations:**
- `Deposit` - Deposit tokens into vault
- `Withdraw` - Withdraw tokens from vault
- `DistributeRewards` - Distribute rewards to users
- `ClaimRewards` - Claim pending rewards

**Query Operations:**
- `GetBalance` - Get user balance
- `GetRewards` - Get user rewards information
- `GetContractState` - Get contract state
- `GetTransaction` - Get transaction details
- `GetTransactionHistory` - Get transaction history

**Network Operations:**
- `GetNetworkStatus` - Get network status
- `GetNodeInfo` - Get node information

### 2. P2PService

Peer-to-peer communication service:

- `ConnectToPeer` - Connect to a peer
- `DisconnectFromPeer` - Disconnect from a peer
- `GetPeerList` - Get list of connected peers
- `BroadcastMessage` - Broadcast message to peers
- `SyncChain` - Synchronize blockchain data

### 3. HealthService

Health monitoring service:

- `Check` - Single health check
- `Watch` - Streaming health checks

### 4. GatewayService

HTTP/JSON-RPC compatibility layer with enhanced features:

- Request tracking with unique IDs
- Processing time metrics
- Callback URL support for async operations
- Enhanced error handling
- Pagination support

## Configuration

### Environment Variables

```bash
# gRPC Server Configuration
GRPC_BIND_ADDRESS=0.0.0.0:50051          # gRPC server bind address
GATEWAY_BIND_ADDRESS=0.0.0.0:8081        # HTTP gateway bind address

# TLS Configuration (Optional)
TLS_CERT_PATH=/path/to/cert.pem          # TLS certificate path
TLS_KEY_PATH=/path/to/key.pem            # TLS private key path

# Feature Flags
ENABLE_GATEWAY=true                       # Enable HTTP gateway
ENABLE_REFLECTION=true                    # Enable gRPC reflection (dev only)
```

## Performance Features

### 1. High-Performance gRPC

- Binary protocol for efficient serialization
- HTTP/2 for multiplexing
- Streaming support for real-time updates
- Connection pooling and reuse

### 2. Message Size Optimization

- Configurable message size limits
- Compression support (gzip)
- Efficient Protocol Buffer encoding

### 3. Caching and Optimization

- Request deduplication
- Response caching for queries
- Connection keep-alive

## Security Features

### 1. Authentication

- Signature validation for all operations
- Nonce-based replay protection
- Request tracking and audit logging

### 2. TLS Support

- Mutual TLS authentication
- Certificate-based encryption
- Secure channel establishment

### 3. Rate Limiting

- Request rate limiting per client
- Resource usage monitoring
- Circuit breaker pattern

## API Documentation

### Swagger/OpenAPI

The HTTP gateway provides automatic OpenAPI documentation:

- **Swagger UI**: `http://localhost:8081/swagger-ui`
- **OpenAPI JSON**: `http://localhost:8081/api-docs/openapi.json`

### gRPC Reflection

Development environments support gRPC reflection:

```bash
# List services
grpcurl -plaintext localhost:50051 list

# Describe service
grpcurl -plaintext localhost:50051 describe axionvera.network.NetworkService

# Call method
grpcurl -plaintext -d '{"user_address":"0x123..."}' \
  localhost:50051 axionvera.network.NetworkService/GetBalance
```

## Usage Examples

### gRPC Client (Rust)

```rust
use tonic::transport::Channel;
use axionvera_network::grpc::network::network_service_client::NetworkServiceClient;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut client = NetworkServiceClient::connect("http://localhost:50051").await?;
    
    let request = DepositRequest {
        user_address: "0x1234567890123456789012345678901234567890".to_string(),
        token_address: "0xtokenaddress".to_string(),
        amount: "1000000".to_string(),
        signature: vec![0u8; 65],
        nonce: 1,
        timestamp: Some(prost_types::Timestamp::now()),
    };
    
    let response = client.deposit(request).await?;
    println!("Deposit response: {:?}", response.into_inner());
    
    Ok(())
}
```

### HTTP Client (JavaScript)

```javascript
// Deposit tokens
const depositResponse = await fetch('http://localhost:8081/v1/contract/deposit', {
    method: 'POST',
    headers: {
        'Content-Type': 'application/json',
    },
    body: JSON.stringify({
        user_address: '0x1234567890123456789012345678901234567890',
        token_address: '0xtokenaddress',
        amount: '1000000',
        signature: 'base64-encoded-signature',
        nonce: 1,
        timestamp: new Date().toISOString(),
        request_id: 'req_123456'
    })
});

const depositResult = await depositResponse.json();
console.log('Deposit result:', depositResult);

// Get balance
const balanceResponse = await fetch(
    'http://localhost:8081/v1/query/balance?user_address=0x123...&token_address=0xtoken...'
);
const balanceResult = await balanceResponse.json();
console.log('Balance:', balanceResult);
```

## Monitoring and Observability

### 1. Metrics

- Request/response latency
- Error rates by service
- Connection pool metrics
- P2P network statistics

### 2. Logging

- Structured logging with request IDs
- Performance metrics
- Error tracking with stack traces
- Security event logging

### 3. Health Checks

- Service health monitoring
- Database connectivity
- P2P network status
- Resource utilization

## Deployment

### Docker Configuration

```dockerfile
# Build stage
FROM rust:1.70 as builder
WORKDIR /app
COPY . .
RUN cargo build --release

# Runtime stage
FROM gcr.io/distroless/cc-debian12
COPY --from=builder /app/target/release/axionvera-network-node /usr/local/bin/
EXPOSE 50051 8081
ENTRYPOINT ["axionvera-network-node"]
```

### Kubernetes Deployment

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: axionvera-grpc
spec:
  replicas: 3
  selector:
    matchLabels:
      app: axionvera-grpc
  template:
    metadata:
      labels:
        app: axionvera-grpc
    spec:
      containers:
      - name: grpc-server
        image: axionvera/network-node:latest
        ports:
        - containerPort: 50051
        - containerPort: 8081
        env:
        - name: GRPC_BIND_ADDRESS
          value: "0.0.0.0:50051"
        - name: GATEWAY_BIND_ADDRESS
          value: "0.0.0.0:8081"
```

## Testing

### Unit Tests

```bash
# Run gRPC service tests
cargo test grpc

# Run gateway tests
cargo test gateway
```

### Integration Tests

```bash
# Start test environment
docker-compose up -d

# Run integration tests
cargo test --test integration

# Cleanup
docker-compose down
```

### Load Testing

```bash
# Install ghz (gRPC load testing tool)
go install github.com/bojand/ghz@latest

# Run load test
ghz --insecure --proto proto/network.proto \
    --call axionvera.network.NetworkService/GetBalance \
    -d '{"user_address":"0x123...","token_address":"0xtoken..."}' \
    -c 100 -n 10000 localhost:50051
```

## Performance Benchmarks

### Expected Performance

- **gRPC Latency**: < 5ms (local), < 50ms (cross-region)
- **HTTP Gateway Latency**: < 10ms (local), < 100ms (cross-region)
- **Throughput**: > 10,000 RPS per instance
- **Concurrent Connections**: > 100,000

### Optimization Tips

1. **Connection Pooling**: Reuse gRPC connections
2. **Streaming**: Use streaming for bulk operations
3. **Compression**: Enable gzip for large payloads
4. **Caching**: Cache frequently accessed data
5. **Load Balancing**: Use gRPC load balancers

## Troubleshooting

### Common Issues

1. **Connection Refused**: Check if gRPC server is running
2. **TLS Errors**: Verify certificate configuration
3. **Timeout Issues**: Increase timeout values
4. **Memory Usage**: Monitor connection pool size

### Debug Commands

```bash
# Check gRPC server status
grpcurl -plaintext localhost:50051 list

# Test health check
curl http://localhost:8081/v1/health

# View logs
docker logs axionvera-grpc

# Monitor connections
netstat -an | grep :50051
```

## Future Enhancements

1. **WebAssembly Support**: WASM-based smart contract execution
2. **GraphQL Gateway**: GraphQL interface over gRPC services
3. **Event Streaming**: Real-time event notifications
4. **Multi-Chain Support**: Cross-chain contract interactions
5. **Advanced Caching**: Redis-based distributed caching

## Contributing

When contributing to the gRPC implementation:

1. Follow Protocol Buffer best practices
2. Update OpenAPI documentation for HTTP endpoints
3. Add comprehensive tests
4. Update this documentation
5. Consider backward compatibility

## License

This implementation is licensed under Apache 2.0 License. See LICENSE file for details.
