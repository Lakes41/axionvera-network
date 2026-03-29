# Horizon API Fallback and Load Balancing

This document describes the Horizon API fallback and load balancing implementation for the Axionvera Network node.

## Overview

The Horizon API client provides automatic failover and load balancing across multiple Stellar Horizon endpoints. This ensures high availability and resilience when communicating with the Stellar network.

## Features

### 1. Multi-Provider Configuration
- Support for multiple Horizon endpoints (Stellar Public, Blockdaemon, Infura, etc.)
- Priority-based provider selection
- Configurable via environment variables

### 2. Circuit Breaker Pattern
- Automatic detection of failing providers
- 3-strike failure threshold before switching providers
- Configurable recovery timeout
- Prevents cascading failures

### 3. Background Health Monitoring
- Continuous health checks every 60 seconds
- Automatic provider health status updates
- Real-time monitoring of provider availability

### 4. Automatic Fallback
- Seamless switching to healthy providers
- Warning logs when switching to fallback providers
- Preserves service continuity during outages

## Configuration

### Environment Variables

```bash
# Horizon provider configuration (JSON format)
HORIZON_CONFIG='{
  "providers": [
    {
      "url": "https://horizon.stellar.org",
      "name": "Stellar Public", 
      "priority": 1
    },
    {
      "url": "https://horizon-blockdaemon.stellar.ovh",
      "name": "Blockdaemon",
      "priority": 2
    },
    {
      "url": "https://stellar-horizon.infura.io",
      "name": "Infura",
      "priority": 3
    }
  ],
  "health_check_interval_seconds": 60,
  "circuit_breaker_failure_threshold": 3,
  "circuit_breaker_recovery_timeout_seconds": 300
}'
```

### Configuration Options

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `providers` | Array | [Stellar Public, Blockdaemon, Infura] | List of Horizon providers |
| `health_check_interval_seconds` | Number | 60 | Health check frequency |
| `circuit_breaker_failure_threshold` | Number | 3 | Failures before circuit breaker opens |
| `circuit_breaker_recovery_timeout_seconds` | Number | 300 | Recovery timeout in seconds |

## API Endpoints

### Stellar Account Information
```http
GET /stellar/account/{account_id}
```
Retrieves account information from the currently active Horizon provider.

### Latest Ledger
```http
GET /stellar/ledger/latest
```
Gets the latest Stellar ledger information.

### Provider Status
```http
GET /stellar/providers/status
```
Returns the status of all configured Horizon providers.

### Switch Provider
```http
POST /stellar/providers/switch
```
Manually switches to the next available provider.

## Usage Examples

### Basic Account Lookup
```bash
curl http://localhost:8080/stellar/account/GA7QYNF7S6W4JZBWKXW7FNJZJ5K4WFAGYFEJHBJHBTHFTDBZMADOVIZP
```

### Check Provider Status
```bash
curl http://localhost:8080/stellar/providers/status
```

Response:
```json
{
  "providers": [
    {
      "name": "Stellar Public",
      "url": "https://horizon.stellar.org",
      "priority": 1,
      "is_healthy": true,
      "failure_count": 0,
      "circuit_state": "Closed",
      "last_health_check": 15
    }
  ],
  "total_providers": 1,
  "healthy_providers": 1
}
```

### Manual Provider Switch
```bash
curl -X POST http://localhost:8080/stellar/providers/switch
```

## Circuit Breaker States

1. **Closed**: Normal operation, requests flow to the provider
2. **Open**: Provider is blocked, requests are redirected to fallback providers
3. **Half-Open**: Testing if the provider has recovered

## Monitoring and Logging

### Warning Logs
The system logs warnings when:
- Switching to a fallback provider
- Circuit breaker opens for a provider
- Health check fails for a provider

Example log output:
```
WARN  Switched to fallback Horizon provider: from_provider="Stellar Public" to_provider="Blockdaemon" to_url="https://horizon-blockdaemon.stellar.ovh"
WARN  Circuit breaker opened for Horizon provider: provider="Stellar Public" url="https://horizon.stellar.org" failure_count=3
```

### Metrics
The system tracks:
- Request success/failure rates per provider
- Circuit breaker state changes
- Provider health status
- Response times

## Error Handling

### HTTP Status Codes
- `200`: Success
- `502`: Horizon client error (provider issues)
- `500`: Internal server error

### Error Response Format
```json
{
  "error": "HORIZON_CLIENT_ERROR",
  "message": "No healthy Horizon providers available"
}
```

## Testing

### Unit Tests
```bash
cargo test -p axionvera-network-node horizon_client
cargo test -p axionvera-network-node stellar_service
```

### Integration Tests
```bash
# Start the node with test configuration
cargo run -p axionvera-network-node

# Test the endpoints
curl http://localhost:8080/stellar/providers/status
curl http://localhost:8080/stellar/ledger/latest
```

## Troubleshooting

### Common Issues

1. **No healthy providers available**
   - Check network connectivity
   - Verify provider URLs are correct
   - Check provider status endpoint

2. **Frequent provider switching**
   - Increase circuit breaker failure threshold
   - Check provider reliability
   - Review health check interval

3. **Slow response times**
   - Check provider priority configuration
   - Monitor network latency
   - Consider adding local provider

### Debug Commands
```bash
# Check current provider status
curl http://localhost:8080/stellar/providers/status

# Force provider switch for testing
curl -X POST http://localhost:8080/stellar/providers/switch

# Check application logs for Horizon-related messages
grep -i horizon /var/log/axionvera-network.log
```

## Security Considerations

1. **HTTPS Only**: All provider URLs should use HTTPS
2. **API Keys**: If using providers that require authentication, store keys securely
3. **Rate Limiting**: Monitor provider rate limits and implement backoff strategies
4. **Network Security**: Ensure outbound connections to Horizon endpoints are allowed

## Future Enhancements

1. **Load Balancing**: Distribute requests across healthy providers
2. **Geographic Routing**: Route requests to nearest providers
3. **Custom Health Checks**: Provider-specific health check endpoints
4. **Metrics Export**: Export provider metrics to monitoring systems
5. **Configuration Hot Reload**: Update provider configuration without restart
