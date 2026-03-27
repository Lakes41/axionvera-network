use std::sync::Arc;
use std::time::Duration;
use tokio::signal;
use tokio::sync::{broadcast, RwLock};
use tracing::{error, info, warn};

use crate::error::NetworkError;

/// Shutdown signal types
#[derive(Debug, Clone)]
pub enum ShutdownSignal {
    SigTerm,
    SigInt,
    SigQuit,
    Timeout,
    Manual,
}

/// Shutdown handler manages graceful shutdown process
pub struct ShutdownHandler {
    grace_period: Duration,
    signal_sender: broadcast::Sender<ShutdownSignal>,
    shutdown_state: Arc<RwLock<ShutdownState>>,
}

#[derive(Debug, Default)]
struct ShutdownState {
    is_shutting_down: bool,
    shutdown_start_time: Option<chrono::DateTime<chrono::Utc>>,
    signal_received: Option<ShutdownSignal>,
}

impl ShutdownHandler {
    /// Create a new shutdown handler
    pub fn new(grace_period: Duration) -> Self {
        let (signal_sender, _) = broadcast::channel(10);

        Self {
            grace_period,
            signal_sender,
            shutdown_state: Arc::new(RwLock::new(ShutdownState::default())),
        }
    }

    /// Start listening for shutdown signals
    pub async fn start(&self) -> broadcast::Receiver<ShutdownSignal> {
        let signal_sender = self.signal_sender.clone();
        let shutdown_state = self.shutdown_state.clone();
        let grace_period = self.grace_period;

        // Spawn signal handler
        tokio::spawn(async move {
            Self::handle_os_signals(signal_sender.clone(), shutdown_state.clone()).await;
        });

        // Spawn timeout handler
        tokio::spawn(async move {
            Self::handle_shutdown_timeout(signal_sender, grace_period).await;
        });

        self.signal_sender.subscribe()
    }

    /// Handle OS-level signals
    async fn handle_os_signals(
        sender: broadcast::Sender<ShutdownSignal>,
        state: Arc<RwLock<ShutdownState>>,
    ) {
        // Handle SIGTERM
        let sigterm_sender = sender.clone();
        let sigterm_state = state.clone();
        tokio::spawn(async move {
            match signal::unix::signal(signal::unix::SignalKind::terminate()) {
                Ok(mut sigterm) => {
                    info!("Listening for SIGTERM signal");
                    sigterm.recv().await;
                    info!("SIGTERM received");

                    let mut shutdown_state = sigterm_state.write().await;
                    if !shutdown_state.is_shutting_down {
                        shutdown_state.is_shutting_down = true;
                        shutdown_state.shutdown_start_time = Some(chrono::Utc::now());
                        shutdown_state.signal_received = Some(ShutdownSignal::SigTerm);

                        let _ = sigterm_sender.send(ShutdownSignal::SigTerm);
                    }
                }
                Err(e) => error!("Failed to setup SIGTERM handler: {}", e),
            }
        });

        // Handle SIGINT (Ctrl+C)
        let sigint_sender = sender.clone();
        let sigint_state = state.clone();
        tokio::spawn(async move {
            match signal::unix::signal(signal::unix::SignalKind::interrupt()) {
                Ok(mut sigint) => {
                    info!("Listening for SIGINT signal");
                    sigint.recv().await;
                    info!("SIGINT received");

                    let mut shutdown_state = sigint_state.write().await;
                    if !shutdown_state.is_shutting_down {
                        shutdown_state.is_shutting_down = true;
                        shutdown_state.shutdown_start_time = Some(chrono::Utc::now());
                        shutdown_state.signal_received = Some(ShutdownSignal::SigInt);

                        let _ = sigint_sender.send(ShutdownSignal::SigInt);
                    }
                }
                Err(e) => error!("Failed to setup SIGINT handler: {}", e),
            }
        });

        // Handle SIGQUIT
        let sigquit_sender = sender.clone();
        let sigquit_state = state.clone();
        tokio::spawn(async move {
            match signal::unix::signal(signal::unix::SignalKind::quit()) {
                Ok(mut sigquit) => {
                    info!("Listening for SIGQUIT signal");
                    sigquit.recv().await;
                    info!("SIGQUIT received");

                    let mut shutdown_state = sigquit_state.write().await;
                    if !shutdown_state.is_shutting_down {
                        shutdown_state.is_shutting_down = true;
                        shutdown_state.shutdown_start_time = Some(chrono::Utc::now());
                        shutdown_state.signal_received = Some(ShutdownSignal::SigQuit);

                        let _ = sigquit_sender.send(ShutdownSignal::SigQuit);
                    }
                }
                Err(e) => error!("Failed to setup SIGQUIT handler: {}", e),
            }
        });
    }

