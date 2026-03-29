use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{error, info, warn};
use serde::{Deserialize, Serialize};
use crate::config::{HorizonConfig, HorizonProvider};
use crate::error::NetworkError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CircuitBreakerState {
    Closed,    // Normal operation
    Open,      // Circuit is open, blocking calls
    HalfOpen,  // Testing if the service has recovered
}

#[derive(Debug, Clone)]
pub struct ProviderStatus {
    pub provider: HorizonProvider,
    pub failure_count: u8,
    pub last_failure_time: Option<Instant>,
    pub circuit_state: CircuitBreakerState,
    pub is_healthy: bool,
    pub last_health_check: Option<Instant>,
}

impl ProviderStatus {
    pub fn new(provider: HorizonProvider) -> Self {
        Self {
            provider,
            failure_count: 0,
            last_failure_time: None,
            circuit_state: CircuitBreakerState::Closed,
            is_healthy: true,
            last_health_check: None,
        }
    }

    pub fn record_failure(&mut self, threshold: u8) {
        self.failure_count += 1;
        self.last_failure_time = Some(Instant::now());
        
        if self.failure_count >= threshold {
            self.circuit_state = CircuitBreakerState::Open;
            warn!(
                provider = %self.provider.name,
                url = %self.provider.url,
                failure_count = self.failure_count,
                "Circuit breaker opened for Horizon provider"
            );
        }
    }

    pub fn record_success(&mut self) {
        self.failure_count = 0;
        self.last_failure_time = None;
        self.is_healthy = true;
        self.last_health_check = Some(Instant::now());
        
        if self.circuit_state == CircuitBreakerState::HalfOpen {
            self.circuit_state = CircuitBreakerState::Closed;
            info!(
                provider = %self.provider.name,
                url = %self.provider.url,
                "Circuit breaker closed for Horizon provider"
            );
        }
    }

    pub fn should_attempt_reset(&self, recovery_timeout: Duration) -> bool {
        if self.circuit_state == CircuitBreakerState::Open {
            if let Some(last_failure) = self.last_failure_time {
                last_failure.elapsed() > recovery_timeout
            } else {
                true
            }
        } else {
            false
        }
    }

    pub fn can_execute_request(&self) -> bool {
        match self.circuit_state {
            CircuitBreakerState::Closed => self.is_healthy,
            CircuitBreakerState::Open => false,
            CircuitBreakerState::HalfOpen => true,
        }
    }
}

pub struct HorizonClient {
    config: HorizonConfig,
    providers: Arc<RwLock<Vec<ProviderStatus>>>,
    current_provider_index: Arc<RwLock<usize>>,
    http_client: reqwest::Client,
}

impl HorizonClient {
    pub fn new(config: HorizonConfig) -> Self {
        let providers: Vec<ProviderStatus> = config.providers
            .clone()
            .into_iter()
            .map(ProviderStatus::new)
            .collect();

        // Sort by priority
        let mut sorted_providers = providers;
        sorted_providers.sort_by_key(|p| p.provider.priority);

        Self {
            config,
            providers: Arc::new(RwLock::new(sorted_providers)),
            current_provider_index: Arc::new(RwLock::new(0)),
            http_client: reqwest::Client::builder()
                .timeout(Duration::from_secs(30))
                .build()
                .expect("Failed to create HTTP client"),
        }
    }

    /// Get the current healthy provider for making requests
    pub async fn get_current_provider(&self) -> Result<HorizonProvider, NetworkError> {
        let providers = self.providers.read().await;
        let current_index = *self.current_provider_index.read().await;

        // Try current provider first
        if let Some(provider_status) = providers.get(current_index) {
            if provider_status.can_execute_request() {
                return Ok(provider_status.provider.clone());
            }
        }

        // Find next healthy provider
        for (i, provider_status) in providers.iter().enumerate() {
            if i != current_index && provider_status.can_execute_request() {
                return Ok(provider_status.provider.clone());
            }
        }

        Err(NetworkError::HorizonClient(
            "No healthy Horizon providers available".to_string()
        ))
    }

    /// Switch to next available provider
    pub async fn switch_provider(&self) -> Result<HorizonProvider, NetworkError> {
        let providers = self.providers.read().await;
        let mut current_index = self.current_provider_index.write().await;

        // Try to find next healthy provider
        for i in 0..providers.len() {
            let next_index = (*current_index + i + 1) % providers.len();
            if let Some(provider_status) = providers.get(next_index) {
                if provider_status.can_execute_request() {
                    let old_provider = providers.get(*current_index)
                        .map(|p| &p.provider.name)
                        .unwrap_or("unknown");
                    
                    *current_index = next_index;
                    
                    warn!(
                        from_provider = old_provider,
                        to_provider = %provider_status.provider.name,
                        to_url = %provider_status.provider.url,
                        "Switched to fallback Horizon provider"
                    );
                    
                    return Ok(provider_status.provider.clone());
                }
            }
        }

        Err(NetworkError::HorizonClient(
            "No healthy fallback providers available".to_string()
        ))
    }

