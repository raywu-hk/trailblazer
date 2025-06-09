use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct Metrics {
    pub avg_response_time: Arc<AtomicU64>,
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
            avg_response_time: Arc::new(AtomicU64::new(0)),
            request_count: Arc::new(AtomicU64::new(0)),
            error_count: Arc::new(AtomicU64::new(0)),
            active_connections: Arc::new(AtomicUsize::new(0)),
        }
    }

    pub fn record_response_time(&self, duration: Duration) {
        // Use exponential moving average instead of simple average
        let duration_nanos = duration.as_nanos().min(u64::MAX as u128) as u64;

        // Use compare-and-swap loop for atomic update
        loop {
            let current_avg = self.avg_response_time.load(Ordering::Acquire);
            let new_avg = if current_avg == 0 {
                duration_nanos
            } else {
                // Exponential moving average with alpha = 0.1
                let alpha = 0.1;
                ((1.0 - alpha) * current_avg as f64 + alpha * duration_nanos as f64) as u64
            };

            match self.avg_response_time.compare_exchange_weak(
                current_avg,
                new_avg,
                Ordering::Release,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(_) => continue, // Retry if another thread modified the value
            }
        }
    }
}
