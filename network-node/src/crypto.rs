use crate::error::{NetworkError, Result};
use ed25519_dalek::{PublicKey, Signature, Verifier};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock, Semaphore};
use tracing::{debug, error, info, instrument, warn, Span};
use uuid::Uuid;

/// Signature verification request
#[derive(Debug, Clone)]
pub struct SignatureVerificationRequest {
    pub id: String,
    pub public_key: Vec<u8>,
    pub message: Vec<u8>,
    pub signature: Vec<u8>,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

impl SignatureVerificationRequest {
    pub fn new(public_key: Vec<u8>, message: Vec<u8>, signature: Vec<u8>) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            public_key,
            message,
            signature,
            timestamp: chrono::Utc::now(),
        }
    }

    pub fn message_hash(&self) -> String {
        let mut hasher = Sha256::new();
        hasher.update(&self.message);
        format!("{:x}", hasher.finalize())
    }
}

/// Signature verification result
#[derive(Debug, Clone)]
pub struct SignatureVerificationResult {
    pub request_id: String,
    pub is_valid: bool,
    pub error: Option<String>,
    pub verification_time_ms: u64,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Batch signature verification result
#[derive(Debug, Clone)]
pub struct BatchVerificationResult {
    pub batch_id: String,
    pub total_requests: usize,
    pub valid_signatures: usize,
    pub invalid_signatures: usize,
    pub failed_verifications: Vec<String>,
    pub total_time_ms: u64,
    pub results: Vec<SignatureVerificationResult>,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Cryptographic operation worker pool
pub struct CryptoWorkerPool {
    workers: Vec<tokio::task::JoinHandle<()>>,
    work_sender: mpsc::UnboundedSender<SignatureVerificationRequest>,
    result_sender: mpsc::UnboundedSender<BatchVerificationResult>,
    batch_size: usize,
    batch_timeout_ms: u64,
    pending_requests: Arc<RwLock<HashMap<String, SignatureVerificationRequest>>>,
    semaphore: Arc<Semaphore>,
}

impl CryptoWorkerPool {
    /// Create a new cryptographic worker pool
    pub fn new(
        worker_count: usize,
        batch_size: usize,
        batch_timeout_ms: u64,
    ) -> (Self, mpsc::UnboundedReceiver<BatchVerificationResult>) {
        let (work_sender, work_receiver) = mpsc::unbounded_channel();
        let (result_sender, result_receiver) = mpsc::unbounded_channel();
        
        let pending_requests = Arc::new(RwLock::new(HashMap::new()));
        let semaphore = Arc::new(Semaphore::new(worker_count));

        let mut workers = Vec::new();

        // Create worker threads
        for i in 0..worker_count {
            let work_receiver = work_receiver.clone();
            let result_sender = result_sender.clone();
            let pending_requests = pending_requests.clone();
            let semaphore = semaphore.clone();

            let worker = tokio::spawn(async move {
                Self::worker_loop(i, work_receiver, result_sender, pending_requests, semaphore).await;
            });
            
            workers.push(worker);
        }

        // Create batch processor
        let batch_processor = tokio::spawn({
            let work_receiver = work_receiver.clone();
            let result_sender = result_sender.clone();
            let pending_requests = pending_requests.clone();
            let batch_size = batch_size;
            let batch_timeout_ms = batch_timeout_ms;

            async move {
                Self::batch_processor_loop(
                    work_receiver,
                    result_sender,
                    pending_requests,
                    batch_size,
                    batch_timeout_ms,
                ).await;
            }
        });

        workers.push(batch_processor);

        Self {
            workers,
            work_sender,
            result_sender,
            batch_size,
            batch_timeout_ms,
            pending_requests,
            semaphore,
        },
        result_receiver
    }

    /// Worker thread loop for processing individual signature verifications
    #[instrument(skip(work_receiver, result_sender, pending_requests, semaphore), fields(worker_id))]
    async fn worker_loop(
        worker_id: usize,
        mut work_receiver: mpsc::UnboundedReceiver<SignatureVerificationRequest>,
        result_sender: mpsc::UnboundedSender<BatchVerificationResult>,
        pending_requests: Arc<RwLock<HashMap<String, SignatureVerificationRequest>>>,
        semaphore: Arc<Semaphore>,
    ) {
        tracing::Span::current().record("worker_id", worker_id);
        
        info!("Crypto worker {} started", worker_id);

        while let Some(request) = work_receiver.recv().await {
            let _permit = semaphore.acquire().await.unwrap();
            
            let span = tracing::info_span!(
                "signature_verification",
                worker_id = worker_id,
                request_id = %request.id,
                message_hash = %request.message_hash()
            );
            let _enter = span.enter();

            debug!("Worker {} processing signature verification request: {}", worker_id, request.id);

            let start_time = std::time::Instant::now();
            let result = Self::verify_signature(&request).await;
            let verification_time = start_time.elapsed().as_millis() as u64;

            let verification_result = SignatureVerificationResult {
                request_id: request.id.clone(),
                is_valid: result.is_ok(),
                error: result.err().map(|e| e.to_string()),
                verification_time_ms: verification_time,
                timestamp: chrono::Utc::now(),
            };

            // Store the result in pending requests
            {
                let mut pending = pending_requests.write().await;
                if let Some(mut req) = pending.remove(&request.id) {
                    // In a real implementation, we would handle the result differently
                    // For now, we just log it
                    if verification_result.is_valid {
                        debug!("Signature verification successful for request: {}", request.id);
                    } else {
                        warn!("Signature verification failed for request: {}", request.id);
                    }
                }
            }

            debug!("Worker {} completed verification in {}ms", worker_id, verification_time);
        }

        info!("Crypto worker {} stopped", worker_id);
    }

