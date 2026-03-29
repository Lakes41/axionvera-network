use metrics::{counter, gauge, histogram};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::Instant;

/// Metrics collector for the network node
pub struct MetricsCollector {
    start_time: Instant,
    total_requests: AtomicU64,
    active_connections: AtomicUsize,
    total_errors: AtomicU64,
    bytes_sent: AtomicU64,
    bytes_received: AtomicU64,
    pending_transactions: AtomicU64,
}

impl MetricsCollector {
    /// Create a new metrics collector
    pub fn new() -> Self {
        Self {
            start_time: Instant::now(),
            total_requests: AtomicU64::new(0),
            active_connections: AtomicUsize::new(0),
            total_errors: AtomicU64::new(0),
            bytes_sent: AtomicU64::new(0),
            bytes_received: AtomicU64::new(0),
            pending_transactions: AtomicU64::new(0),
        }
    }

    /// Increment total requests counter
    pub fn increment_requests(&self) {
        self.total_requests.fetch_add(1, Ordering::Relaxed);
        counter!("http_requests_total").increment(1);
    }

    /// Set active connections gauge
    pub fn set_active_connections(&self, count: usize) {
        self.active_connections.store(count, Ordering::Relaxed);
        gauge!("active_connections").set(count as f64);
    }

    /// Increment error counter
    pub fn increment_errors(&self) {
        self.total_errors.fetch_add(1, Ordering::Relaxed);
        counter!("errors_total").increment(1);
    }

    /// Add to bytes sent
    pub fn add_bytes_sent(&self, bytes: u64) {
        self.bytes_sent.fetch_add(bytes, Ordering::Relaxed);
        counter!("bytes_sent_total").increment(bytes);
    }

    /// Add to bytes received
    pub fn add_bytes_received(&self, bytes: u64) {
        self.bytes_received.fetch_add(bytes, Ordering::Relaxed);
        counter!("bytes_received_total").increment(bytes);
    }

    /// Record request duration
    pub fn record_request_duration(&self, duration_secs: f64) {
        histogram!("request_duration_seconds").record(duration_secs);
    }

    /// Set pending transactions gauge
    pub fn set_pending_transactions(&self, count: u64) {
        self.pending_transactions.store(count, Ordering::Relaxed);
        gauge!("axionvera_pending_transactions_total").set(count as f64);
        gauge!("axionvera_transaction_queue_depth").set(count as f64);
    }

    /// Get pending transactions
    pub fn get_pending_transactions(&self) -> u64 {
        self.pending_transactions.load(Ordering::Relaxed)
    }

    /// Get uptime in seconds
    pub fn uptime_secs(&self) -> u64 {
        self.start_time.elapsed().as_secs()
    }

    /// Get total requests
    pub fn get_total_requests(&self) -> u64 {
        self.total_requests.load(Ordering::Relaxed)
    }

    /// Get active connections
    pub fn get_active_connections(&self) -> usize {
        self.active_connections.load(Ordering::Relaxed)
    }

    /// Get total errors
    pub fn get_total_errors(&self) -> u64 {
        self.total_errors.load(Ordering::Relaxed)
    }

    /// Get bytes sent
    pub fn get_bytes_sent(&self) -> u64 {
        self.bytes_sent.load(Ordering::Relaxed)
    }

    /// Get bytes received
    pub fn get_bytes_received(&self) -> u64 {
        self.bytes_received.load(Ordering::Relaxed)
    }

    /// Get pending transactions
    pub fn get_pending_transactions(&self) -> u64 {
        self.pending_transactions.load(Ordering::Relaxed)
    }

    /// Get all metrics as Prometheus format string
    pub fn get_prometheus_metrics(&self) -> String {
        format!(
            r#"# HELP axionvera_uptime_seconds Node uptime in seconds
# TYPE axionvera_uptime_seconds counter
axionvera_uptime_seconds {}

# HELP axionvera_http_requests_total Total number of HTTP requests
# TYPE axionvera_http_requests_total counter
axionvera_http_requests_total {}

# HELP axionvera_active_connections Current number of active connections
# TYPE axionvera_active_connections gauge
axionvera_active_connections {}

# HELP axionvera_errors_total Total number of errors
# TYPE axionvera_errors_total counter
axionvera_errors_total {}

# HELP axionvera_bytes_sent_total Total bytes sent
# TYPE axionvera_bytes_sent_total counter
axionvera_bytes_sent_total {}

# HELP axionvera_bytes_received_total Total bytes received
# TYPE axionvera_bytes_received_total counter
axionvera_bytes_received_total {}

# HELP axionvera_pending_transactions_total Number of pending transactions in queue
# TYPE axionvera_pending_transactions_total gauge
axionvera_pending_transactions_total {}

# HELP axionvera_transaction_queue_depth Current transaction queue depth
# TYPE axionvera_transaction_queue_depth gauge
axionvera_transaction_queue_depth {}

# HELP process_memory_bytes Current memory usage in bytes
# TYPE process_memory_bytes gauge
process_memory_bytes {}

# HELP request_duration_seconds Request duration histogram
# TYPE request_duration_seconds histogram
"#,
            self.uptime_secs(),
            self.get_total_requests(),
            self.get_active_connections(),
            self.get_total_errors(),
            self.get_bytes_sent(),
            self.get_bytes_received(),
            self.get_pending_transactions(),
            self.get_pending_transactions(),
            self.get_process_memory(),
        )
    }

    /// Get process memory usage in bytes
    fn get_process_memory(&self) -> u64 {
        // Simple implementation - in production, use sysinfo crate for accurate memory stats
        // For now, return a placeholder value
        0
    }
}

impl Default for MetricsCollector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_collector() {
        let collector = MetricsCollector::new();

        assert_eq!(collector.get_total_requests(), 0);
        assert_eq!(collector.get_active_connections(), 0);
        assert_eq!(collector.get_total_errors(), 0);

        collector.increment_requests();
        collector.set_active_connections(5);
        collector.increment_errors();

        assert_eq!(collector.get_total_requests(), 1);
        assert_eq!(collector.get_active_connections(), 5);
        assert_eq!(collector.get_total_errors(), 1);
    }
}
