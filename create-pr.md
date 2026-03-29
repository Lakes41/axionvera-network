# Pull Request Creation Instructions

## 🚀 Create Pull Request for Issue #420

### Method 1: Using GitHub Web UI

1. **Navigate to GitHub**
   - Go to: https://github.com/Ardecrownn/axionvera-network
   - You should see a banner suggesting to create a pull request for your recently pushed branch

2. **Create Pull Request**
   - Click "Compare & pull request"
   - Base repository: `Ardecrownn/axionvera-network`
   - Base branch: `main`
   - Head repository: `Ardecrownn/axionvera-network`
   - Head branch: `feature/hpa-custom-metrics`

3. **Fill PR Details**
   - **Title**: `feat: Implement HPA based on Custom Metrics for Issue #420`
   - **Description**: Copy the content from `HPA_PR_DESCRIPTION.md`
   - **Labels**: Add relevant labels (enhancement, infrastructure, kubernetes)
   - **Assignees**: Assign to yourself or appropriate reviewer

### Method 2: Using GitHub CLI (if installed)

```bash
# Install GitHub CLI first
# Windows: winget install GitHub.cli
# Or download from: https://cli.github.com/

# Then create PR
gh pr create \
  --title "feat: Implement HPA based on Custom Metrics for Issue #420" \
  --body "$(cat HPA_PR_DESCRIPTION.md)" \
  --base main \
  --head feature/hpa-custom-metrics \
  --label enhancement \
  --label infrastructure \
  --label kubernetes
```

### Method 3: Using Git Commands

```bash
# Create PR using git (requires GitHub CLI)
git request-pull main origin feature/hpa-custom-metrics
```

## 📋 PR Description Content

The PR description is ready in `HPA_PR_DESCRIPTION.md`. You can copy and paste it directly into the GitHub PR form.

## ✅ Pre-merge Checklist

Before merging, ensure:

- [ ] All tests pass
- [ ] Documentation is updated
- [ ] Code review completed
- [ ] No merge conflicts
- [ ] CI/CD pipeline passes
- [ ] Security review completed (if applicable)

## 🔗 Quick Links

- **Branch**: https://github.com/Ardecrownn/axionvera-network/tree/feature/hpa-custom-metrics
- **Compare**: https://github.com/Ardecrownn/axionvera-network/compare/main...feature/hpa-custom-metrics
- **Issue**: https://github.com/Axionvera/axionvera-network/issues/420

## 📝 PR Summary

This PR implements:
- ✅ Kubernetes metrics-server and prometheus-adapter
- ✅ Custom metrics for pending transaction queue depth
- ✅ HPA resource (3-10 replicas) scaling when queue > 1000
- ✅ Scale-down stabilization (300s) to prevent flapping
- ✅ EKS migration from EC2 Auto Scaling
- ✅ Comprehensive deployment and testing scripts
- ✅ Detailed documentation and troubleshooting guides

The implementation addresses Issue #420 by providing faster scaling for network traffic spikes using custom metrics instead of standard CPU/RAM scaling.
