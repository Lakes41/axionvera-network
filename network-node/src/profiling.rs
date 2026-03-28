use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{debug, info, instrument, warn};
use uuid::Uuid;

/// CPU profiling metrics
#[derive(Debug, Clone)]
pub struct CpuMetrics {
    pub process_id: u32,
    pub cpu_usage_percent: f64,
    pub user_time_ms: u64,
    pub system_time_ms: u64,
    pub memory_usage_mb: f64,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Benchmark result
#[derive(Debug, Clone)]
pub struct BenchmarkResult {
    pub id: String,
    pub operation: String,
    pub duration_ms: u64,
    pub throughput_ops_per_sec: f64,
    pub cpu_metrics: CpuMetrics,
    pub success_count: u64,
    pub error_count: u64,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

impl BenchmarkResult {
    pub fn new(operation: String, duration_ms: u64, success_count: u64, error_count: u64) -> Self {
        let throughput = if duration_ms > 0 {
            (success_count as f64) / (duration_ms as f64 / 1000.0)
        } else {
            0.0
        };

        Self {
            id: Uuid::new_v4().to_string(),
            operation,
            duration_ms,
            throughput_ops_per_sec: throughput,
            cpu_metrics: CpuMetrics::default(),
            success_count,
            error_count,
            timestamp: chrono::Utc::now(),
        }
    }
}

impl Default for CpuMetrics {
    fn default() -> Self {
        Self {
            process_id: std::process::id(),
            cpu_usage_percent: 0.0,
            user_time_ms: 0,
            system_time_ms: 0,
            memory_usage_mb: 0.0,
            timestamp: chrono::Utc::now(),
        }
    }
}

/// Performance profiler
pub struct PerformanceProfiler {
    benchmarks: Arc<RwLock<HashMap<String, BenchmarkResult>>>,
    current_operations: Arc<RwLock<HashMap<String, Instant>>>,
}

impl PerformanceProfiler {
    pub fn new() -> Self {
        Self {
            benchmarks: Arc::new(RwLock::new(HashMap::new())),
            current_operations: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Start timing an operation
    #[instrument(skip(self), fields(operation_id))]
    pub async fn start_operation(&self, operation_id: &str) -> Result<(), crate::error::NetworkError> {
        let start_time = Instant::now();
        
        {
            let mut operations = self.current_operations.write().await;
            operations.insert(operation_id.to_string(), start_time);
        }

        debug!("Started profiling operation: {}", operation_id);
        Ok(())
    }

    /// End timing an operation and record the result
    #[instrument(skip(self), fields(operation_id, success_count, error_count))]
    pub async fn end_operation(
        &self,
        operation_id: &str,
        success_count: u64,
        error_count: u64,
    ) -> Result<BenchmarkResult, crate::error::NetworkError> {
        let end_time = Instant::now();
        
        let start_time = {
            let mut operations = self.current_operations.write().await;
            operations.remove(operation_id).ok_or_else(|| {
                crate::error::NetworkError::Validation(format!("Operation {} not found", operation_id))
            })?
        };

        let duration_ms = end_time.duration_since(start_time).as_millis() as u64;
        
        let result = BenchmarkResult::new(
            operation_id.to_string(),
            duration_ms,
            success_count,
            error_count,
        );

        {
            let mut benchmarks = self.benchmarks.write().await;
            benchmarks.insert(operation_id.to_string(), result.clone());
        }

        info!(
            "Operation {} completed: {}ms, {}/{} success/error, {:.2} ops/sec",
            operation_id,
            duration_ms,
            success_count,
            error_count,
            result.throughput_ops_per_sec
        );

        Ok(result)
    }

    /// Get benchmark results
    pub async fn get_benchmarks(&self) -> HashMap<String, BenchmarkResult> {
        self.benchmarks.read().await.clone()
    }

    /// Get benchmark result for a specific operation
    pub async fn get_benchmark(&self, operation_id: &str) -> Option<BenchmarkResult> {
        self.benchmarks.read().await.get(operation_id).cloned()
    }

    /// Clear all benchmarks
    pub async fn clear_benchmarks(&self) {
        let mut benchmarks = self.benchmarks.write().await;
        benchmarks.clear();
        info!("All benchmarks cleared");
    }

    /// Get performance summary
    pub async fn get_performance_summary(&self) -> PerformanceSummary {
        let benchmarks = self.benchmarks.read().await;
        
        let total_operations = benchmarks.len();
        let total_duration_ms: u64 = benchmarks.values().map(|b| b.duration_ms).sum();
        let total_successes: u64 = benchmarks.values().map(|b| b.success_count).sum();
        let total_errors: u64 = benchmarks.values().map(|b| b.error_count).sum();
        
        let avg_throughput = if total_duration_ms > 0 {
            (total_successes as f64) / (total_duration_ms as f64 / 1000.0)
        } else {
            0.0
        };

        PerformanceSummary {
            total_operations,
            total_duration_ms,
            total_successes,
            total_errors,
            average_throughput_ops_per_sec: avg_throughput,
            error_rate_percent: if total_successes + total_errors > 0 {
                (total_errors as f64) / ((total_successes + total_errors) as f64) * 100.0
            } else {
                0.0
            },
        }
    }
}

/// Performance summary
#[derive(Debug, Clone)]
pub struct PerformanceSummary {
    pub total_operations: usize,
    pub total_duration_ms: u64,
    pub total_successes: u64,
    pub total_errors: u64,
    pub average_throughput_ops_per_sec: f64,
    pub error_rate_percent: f64,
}

impl Default for PerformanceProfiler {
    fn default() -> Self {
        Self::new()
    }
}

/// Macro for easy benchmarking
#[macro_export]
macro_rules! benchmark_operation {
    ($profiler:expr, $operation_name:expr, $async_block:block) => {{
        let operation_id = format!("{}-{}", $operation_name, uuid::Uuid::new_v4());
        $profiler.start_operation(&operation_id).await?;
        
        let start_time = std::time::Instant::now();
        let result = $async_block;
        let duration = start_time.elapsed();
        
        let (success_count, error_count) = match result {
            Ok(count) => (count, 0),
            Err(_) => (0, 1),
        };
        
        let benchmark_result = $profiler.end_operation(&operation_id, success_count, error_count).await?;
        Ok((result, benchmark_result))
    }};
}

/// Signature verification benchmark
pub async fn benchmark_signature_verification(
    profiler: &PerformanceProfiler,
    requests: Vec<crate::crypto::SignatureVerificationRequest>,
) -> Result<Vec<BenchmarkResult>, crate::error::NetworkError> {
    let operation_id = format!("signature-verification-batch-{}", Uuid::new_v4());
    profiler.start_operation(&operation_id).await?;

    let start_time = Instant::now();
    let mut success_count = 0;
    let mut error_count = 0;

    // Simulate signature verification
    for request in requests {
        // In a real implementation, this would verify the actual signature
        let verification_result = simulate_signature_verification(&request).await;
        match verification_result {
            Ok(_) => success_count += 1,
            Err(_) => error_count += 1,
        }
    }

    let duration = start_time.elapsed();
    let result = profiler.end_operation(&operation_id, success_count, error_count).await?;

    info!(
        "Signature verification benchmark: {} signatures in {:?}, {}/{} success/error",
        success_count + error_count,
        duration,
        success_count,
        error_count
    );

    Ok(vec![result])
}

/// Database operation benchmark
pub async fn benchmark_database_operations(
    profiler: &PerformanceProfiler,
    pool: &crate::database::ConnectionPool,
    query_count: usize,
) -> Result<Vec<BenchmarkResult>, crate::error::NetworkError> {
    let operation_id = format!("database-operations-{}", Uuid::new_v4());
    profiler.start_operation(&operation_id).await?;

    let start_time = Instant::now();
    let mut success_count = 0;
    let mut error_count = 0;

    for i in 0..query_count {
        let query = format!("SELECT {} FROM test_table", i);
        match pool.get_connection().await {
            Ok(conn) => {
                match conn.execute_query(&query).await {
                    Ok(_) => success_count += 1,
                    Err(_) => error_count += 1,
                }
            }
            Err(_) => error_count += 1,
        }
    }

    let duration = start_time.elapsed();
    let result = profiler.end_operation(&operation_id, success_count, error_count).await?;

    info!(
        "Database operations benchmark: {} queries in {:?}, {}/{} success/error",
        query_count,
        duration,
        success_count,
        error_count
    );

    Ok(vec![result])
}

/// P2P message broadcast benchmark
pub async fn benchmark_p2p_broadcast(
    profiler: &PerformanceProfiler,
    p2p_manager: &crate::p2p::P2PManager,
    message_count: usize,
) -> Result<Vec<BenchmarkResult>, crate::error::NetworkError> {
    let operation_id = format!("p2p-broadcast-{}", Uuid::new_v4());
    profiler.start_operation(&operation_id).await?;

    let start_time = Instant::now();
    let mut success_count = 0;
    let mut error_count = 0;

    for i in 0..message_count {
        let message_type = i as i32;
        let payload = format!("Test message {}", i).into_bytes();
        let target_peers = vec![]; // Broadcast to all peers

        match p2p_manager.broadcast_message(message_type, &payload, &target_peers, 300).await {
            Ok((recipients, _failed)) => {
                if recipients > 0 {
                    success_count += 1;
                } else {
                    error_count += 1;
                }
            }
            Err(_) => error_count += 1,
        }
    }

    let duration = start_time.elapsed();
    let result = profiler.end_operation(&operation_id, success_count, error_count).await?;

    info!(
        "P2P broadcast benchmark: {} messages in {:?}, {}/{} success/error",
        message_count,
        duration,
        success_count,
        error_count
    );

    Ok(vec![result])
}

/// Simulate signature verification (for benchmarking)
async fn simulate_signature_verification(
    _request: &crate::crypto::SignatureVerificationRequest,
) -> Result<(), crate::error::NetworkError> {
    // Simulate verification time
    tokio::time::sleep(Duration::from_micros(100)).await;
    
    // Simulate 95% success rate
    if fastrand::u64(0..100) < 95 {
        Ok(())
    } else {
        Err(crate::error::NetworkError::Crypto("Simulated verification failure".to_string()))
    }
}

/// Get current CPU metrics (simplified implementation)
pub async fn get_cpu_metrics() -> CpuMetrics {
    // In a real implementation, you would use system APIs to get actual CPU metrics
    // For now, we return simulated values
    CpuMetrics {
        process_id: std::process::id(),
        cpu_usage_percent: fastrand::f64() * 100.0,
        user_time_ms: fastrand::u64(0..10000),
        system_time_ms: fastrand::u64(0..5000),
        memory_usage_mb: fastrand::f64() * 1024.0,
        timestamp: chrono::Utc::now(),
    }
}

/// Compare performance before and after optimization
pub fn compare_performance(
    before: &PerformanceSummary,
    after: &PerformanceSummary,
) -> PerformanceComparison {
    let throughput_improvement = if before.average_throughput_ops_per_sec > 0.0 {
        ((after.average_throughput_ops_per_sec - before.average_throughput_ops_per_sec) 
            / before.average_throughput_ops_per_sec) * 100.0
    } else {
        0.0
    };

    let error_rate_change = after.error_rate_percent - before.error_rate_percent;

    PerformanceComparison {
        throughput_improvement_percent: throughput_improvement,
        error_rate_change_percent: error_rate_change,
        before_throughput: before.average_throughput_ops_per_sec,
        after_throughput: after.average_throughput_ops_per_sec,
        before_error_rate: before.error_rate_percent,
        after_error_rate: after.error_rate_percent,
    }
}

/// Performance comparison result
#[derive(Debug, Clone)]
pub struct PerformanceComparison {
    pub throughput_improvement_percent: f64,
    pub error_rate_change_percent: f64,
    pub before_throughput: f64,
    pub after_throughput: f64,
    pub before_error_rate: f64,
    pub after_error_rate: f64,
}

impl PerformanceComparison {
    pub fn summary(&self) -> String {
        format!(
            "Throughput: {:.2} -> {:.2} ops/sec ({:.1}% improvement), Error rate: {:.2}% -> {:.2}% ({:.1}% change)",
            self.before_throughput,
            self.after_throughput,
            self.throughput_improvement_percent,
            self.before_error_rate,
            self.after_error_rate,
            self.error_rate_change_percent
        )
    }
}