    /// Batch processor loop for aggregating verification requests
    #[instrument(skip(work_receiver, result_sender, pending_requests), fields(batch_size, batch_timeout_ms))]
    async fn batch_processor_loop(
        mut work_receiver: mpsc::UnboundedReceiver<SignatureVerificationRequest>,
        result_sender: mpsc::UnboundedSender<BatchVerificationResult>,
        pending_requests: Arc<RwLock<HashMap<String, SignatureVerificationRequest>>>,
        batch_size: usize,
        batch_timeout_ms: u64,
    ) {
        tracing::Span::current().record("batch_size", batch_size);
        tracing::Span::current().record("batch_timeout_ms", batch_timeout_ms);

        info!("Batch processor started with batch size: {}, timeout: {}ms", batch_size, batch_timeout_ms);

        let mut batch = Vec::new();
        let mut last_batch_time = std::time::Instant::now();

        loop {
            let timeout_duration = std::time::Duration::from_millis(batch_timeout_ms);
            
            tokio::select! {
                Some(request) = work_receiver.recv() => {
                    batch.push(request);
                    
                    if batch.len() >= batch_size {
                        Self::process_batch(&batch, &result_sender).await;
                        batch.clear();
                        last_batch_time = std::time::Instant::now();
                    }
                }
                _ = tokio::time::sleep(timeout_duration) => {
                    if !batch.is_empty() && last_batch_time.elapsed() >= timeout_duration {
                        info!("Batch timeout reached, processing {} pending requests", batch.len());
                        Self::process_batch(&batch, &result_sender).await;
                        batch.clear();
                        last_batch_time = std::time::Instant::now();
                    }
                }
            }
        }
    }

    /// Process a batch of signature verification requests
    #[instrument(skip(batch, result_sender), fields(batch_id, batch_size = batch.len()))]
    async fn process_batch(
        batch: &[SignatureVerificationRequest],
        result_sender: &mpsc::UnboundedSender<BatchVerificationResult>,
    ) {
        let batch_id = Uuid::new_v4().to_string();
        tracing::Span::current().record("batch_id", &batch_id);

        let start_time = std::time::Instant::now();
        info!("Processing batch {} with {} requests", batch_id, batch.len());

        let mut results = Vec::new();
        let mut valid_count = 0;
        let mut invalid_count = 0;
        let mut failed_verifications = Vec::new();

        for request in batch {
            let verification_start = std::time::Instant::now();
            let verification_result = Self::verify_signature(request).await;
            let verification_time = verification_start.elapsed().as_millis() as u64;

            let result = SignatureVerificationResult {
                request_id: request.id.clone(),
                is_valid: verification_result.is_ok(),
                error: verification_result.err().map(|e| e.to_string()),
                verification_time_ms: verification_time,
                timestamp: chrono::Utc::now(),
            };

            if result.is_valid {
                valid_count += 1;
            } else {
                invalid_count += 1;
                failed_verifications.push(request.id.clone());
            }

            results.push(result);
        }

        let total_time = start_time.elapsed().as_millis() as u64;

        let batch_result = BatchVerificationResult {
            batch_id,
            total_requests: batch.len(),
            valid_signatures: valid_count,
            invalid_signatures: invalid_count,
            failed_verifications,
            total_time_ms: total_time,
            results,
            timestamp: chrono::Utc::now(),
        };

        info!(
            "Batch {} completed: {}/{} valid signatures in {}ms",
            batch_result.batch_id,
            batch_result.valid_signatures,
            batch_result.total_requests,
            batch_result.total_time_ms
        );

        if let Err(e) = result_sender.send(batch_result) {
            error!("Failed to send batch result: {}", e);
        }
    }

