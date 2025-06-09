use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

#[derive(Debug)]
pub struct Metrics {
    avg_response_time: AtomicU64,
    connection_count: AtomicU64,
}

impl Default for Metrics {
    fn default() -> Self {
        Self::new()
    }
}

impl Metrics {
    fn new() -> Self {
        Self {
            avg_response_time: AtomicU64::new(0),
            connection_count: AtomicU64::new(0),
        }
    }

    pub fn get_connection_count(&self) -> u64 {
        self.connection_count.load(Ordering::Relaxed)
    }
    pub fn get_avg_response_time(&self) -> u64 {
        self.avg_response_time.load(Ordering::Relaxed)
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
    pub fn record_connection(&self) {
        self.connection_count.fetch_add(1, Ordering::Relaxed);
    }
    pub fn release_connection(&self) {
        self.connection_count.fetch_sub(1, Ordering::Relaxed);
    }
}
