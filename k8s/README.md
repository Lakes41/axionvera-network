# Axionvera Network Kubernetes Deployment with HPA

This directory contains Kubernetes manifests for deploying the Axionvera Network with Horizontal Pod Autoscaling (HPA) based on custom metrics.

## Architecture

The deployment includes:

1. **Metrics Stack**: Prometheus + Prometheus Adapter + Metrics Server
2. **Custom Metrics**: Pending Transaction Queue Depth tracking
3. **HPA**: Autoscaling based on custom metrics and resource utilization
4. **Application**: Axionvera Network Node deployment

## Components

### Monitoring Stack

- **Prometheus**: Collects metrics from all pods
- **Metrics Server**: Provides standard CPU/memory metrics to Kubernetes
- **Prometheus Adapter**: Exposes custom metrics to the Kubernetes autoscaler

### Custom Metrics

The following custom metrics are exposed:

- `axionvera_pending_transactions_total`: Number of pending transactions in queue
- `axionvera_transaction_queue_depth`: Current transaction queue depth

### HPA Configuration

The HPA scales the `axionvera-node` deployment based on:

- **Custom Metric**: Scale when `axionvera_transaction_queue_depth` > 1000
- **CPU Utilization**: Scale when CPU > 70%
- **Memory Utilization**: Scale when memory > 80%
- **Replica Range**: 3-10 replicas

### Scale-down Stabilization

To prevent flapping during fluctuating traffic:

- **Stabilization Window**: 300 seconds (5 minutes)
- **Scale-down Policies**: 
  - Max 10% reduction per minute
  - Max 1 pod reduction per minute
  - Selects the minimum change policy

## Deployment

### Prerequisites

- Kubernetes cluster (v1.20+)
- kubectl configured
- Helm (optional, for easier management)

### Quick Deploy

```bash
# Make the deploy script executable
chmod +x deploy.sh

# Deploy everything
./deploy.sh
```

### Manual Deploy

```bash
# 1. Create namespace
kubectl apply -f namespace.yaml

# 2. Deploy monitoring stack
kubectl apply -f prometheus.yaml
kubectl apply -f metrics-server.yaml
kubectl apply -f prometheus-adapter.yaml

# 3. Deploy application
kubectl apply -f axionvera-node-deployment.yaml

# 4. Deploy HPA
kubectl apply -f hpa.yaml
```

## Verification

### Check HPA Status

```bash
kubectl get hpa axionvera-node-hpa -n axionvera-network -w
```

### Check Custom Metrics

```bash
# List available custom metrics
kubectl get --raw "/apis/custom.metrics.k8s.io/v1beta1" | jq

# Check transaction queue depth metric
kubectl get --raw '/apis/custom.metrics.k8s.io/v1beta1/namespaces/axionvera-network/pods/*/axionvera_transaction_queue_depth'
```

### Check Application Status

```bash
# Pods
kubectl get pods -n axionvera-network -l app=axionvera-node

# Services
kubectl get services -n axionvera-network

# Logs
kubectl logs -n axionvera-network -l app=axionvera-node
```

## Testing the HPA

### Generate Load

To test the HPA, you need to generate load that increases the transaction queue:

```bash
# Access the application
kubectl port-forward -n axionvera-network svc/axionvera-node-service 8080:8080

# Generate transactions (example using curl)
for i in {1..2000}; do
  curl -X POST http://localhost:8080/transactions \
    -H "Content-Type: application/json" \
    -d '{"from": "user1", "to": "user2", "amount": 100}' &
done
```

### Monitor Scaling

```bash
# Watch HPA in real-time
kubectl get hpa axionvera-node-hpa -n axionvera-network -w

# Watch pod scaling
kubectl get pods -n axionvera-network -l app=axionvera-node -w
```

## Troubleshooting

### Common Issues

1. **Custom Metrics Not Available**
   ```bash
   # Check prometheus-adapter logs
   kubectl logs -n monitoring -l app=prometheus-adapter
   
   # Verify metrics are being collected
   kubectl port-forward -n monitoring svc/prometheus-service 9090:9090
   # Visit http://localhost:9090 and check for axionvera metrics
   ```

2. **HPA Not Scaling**
   ```bash
   # Check HPA events
   kubectl describe hpa axionvera-node-hpa -n axionvera-network
   
   # Check if custom metrics are accessible
   kubectl get --raw '/apis/custom.metrics.k8s.io/v1beta1/namespaces/axionvera-network/pods/*/axionvera_transaction_queue_depth'
   ```

3. **Prometheus Not Scraping Metrics**
   ```bash
   # Check service discovery
   kubectl port-forward -n monitoring svc/prometheus-service 9090:9090
   # Visit http://localhost:9090/targets
   ```

### Cleanup

```bash
# Delete all resources
kubectl delete -f hpa.yaml
kubectl delete -f axionvera-node-deployment.yaml
kubectl delete -f prometheus-adapter.yaml
kubectl delete -f metrics-server.yaml
kubectl delete -f prometheus.yaml
kubectl delete -f namespace.yaml
```

## Configuration

### Adjusting HPA Thresholds

Edit `hpa.yaml` to modify:

- `minReplicas`/`maxReplicas`: Replica range
- `averageValue`: Custom metric threshold (currently 1000)
- `averageUtilization`: CPU/memory thresholds
- `stabilizationWindowSeconds`: Scale-down stabilization

### Adjusting Metrics Collection

Edit `prometheus.yaml` to modify:

- `scrape_interval`: How often metrics are collected
- `evaluation_interval`: How often rules are evaluated
- `retention`: How long metrics are stored

### Adjusting Application Resources

Edit `axionvera-node-deployment.yaml` to modify:

- Resource requests/limits
- Probe configurations
- Environment variables