    /// Verify a single signature
    #[instrument(skip(request), fields(request_id = %request.id, message_hash = %request.message_hash()))]
    async fn verify_signature(request: &SignatureVerificationRequest) -> Result<()> {
        debug!("Verifying signature for request: {}", request.id);

        // Parse public key
        let public_key = PublicKey::from_bytes(&request.public_key)
            .map_err(|e| NetworkError::Crypto(format!("Invalid public key: {}", e)))?;

        // Parse signature
        let signature = Signature::from_bytes(&request.signature)
            .map_err(|e| NetworkError::Crypto(format!("Invalid signature: {}", e)))?;

        // Verify signature
        public_key
            .verify(&request.message, &signature)
            .map_err(|e| NetworkError::Crypto(format!("Signature verification failed: {}", e)))?;

        debug!("Signature verification successful for request: {}", request.id);
        Ok(())
    }

    /// Submit a signature verification request
    #[instrument(skip(self), fields(request_id))]
    pub async fn verify_signature_batch(
        &self,
        requests: Vec<SignatureVerificationRequest>,
    ) -> Result<()> {
        let batch_id = Uuid::new_v4().to_string();
        tracing::Span::current().record("batch_id", &batch_id);

        info!("Submitting batch {} with {} signature verification requests", batch_id, requests.len());

        for request in requests {
            let request_id = request.id.clone();
            
            // Store request in pending map
            {
                let mut pending = self.pending_requests.write().await;
                pending.insert(request_id.clone(), request.clone());
            }

            // Send to worker pool
            if let Err(e) = self.work_sender.send(request) {
                error!("Failed to send verification request to worker pool: {}", e);
                
                // Remove from pending map
                {
                    let mut pending = self.pending_requests.write().await;
                    pending.remove(&request_id);
                }
                
                return Err(NetworkError::Crypto(format!("Failed to queue verification request: {}", e)));
            }
        }

        debug!("Batch {} submitted successfully", batch_id);
        Ok(())
    }

    /// Get current worker pool statistics
    pub async fn get_stats(&self) -> CryptoWorkerPoolStats {
        let pending_count = self.pending_requests.read().await.len();
        let available_permits = self.semaphore.available_permits();

        CryptoWorkerPoolStats {
            worker_count: self.workers.len(),
            pending_requests: pending_count,
            available_workers: available_permits,
            batch_size: self.batch_size,
            batch_timeout_ms: self.batch_timeout_ms,
        }
    }
}

impl Drop for CryptoWorkerPool {
    fn drop(&mut self) {
        info!("Shutting down crypto worker pool");
        // Workers will be automatically dropped when the channels are closed
    }
}

/// Worker pool statistics
#[derive(Debug, Clone)]
pub struct CryptoWorkerPoolStats {
    pub worker_count: usize,
    pub pending_requests: usize,
    pub available_workers: usize,
    pub batch_size: usize,
    pub batch_timeout_ms: u64,
}

/// Global signature verification service
pub struct SignatureVerificationService {
    worker_pool: CryptoWorkerPool,
    result_receiver: mpsc::UnboundedReceiver<BatchVerificationResult>,
}

impl SignatureVerificationService {
    /// Create a new signature verification service
    pub fn new(worker_count: usize, batch_size: usize, batch_timeout_ms: u64) -> Self {
        let (worker_pool, result_receiver) = CryptoWorkerPool::new(worker_count, batch_size, batch_timeout_ms);
        
        Self {
            worker_pool,
            result_receiver,
        }
    }

    /// Verify a batch of signatures
    #[instrument(skip(self, requests), fields(batch_size = requests.len()))]
    pub async fn verify_batch(&mut self, requests: Vec<SignatureVerificationRequest>) -> Result<BatchVerificationResult> {
        info!("Starting batch verification of {} signatures", requests.len());

        // Submit batch to worker pool
        self.worker_pool.verify_signature_batch(requests).await?;

        // Wait for result
        match self.result_receiver.recv().await {
            Some(result) => {
                info!("Batch verification completed: {}/{} valid", result.valid_signatures, result.total_requests);
                Ok(result)
            }
            None => {
                error!("Failed to receive batch verification result");
                Err(NetworkError::Crypto("Failed to receive verification result".to_string()))
            }
        }
    }

    /// Get worker pool statistics
    pub async fn get_stats(&self) -> CryptoWorkerPoolStats {
        self.worker_pool.get_stats().await
    }

    /// Start background result processor
    pub async fn start_result_processor(&mut self) -> Result<tokio::task::JoinHandle<()>> {
        let mut result_receiver = self.result_receiver.clone();
        
        let handle = tokio::spawn(async move {
            while let Some(batch_result) = result_receiver.recv().await {
                info!(
                    "Processed batch {}: {}/{} signatures valid in {}ms",
                    batch_result.batch_id,
                    batch_result.valid_signatures,
                    batch_result.total_requests,
                    batch_result.total_time_ms
                );
                
                // In a real implementation, you might want to:
                // 1. Store results in a database
                // 2. Update metrics
                // 3. Notify waiting clients
                // 4. Handle failed verifications
            }
        });

        Ok(handle)
    }
}