    /// Handle shutdown timeout
    async fn handle_shutdown_timeout(
        sender: broadcast::Sender<ShutdownSignal>,
        grace_period: Duration,
    ) {
        // This would be triggered when grace period expires
        // In practice, this is handled by the main shutdown logic
    }

    /// Trigger manual shutdown
    pub async fn trigger_shutdown(&self, signal: ShutdownSignal) -> Result<(), NetworkError> {
        let mut state = self.shutdown_state.write().await;

        if state.is_shutting_down {
            warn!("Shutdown already in progress");
            return Ok(());
        }

        info!("Triggering manual shutdown: {:?}", signal);
        state.is_shutting_down = true;
        state.shutdown_start_time = Some(chrono::Utc::now());
        state.signal_received = Some(signal.clone());

        self.signal_sender.send(signal).map_err(|e| {
            NetworkError::Internal(format!("Failed to send shutdown signal: {}", e))
        })?;

        Ok(())
    }

    /// Check if shutdown is in progress
    pub async fn is_shutting_down(&self) -> bool {
        self.shutdown_state.read().await.is_shutting_down
    }

    /// Get shutdown state information
    pub async fn get_shutdown_info(&self) -> ShutdownInfo {
        let state = self.shutdown_state.read().await;
        ShutdownInfo {
            is_shutting_down: state.is_shutting_down,
            shutdown_start_time: state.shutdown_start_time,
            signal_received: state.signal_received.clone(),
            grace_period: self.grace_period,
        }
    }

    /// Get remaining grace period
    pub async fn remaining_grace_period(&self) -> Option<Duration> {
        let state = self.shutdown_state.read().await;

        if let Some(start_time) = state.shutdown_start_time {
            let elapsed = chrono::Utc::now() - start_time;
            let elapsed_duration = elapsed.to_std().unwrap_or(Duration::MAX);

            if elapsed_duration < self.grace_period {
                Some(self.grace_period - elapsed_duration)
            } else {
                Some(Duration::ZERO)
            }
        } else {
            Some(self.grace_period)
        }
    }
}

/// Shutdown information for monitoring
#[derive(Debug, Clone)]
pub struct ShutdownInfo {
    pub is_shutting_down: bool,
    pub shutdown_start_time: Option<chrono::DateTime<chrono::Utc>>,
    pub signal_received: Option<ShutdownSignal>,
    pub grace_period: Duration,
}

impl ShutdownInfo {
    /// Get time elapsed since shutdown started
    pub fn elapsed_time(&self) -> Option<chrono::Duration> {
        self.shutdown_start_time
            .map(|start| chrono::Utc::now() - start)
    }

    /// Get remaining grace period
    pub fn remaining_time(&self) -> Option<Duration> {
        if let Some(elapsed) = self.elapsed_time() {
            let elapsed_duration = elapsed.to_std().unwrap_or(Duration::MAX);
            if elapsed_duration < self.grace_period {
                Some(self.grace_period - elapsed_duration)
            } else {
                Some(Duration::ZERO)
            }
        } else {
            Some(self.grace_period)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::sleep;

    #[tokio::test]
    async fn test_shutdown_signal_handling() {
        let handler = ShutdownHandler::new(Duration::from_secs(5));
        let mut receiver = handler.start().await;

        // Trigger manual shutdown
        handler
            .trigger_shutdown(ShutdownSignal::Manual)
            .await
            .unwrap();

        // Should receive the signal
        let received = receiver.recv().await.unwrap();
        assert!(matches!(received, ShutdownSignal::Manual));

        // Check shutdown state
        assert!(handler.is_shutting_down().await);

        let info = handler.get_shutdown_info().await;
        assert!(info.is_shutting_down);
        assert!(matches!(info.signal_received, Some(ShutdownSignal::Manual)));
    }

    #[tokio::test]
    async fn test_grace_period_calculation() {
        let handler = ShutdownHandler::new(Duration::from_secs(10));

        // Before shutdown
        assert_eq!(
            handler.remaining_grace_period().await,
            Some(Duration::from_secs(10))
        );

        // Trigger shutdown
        handler
            .trigger_shutdown(ShutdownSignal::Manual)
            .await
            .unwrap();

        // Should have full grace period initially
        let remaining = handler.remaining_grace_period().await;
        assert!(remaining.is_some());
        assert!(remaining.unwrap() <= Duration::from_secs(10));

        // Wait a bit and check again
        sleep(Duration::from_millis(100)).await;
        let remaining = handler.remaining_grace_period().await;
        assert!(remaining.is_some());
        assert!(remaining.unwrap() < Duration::from_secs(10));
    }
}
