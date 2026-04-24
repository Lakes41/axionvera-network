## Local Testing with Minikube

To verify the HPA configuration locally before deploying to production, you can use `minikube` along with a load-generation tool.

### 1. Enable Required Addons
The HPA requires the Kubernetes Metrics Server to fetch CPU/Memory metrics.
```bash
minikube addons enable metrics-server
```