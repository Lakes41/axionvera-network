#!/bin/bash

set -e

NAMESPACE="axionvera-network"
HPA_NAME="axionvera-node-hpa"
DEPLOYMENT_NAME="axionvera-node"

echo "=== Axionvera Network HPA Test Script ==="
echo ""

# Function to check if resource exists
check_resource() {
    local resource_type=$1
    local resource_name=$2
    local namespace=$3
    
    if kubectl get $resource_type $resource_name -n $namespace >/dev/null 2>&1; then
        echo "✓ $resource_type '$resource_name' exists in namespace '$namespace'"
        return 0
    else
        echo "✗ $resource_type '$resource_name' not found in namespace '$namespace'"
        return 1
    fi
}

# Function to check HPA status
check_hpa_status() {
    echo "=== HPA Status ==="
    kubectl get hpa $HPA_NAME -n $NAMESPACE -o yaml
    echo ""
    
    echo "=== Current HPA Metrics ==="
    kubectl get hpa $HPA_NAME -n $NAMESPACE
    echo ""
}

# Function to check custom metrics
check_custom_metrics() {
    echo "=== Checking Custom Metrics Availability ==="
    
    # Check if custom metrics API is available
    if kubectl get --raw "/apis/custom.metrics.k8s.io/v1beta1" >/dev/null 2>&1; then
        echo "✓ Custom metrics API is available"
    else
        echo "✗ Custom metrics API is not available"
        return 1
    fi
    
    # Check for our specific metric
    echo "=== Available Axionvera Custom Metrics ==="
    kubectl get --raw "/apis/custom.metrics.k8s.io/v1beta1" | jq '.resources[] | select(.name | contains("axionvera"))' || echo "No axionvera metrics found"
    echo ""
    
    # Check current metric values
    echo "=== Current Transaction Queue Depth Values ==="
    kubectl get --raw "/apis/custom.metrics.k8s.io/v1beta1/namespaces/$NAMESPACE/pods/*/axionvera_transaction_queue_depth" 2>/dev/null || echo "Metric not available yet"
    echo ""
}

# Function to simulate load
simulate_load() {
    echo "=== Simulating Transaction Load ==="
    
    # Get service endpoint
    SERVICE_IP=$(kubectl get svc axionvera-node-service -n $NAMESPACE -o jsonpath='{.spec.clusterIP}')
    
    if [ -z "$SERVICE_IP" ]; then
        echo "✗ Could not get service IP"
        return 1
    fi
    
    echo "Service IP: $SERVICE_IP"
    
    # Start port-forward in background
    echo "Setting up port-forward..."
    kubectl port-forward -n $NAMESPACE svc/axionvera-node-service 8080:8080 &
    PF_PID=$!
    
    # Wait for port-forward to be ready
    sleep 5
    
    # Generate transactions to increase queue
    echo "Generating transactions to increase queue depth..."
    for i in {1..1500}; do
        curl -s -X POST http://localhost:8080/api/transactions \
            -H "Content-Type: application/json" \
            -d '{"from": "test_user", "to": "target_user", "amount": 100}' \
            >/dev/null 2>&1 || true
        
        if [ $((i % 100)) -eq 0 ]; then
            echo "Generated $i transactions..."
        fi
    done
    
    echo "Load generation completed"
    
    # Clean up port-forward
    kill $PF_PID 2>/dev/null || true
}

# Function to monitor scaling
monitor_scaling() {
    echo "=== Monitoring HPA Scaling (60 seconds) ==="
    
    for i in {1..12}; do
        echo "--- Check $i/12 ---"
        echo "Time: $(date)"
        
        # Show HPA status
        kubectl get hpa $HPA_NAME -n $NAMESPACE
        
        # Show replica count
        REPLICAS=$(kubectl get deployment $DEPLOYMENT_NAME -n $NAMESPACE -o jsonpath='{.status.readyReplicas}')
        echo "Ready replicas: $REPLICAS"
        
        # Show custom metric
        echo "Transaction queue depth:"
        kubectl get --raw "/apis/custom.metrics.k8s.io/v1beta1/namespaces/$NAMESPACE/pods/*/axionvera_transaction_queue_depth" 2>/dev/null || echo "Not available"
        
        echo ""
        sleep 5
    done
}

# Function to check logs
check_logs() {
    echo "=== Checking Component Logs ==="
    
    echo "--- Prometheus Adapter Logs ---"
    kubectl logs -n monitoring -l app=prometheus-adapter --tail=20
    echo ""
    
    echo "--- Metrics Server Logs ---"
    kubectl logs -n kube-system -l k8s-app=metrics-server --tail=20
    echo ""
    
    echo "--- Sample Application Logs ---"
    kubectl logs -n $NAMESPACE -l app=axionvera-node --tail=10
    echo ""
}

# Main test execution
echo "Starting HPA validation test..."
echo ""

# Check prerequisites
echo "=== Checking Prerequisites ==="

check_resource "namespace" $NAMESPACE "" || exit 1
check_resource "deployment" $DEPLOYMENT_NAME $NAMESPACE || exit 1
check_resource "hpa" $HPA_NAME $NAMESPACE || exit 1
check_resource "service" "prometheus-service" "monitoring" || exit 1
check_resource "deployment" "prometheus-adapter" "monitoring" || exit 1

echo ""

# Initial status check
echo "=== Initial Status ==="
check_hpa_status
check_custom_metrics

# Test custom metrics availability
echo "=== Testing Custom Metrics ==="
if ! check_custom_metrics; then
    echo "Custom metrics not properly configured. Please check prometheus-adapter logs."
    check_logs
    exit 1
fi

# Simulate load
echo "=== Load Testing Phase ==="
simulate_load

# Wait a bit for metrics to be collected
echo "Waiting for metrics to be collected..."
sleep 30

# Monitor scaling
echo "=== Scaling Monitoring Phase ==="
monitor_scaling

# Final status
echo "=== Final Status ==="
check_hpa_status

echo ""
echo "=== Test Summary ==="
echo "✓ Test completed"
echo ""
echo "To continue monitoring:"
echo "kubectl get hpa $HPA_NAME -n $NAMESPACE -w"
echo ""
echo "To check custom metrics:"
echo "kubectl get --raw '/apis/custom.metrics.k8s.io/v1beta1/namespaces/$NAMESPACE/pods/*/axionvera_transaction_queue_depth'"
echo ""
echo "To check pod scaling:"
echo "kubectl get pods -n $NAMESPACE -l app=axionvera-node -w"
