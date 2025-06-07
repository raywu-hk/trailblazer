use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize};
use std::time::Duration;
use tokio::sync::RwLock;

#[derive(Debug, Clone)]
pub struct Metrics {
    pub avg_response_time: Arc<RwLock<Duration>>,
    pub request_count: Arc<AtomicU64>,
    pub error_count: Arc<AtomicU64>,
    pub active_connections: Arc<AtomicUsize>,
}

impl Default for Metrics {
    fn default() -> Self {
        Self::new()
    }
}

impl Metrics {
    fn new() -> Self {
        Self {
            avg_response_time: Arc::new(RwLock::new(Duration::ZERO)),
            request_count: Arc::new(AtomicU64::new(0)),
            error_count: Arc::new(AtomicU64::new(0)),
            active_connections: Arc::new(AtomicUsize::new(0)),
        }
    }

    pub fn record_response_time(&self, duration: Duration) {
        // Use exponential moving average instead of simple average
        let avg_time = self.avg_response_time.clone();
        tokio::spawn(async move {
            let mut current_avg = avg_time.write().await;
            let new_avg = if current_avg.is_zero() {
                duration
            } else {
                // Exponential moving average with alpha = 0.1
                let alpha = 0.1;
                Duration::from_nanos(
                    ((1.0 - alpha) * current_avg.as_nanos() as f64
                        + alpha * duration.as_nanos() as f64) as u64,
                )
            };
            *current_avg = new_avg;
            println!("record_response_time avg: {:?}", new_avg);
        });
    }
}
