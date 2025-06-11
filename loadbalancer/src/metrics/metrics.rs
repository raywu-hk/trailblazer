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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn test_metrics_new() {
        let metrics = Metrics::new();

        assert_eq!(metrics.avg_response_time.load(Ordering::Relaxed), 0);
        assert_eq!(metrics.request_count.load(Ordering::Relaxed), 0);
        assert_eq!(metrics.error_count.load(Ordering::Relaxed), 0);
        assert_eq!(metrics.active_connections.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn test_metrics_default() {
        let metrics = Metrics::default();

        assert_eq!(metrics.avg_response_time.load(Ordering::Relaxed), 0);
        assert_eq!(metrics.request_count.load(Ordering::Relaxed), 0);
        assert_eq!(metrics.error_count.load(Ordering::Relaxed), 0);
        assert_eq!(metrics.active_connections.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn test_record_response_time_first_measurement() {
        let metrics = Metrics::new();
        let duration = Duration::from_millis(100);

        metrics.record_response_time(duration);

        let recorded_time = metrics.avg_response_time.load(Ordering::Relaxed);
        assert_eq!(recorded_time, duration.as_nanos() as u64);
    }

    #[test]
    fn test_record_response_time_exponential_moving_average() {
        let metrics = Metrics::new();

        // First measurement
        let first_duration = Duration::from_millis(100);
        metrics.record_response_time(first_duration);
        let first_avg = metrics.avg_response_time.load(Ordering::Relaxed);

        // Second measurement
        let second_duration = Duration::from_millis(200);
        metrics.record_response_time(second_duration);
        let second_avg = metrics.avg_response_time.load(Ordering::Relaxed);

        // Verify exponential moving average calculation
        let expected_avg =
            ((1.0 - 0.1) * first_avg as f64 + 0.1 * second_duration.as_nanos() as f64) as u64;
        assert_eq!(second_avg, expected_avg);

        // Ensure the average changed
        assert_ne!(first_avg, second_avg);
    }

    #[test]
    fn test_record_response_time_multiple_measurements() {
        let metrics = Metrics::new();
        let durations = vec![
            Duration::from_millis(50),
            Duration::from_millis(100),
            Duration::from_millis(150),
            Duration::from_millis(200),
        ];

        for duration in durations {
            metrics.record_response_time(duration);
        }

        // Verify that the average is non-zero and reasonable
        let final_avg = metrics.avg_response_time.load(Ordering::Relaxed);
        assert!(final_avg > 0);
        assert!(final_avg < Duration::from_millis(250).as_nanos() as u64);
    }

    #[test]
    fn test_record_response_time_zero_duration() {
        let metrics = Metrics::new();
        let duration = Duration::from_nanos(0);

        metrics.record_response_time(duration);

        assert_eq!(metrics.avg_response_time.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn test_record_response_time_very_large_duration() {
        let metrics = Metrics::new();
        let duration = Duration::from_secs(3600); // 1 hour

        metrics.record_response_time(duration);

        let recorded_time = metrics.avg_response_time.load(Ordering::Relaxed);
        assert_eq!(
            recorded_time,
            duration.as_nanos().min(u64::MAX as u128) as u64
        );
    }

    #[test]
    fn test_metrics_clone() {
        let metrics = Metrics::new();
        let duration = Duration::from_millis(100);
        metrics.record_response_time(duration);

        let cloned_metrics = metrics.clone();

        // Verify that cloned metrics share the same atomic values
        assert_eq!(
            metrics.avg_response_time.load(Ordering::Relaxed),
            cloned_metrics.avg_response_time.load(Ordering::Relaxed)
        );

        // Verify that updating one affects the other (shared Arc)
        let new_duration = Duration::from_millis(200);
        cloned_metrics.record_response_time(new_duration);

        assert_eq!(
            metrics.avg_response_time.load(Ordering::Relaxed),
            cloned_metrics.avg_response_time.load(Ordering::Relaxed)
        );
    }

    #[test]
    fn test_concurrent_response_time_recording() {
        let metrics = Arc::new(Metrics::new());
        let num_threads = 10;
        let measurements_per_thread = 100;

        let handles: Vec<_> = (0..num_threads)
            .map(|i| {
                let metrics_clone = Arc::clone(&metrics);
                thread::spawn(move || {
                    for j in 0..measurements_per_thread {
                        let duration = Duration::from_millis((i * 10 + j) as u64);
                        metrics_clone.record_response_time(duration);
                    }
                })
            })
            .collect();

        for handle in handles {
            handle.join().unwrap();
        }

        // Verify that the final average is reasonable and not corrupted
        let final_avg = metrics.avg_response_time.load(Ordering::Relaxed);
        assert!(final_avg > 0);
        assert!(final_avg < Duration::from_secs(1).as_nanos() as u64);
    }

    #[test]
    fn test_metrics_debug_format() {
        let metrics = Metrics::new();
        let debug_str = format!("{:?}", metrics);

        // Verify that Debug trait is implemented and produces reasonable output
        assert!(debug_str.contains("Metrics"));
        assert!(debug_str.contains("avg_response_time"));
        assert!(debug_str.contains("request_count"));
        assert!(debug_str.contains("error_count"));
        assert!(debug_str.contains("active_connections"));
    }

    #[test]
    fn test_atomic_field_operations() {
        let metrics = Metrics::new();

        // Test request_count operations
        metrics.request_count.fetch_add(5, Ordering::Relaxed);
        assert_eq!(metrics.request_count.load(Ordering::Relaxed), 5);

        // Test error_count operations
        metrics.error_count.fetch_add(2, Ordering::Relaxed);
        assert_eq!(metrics.error_count.load(Ordering::Relaxed), 2);

        // Test active_connections operations
        metrics.active_connections.fetch_add(10, Ordering::Relaxed);
        assert_eq!(metrics.active_connections.load(Ordering::Relaxed), 10);

        metrics.active_connections.fetch_sub(3, Ordering::Relaxed);
        assert_eq!(metrics.active_connections.load(Ordering::Relaxed), 7);
    }

    #[test]
    fn test_exponential_moving_average_calculation_precision() {
        let metrics = Metrics::new();

        // Test with known values to verify calculation precision
        let first_duration = Duration::from_nanos(1000);
        metrics.record_response_time(first_duration);

        let second_duration = Duration::from_nanos(2000);
        metrics.record_response_time(second_duration);

        let result = metrics.avg_response_time.load(Ordering::Relaxed);
        let expected = ((1.0 - 0.1) * 1000.0 + 0.1 * 2000.0) as u64;

        assert_eq!(result, expected);
        assert_eq!(result, 1100); // 900 + 200 = 1100
    }
}