    /// Execute a request with automatic fallback
    pub async fn execute_request<F, T, E>(&self, operation: F) -> Result<T, NetworkError>
    where
        F: Fn(HorizonProvider) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<T, E>> + Send>>,
        E: std::error::Error + Send + Sync + 'static,
    {
        let mut attempts = 0;
        let max_attempts = self.config.providers.len();

        loop {
            attempts += 1;

            // Get current provider
            let provider = match self.get_current_provider().await {
                Ok(p) => p,
                Err(e) => {
                    if attempts >= max_attempts {
                        return Err(e);
                    }
                    // Try to switch to another provider
                    self.switch_provider().await?;
                    continue;
                }
            };

            // Execute the operation
            match operation(provider.clone()).await {
                Ok(result) => {
                    // Record success
                    self.record_provider_success(&provider).await;
                    return Ok(result);
                }
                Err(e) => {
                    // Record failure
                    let should_switch = self.record_provider_failure(&provider).await;
                    
                    error!(
                        provider = %provider.name,
                        url = %provider.url,
                        error = %e,
                        attempt = attempts,
                        "Horizon API request failed"
                    );

                    if should_switch && attempts < max_attempts {
                        // Try to switch to next provider
                        if let Err(switch_err) = self.switch_provider().await {
                            return Err(NetworkError::HorizonClient(format!(
                                "Failed to switch provider: {}", switch_err
                            )));
                        }
                        continue;
                    } else {
                        return Err(NetworkError::HorizonClient(format!(
                            "All Horizon providers failed: {}", e
                        )));
                    }
                }
            }
        }
    }

    /// Record a successful operation for a provider
    async fn record_provider_success(&self, provider: &HorizonProvider) {
        let mut providers = self.providers.write().await;
        if let Some(provider_status) = providers.iter_mut()
            .find(|p| p.provider.url == provider.url) {
            provider_status.record_success();
        }
    }

    /// Record a failed operation for a provider
    async fn record_provider_failure(&self, provider: &HorizonProvider) -> bool {
        let mut providers = self.providers.write().await;
        if let Some(provider_status) = providers.iter_mut()
            .find(|p| p.provider.url == provider.url) {
            provider_status.record_failure(self.config.circuit_breaker_failure_threshold);
            
            // Return true if circuit breaker opened (should switch providers)
            matches!(provider_status.circuit_state, CircuitBreakerState::Open)
        } else {
            false
        }
    }

    /// Health check for all providers
    pub async fn health_check_all(&self) -> Result<(), NetworkError> {
        let providers = self.providers.read().await;
        let health_check_tasks: Vec<_> = providers.iter()
            .map(|provider_status| {
                let provider = provider_status.provider.clone();
                let client = self.http_client.clone();
                
                tokio::spawn(async move {
                    let url = format!("{}/", provider.url.trim_end_matches('/'));
                    match client.get(&url).send().await {
                        Ok(response) => {
                            let is_healthy = response.status().is_success();
                            (provider.clone(), is_healthy, None)
                        }
                        Err(e) => {
                            (provider.clone(), false, Some(e.to_string()))
                        }
                    }
                })
            })
            .collect();

        // Wait for all health checks to complete
        let results = futures::future::join_all(health_check_tasks).await;
        
        // Update provider statuses
        let mut providers_mut = self.providers.write().await;
        for result in results {
            if let Ok(Ok((provider, is_healthy, error))) = result {
                if let Some(provider_status) = providers_mut.iter_mut()
                    .find(|p| p.provider.url == provider.url) {
                    
                    let was_healthy = provider_status.is_healthy;
                    provider_status.is_healthy = is_healthy;
                    provider_status.last_health_check = Some(Instant::now());

                    if !is_healthy && was_healthy {
                        warn!(
                            provider = %provider.name,
                            url = %provider.url,
                            error = ?error,
                            "Horizon provider health check failed"
                        );
                    } else if is_healthy && !was_healthy {
                        info!(
                            provider = %provider.name,
                            url = %provider.url,
                            "Horizon provider health check passed"
                        );
                    }
                }
            }
        }

        Ok(())
    }

    /// Start background health checker
    pub async fn start_health_checker(self: Arc<Self>) -> tokio::task::JoinHandle<()> {
        let health_check_interval = Duration::from_secs(self.config.health_check_interval_seconds);
        
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(health_check_interval);
            
            loop {
                interval.tick().await;
                
                if let Err(e) = self.health_check_all().await {
                    error!(
                        error = %e,
                        "Background health check failed"
                    );
                }
            }
        })
    }

    /// Get provider statuses for monitoring
    pub async fn get_provider_statuses(&self) -> Vec<ProviderStatus> {
        self.providers.read().await.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_provider_status_circuit_breaker() {
        let provider = HorizonProvider {
            url: "https://test.com".to_string(),
            name: "Test".to_string(),
            priority: 1,
        };
        
        let mut status = ProviderStatus::new(provider);
        
        // Record failures to trigger circuit breaker
        for i in 0..3 {
            status.record_failure(3);
            assert_eq!(status.failure_count, i + 1);
        }
        
        // Circuit should be open now
        assert!(matches!(status.circuit_state, CircuitBreakerState::Open));
        assert!(!status.can_execute_request());
        
        // Test recovery timeout
        let recovery_timeout = Duration::from_secs(1);
        assert!(!status.should_attempt_reset(recovery_timeout));
        
        // Simulate time passing
        status.last_failure_time = Some(Instant::now() - Duration::from_secs(2));
        assert!(status.should_attempt_reset(recovery_timeout));
    }

    #[tokio::test]
    async fn test_horizon_client_creation() {
        let config = HorizonConfig::default();
        let client = HorizonClient::new(config);
        
        let providers = client.get_provider_statuses().await;
        assert!(!providers.is_empty());
        assert_eq!(providers.len(), 3); // Default has 3 providers
    }
}
