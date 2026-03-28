use std::collections::HashMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use tokio::sync::Mutex;
use crate::error::{NetworkError, Result};

// Optional redis backend
#[cfg(feature = "redis-backend")]
use redis::AsyncCommands;

/// Simple rate limiter supporting Redis (recommended) or in-memory fallback.
pub enum RateLimiterBackend {
    Redis(redis::aio::ConnectionManager),
    InMemory(Arc<Mutex<HashMap<String, (u64, i64)>>>), // (count, bucket_ts)
}

pub struct RateLimiter {
    backend: RateLimiterBackend,
    limit_per_minute: u64,
}

impl RateLimiter {
    /// Create a new RateLimiter. If `redis_url` is provided and connection succeeds, Redis backend is used.
    pub async fn new(redis_url: Option<String>, limit_per_minute: u64) -> Self {
        if let Some(url) = redis_url {
            match redis::Client::open(url.as_str()) {
                Ok(client) => match client.get_tokio_connection_manager().await {
                    Ok(manager) => {
                        return Self { backend: RateLimiterBackend::Redis(manager), limit_per_minute };
                    }
                    Err(_) => {
                        tracing::warn!("Failed to connect to Redis, falling back to in-memory rate limiter");
                    }
                },
                Err(_) => {
                    tracing::warn!("Invalid Redis URL, falling back to in-memory rate limiter");
                }
            }
        }

        Self {
            backend: RateLimiterBackend::InMemory(Arc::new(Mutex::new(HashMap::new()))),
            limit_per_minute,
        }
    }

    /// Check and increment the counter for a given key (e.g., client IP). Returns true if allowed.
    pub async fn allow(&self, key: &str) -> Result<bool> {
        // Use minute-bucket (epoch minutes)
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
        let bucket = (now.as_secs() / 60) as i64;

        match &self.backend {
            RateLimiterBackend::Redis(manager) => {
                // Build key
                let redis_key = format!("rate:{}:{}", key, bucket);
                // Try to INCR and set EXPIRE atomically
                let mut conn = manager.clone();
                let res: redis::RedisResult<u64> = async {
                    let mut c = conn;
                    let v: u64 = redis::cmd("INCR").arg(&redis_key).query_async(&mut c).await?;
                    // Set expiry of 70 seconds to cover the bucket
                    let _: () = redis::cmd("EXPIRE").arg(&redis_key).arg(70).query_async(&mut c).await?;
                    Ok(v)
                }.await;

                match res {
                    Ok(count) => Ok(count <= self.limit_per_minute),
                    Err(e) => Err(NetworkError::Internal(format!("Redis error: {}", e))),
                }
            }
            RateLimiterBackend::InMemory(map) => {
                let mut lock = map.lock().await;
                match lock.get_mut(key) {
                    Some((count, ts)) => {
                        if *ts == bucket {
                            *count += 1;
                        } else {
                            *ts = bucket;
                            *count = 1;
                        }
                        Ok(*count <= self.limit_per_minute)
                    }
                    None => {
                        lock.insert(key.to_string(), (1, bucket));
                        Ok(1 <= self.limit_per_minute)
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_inmemory_rate_limiter() {
        let rl = RateLimiter::new(None, 3).await;
        let key = "1.2.3.4";

        assert!(rl.allow(key).await.unwrap());
        assert!(rl.allow(key).await.unwrap());
        assert!(rl.allow(key).await.unwrap());
        assert!(!rl.allow(key).await.unwrap());
    }
}
