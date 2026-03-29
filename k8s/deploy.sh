#!/bin/bash

set -e

echo "Deploying Axionvera Network with HPA based on Custom Metrics..."

# Create namespace
echo "Creating namespace..."
kubectl apply -f namespace.yaml

# Deploy monitoring stack
echo "Deploying Prometheus..."
kubectl apply -f prometheus.yaml

echo "Deploying metrics-server..."
kubectl apply -f metrics-server.yaml

echo "Deploying prometheus-adapter..."
kubectl apply -f prometheus-adapter.yaml

# Wait for monitoring stack to be ready
echo "Waiting for monitoring stack to be ready..."
kubectl wait --for=condition=available --timeout=300s deployment/prometheus -n monitoring
kubectl wait --for=condition=available --timeout=300s deployment/metrics-server -n kube-system
kubectl wait --for=condition=available --timeout=300s deployment/prometheus-adapter -n monitoring

# Deploy application
echo "Deploying axionvera-node..."
kubectl apply -f axionvera-node-deployment.yaml

# Wait for application to be ready
echo "Waiting for axionvera-node to be ready..."
kubectl wait --for=condition=available --timeout=300s deployment/axionvera-node -n axionvera-network

# Deploy HPA
echo "Deploying Horizontal Pod Autoscaler..."
kubectl apply -f hpa.yaml

# Verify deployment
echo "Verifying deployment..."
echo "=== HPA Status ==="
kubectl get hpa axionvera-node-hpa -n axionvera-network

echo "=== Available Custom Metrics ==="
kubectl get --raw "/apis/custom.metrics.k8s.io/v1beta1" | jq '.resources[] | select(.name | contains("axionvera"))'

echo "=== Pods Status ==="
kubectl get pods -n axionvera-network -l app=axionvera-node

echo "=== Services Status ==="
kubectl get services -n axionvera-network

echo "=== Prometheus Status ==="
kubectl get pods -n monitoring -l app=prometheus

echo "Deployment completed successfully!"
echo ""
echo "To test the HPA:"
echo "1. Generate load on the application to increase transaction queue"
echo "2. Monitor HPA: kubectl get hpa axionvera-node-hpa -n axionvera-network -w"
echo "3. Check custom metrics: kubectl get --raw '/apis/custom.metrics.k8s.io/v1beta1/namespaces/axionvera-network/pods/*/axionvera_transaction_queue_depth'"
echo ""
echo "To check logs:"
echo "Prometheus: kubectl logs -n monitoring -l app=prometheus"
echo "Prometheus Adapter: kubectl logs -n monitoring -l app=prometheus-adapter"
