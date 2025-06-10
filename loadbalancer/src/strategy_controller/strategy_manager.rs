use crate::LoadBalanceStrategy;
use crate::lb_config::StrategyConfig;
use crate::metrics::Metrics;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub struct StrategyManager {
    current_strategy: Arc<Mutex<LoadBalanceStrategy>>,
    metrics: Arc<Metrics>,
    last_switch: Arc<Mutex<Instant>>,
    config: Arc<StrategyConfig>,
}
impl StrategyManager {
    pub(crate) fn new(config: Arc<StrategyConfig>, metrics: Arc<Metrics>) -> Self {
        let current_strategy = Arc::new(Mutex::new(config.default_strategy.clone()));
        Self {
            current_strategy,
            metrics,
            last_switch: Arc::new(Mutex::new(Instant::now())),
            config,
        }
    }

    pub fn get_current_strategy(&self) -> LoadBalanceStrategy {
        self.current_strategy.lock().unwrap().clone()
    }
    fn should_not_switch(&self) -> bool {
        self.last_switch
            .lock()
            .unwrap()
            .elapsed()
            .lt(&self.config.switch_interval)
    }
    pub fn switch_strategy(&self) {
        if self.should_not_switch() {
            return; // Too soon to switch
        }
        let curr_avg_response_time =
            Duration::from_nanos(self.metrics.avg_response_time.load(Ordering::Acquire));
        if curr_avg_response_time.is_zero() {
            return; //No data yet
        }

        let threshold = &self.config.response_time_threshold;
        let mut curr_strategy = self.current_strategy.lock().unwrap();
        let should_switch = match *curr_strategy {
            LoadBalanceStrategy::RoundRobin if curr_avg_response_time.gt(threshold) => {
                Some(LoadBalanceStrategy::LeastAvgResponseTime)
            }
            LoadBalanceStrategy::LeastAvgResponseTime if curr_avg_response_time.lt(threshold) => {
                Some(LoadBalanceStrategy::RoundRobin)
            }
            _ => None,
        };
        if let Some(new_strategy) = should_switch {
            println!(
                "Strategy switched to {:?} (avg response time: {:?})",
                &new_strategy, curr_avg_response_time
            );
            *curr_strategy = new_strategy;
            *self.last_switch.lock().unwrap() = Instant::now();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::LoadBalanceStrategy;
    use crate::lb_config::StrategyConfig;
    use crate::metrics::Metrics;
    use std::sync::Arc;
    use std::time::Duration;

    fn create_test_config() -> Arc<StrategyConfig> {
        Arc::new(StrategyConfig {
            default_strategy: LoadBalanceStrategy::RoundRobin,
            switch_interval: Duration::from_secs(5),
            response_time_threshold: Duration::from_millis(100),
        })
    }

    fn create_test_metrics() -> Arc<Metrics> {
        Arc::new(Metrics::default())
    }

    #[test]
    fn test_new_strategy_manager() {
        let config = create_test_config();
        let metrics = create_test_metrics();

        let manager = StrategyManager::new(config.clone(), metrics);

        assert_eq!(
            manager.get_current_strategy(),
            LoadBalanceStrategy::RoundRobin
        );
    }

    #[test]
    fn test_get_current_strategy() {
        let config = create_test_config();
        let metrics = create_test_metrics();
        let manager = StrategyManager::new(config, metrics);

        let strategy = manager.get_current_strategy();

        assert_eq!(strategy, LoadBalanceStrategy::RoundRobin);
    }

    #[test]
    fn test_should_not_switch_immediately_after_creation() {
        let config = create_test_config();
        let metrics = create_test_metrics();
        let manager = StrategyManager::new(config, metrics);

        assert!(manager.should_not_switch());
    }

    #[test]
    fn test_should_not_switch_within_interval() {
        let config = StrategyConfig {
            default_strategy: LoadBalanceStrategy::RoundRobin,
            switch_interval: Duration::from_secs(10), // Long interval
            response_time_threshold: Duration::from_millis(100),
        };
        let config = Arc::new(config);
        let metrics = create_test_metrics();
        let manager = StrategyManager::new(config, metrics);

        assert!(manager.should_not_switch());
    }

    #[test]
    fn test_switch_strategy_no_data() {
        let config = create_test_config();
        let metrics = create_test_metrics();
        // Set avg_response_time to 0 (no data)
        metrics.avg_response_time.store(0, Ordering::Release);

        let manager = StrategyManager::new(config, metrics);
        // Force last_switch to be old enough
        *manager.last_switch.lock().unwrap() = Instant::now() - Duration::from_secs(10);

        manager.switch_strategy();

        // Should remain RoundRobin due to no data
        assert_eq!(
            manager.get_current_strategy(),
            LoadBalanceStrategy::RoundRobin
        );
    }

    #[test]
    fn test_switch_from_round_robin_to_least_avg_high_response_time() {
        let config = create_test_config();
        let metrics = create_test_metrics();

        // Set a high response time (above a threshold of 100 ms)
        let high_response_time = Duration::from_millis(200);
        metrics
            .avg_response_time
            .store(high_response_time.as_nanos() as u64, Ordering::Release);

        let manager = StrategyManager::new(config, metrics);
        // Force last_switch to be old enough
        *manager.last_switch.lock().unwrap() = Instant::now() - Duration::from_secs(10);

        manager.switch_strategy();

        assert_eq!(
            manager.get_current_strategy(),
            LoadBalanceStrategy::LeastAvgResponseTime
        );
    }

    #[test]
    fn test_switch_from_least_avg_to_round_robin_low_response_time() {
        let config = StrategyConfig {
            default_strategy: LoadBalanceStrategy::LeastAvgResponseTime,
            switch_interval: Duration::from_secs(5),
            response_time_threshold: Duration::from_millis(100),
        };
        let config = Arc::new(config);
        let metrics = create_test_metrics();

        // Set a low response time (below a threshold of 100 ms)
        let low_response_time = Duration::from_millis(50);
        metrics
            .avg_response_time
            .store(low_response_time.as_nanos() as u64, Ordering::Release);

        let manager = StrategyManager::new(config, metrics);
        // Force last_switch to be old enough
        *manager.last_switch.lock().unwrap() = Instant::now() - Duration::from_secs(10);

        manager.switch_strategy();

        assert_eq!(
            manager.get_current_strategy(),
            LoadBalanceStrategy::RoundRobin
        );
    }

    #[test]
    fn test_no_switch_round_robin_low_response_time() {
        let config = create_test_config();
        let metrics = create_test_metrics();

        // Set low response time (below threshold)
        let low_response_time = Duration::from_millis(50);
        metrics
            .avg_response_time
            .store(low_response_time.as_nanos() as u64, Ordering::Release);

        let manager = StrategyManager::new(config, metrics);
        // Force last_switch to be old enough
        *manager.last_switch.lock().unwrap() = Instant::now() - Duration::from_secs(10);

        manager.switch_strategy();

        // Should remain RoundRobin
        assert_eq!(
            manager.get_current_strategy(),
            LoadBalanceStrategy::RoundRobin
        );
    }

    #[test]
    fn test_no_switch_least_avg_high_response_time() {
        let config = StrategyConfig {
            default_strategy: LoadBalanceStrategy::LeastAvgResponseTime,
            switch_interval: Duration::from_secs(5),
            response_time_threshold: Duration::from_millis(100),
        };
        let config = Arc::new(config);
        let metrics = create_test_metrics();

        // Set high response time (above threshold)
        let high_response_time = Duration::from_millis(200);
        metrics
            .avg_response_time
            .store(high_response_time.as_nanos() as u64, Ordering::Release);

        let manager = StrategyManager::new(config, metrics);
        // Force last_switch to be old enough
        *manager.last_switch.lock().unwrap() = Instant::now() - Duration::from_secs(10);

        manager.switch_strategy();

        // Should remain LeastAvgResponseTime
        assert_eq!(
            manager.get_current_strategy(),
            LoadBalanceStrategy::LeastAvgResponseTime
        );
    }

    #[test]
    fn test_switch_strategy_too_soon() {
        let config = create_test_config();
        let metrics = create_test_metrics();

        // Set high response time to trigger switch
        let high_response_time = Duration::from_millis(200);
        metrics
            .avg_response_time
            .store(high_response_time.as_nanos() as u64, Ordering::Release);

        let manager = StrategyManager::new(config, metrics);
        // Don't modify last_switch, so it's too soon to switch

        manager.switch_strategy();

        // Should remain RoundRobin due to timing constraint
        assert_eq!(
            manager.get_current_strategy(),
            LoadBalanceStrategy::RoundRobin
        );
    }

    #[test]
    fn test_response_time_threshold_boundary() {
        let config = create_test_config();
        let metrics = create_test_metrics();

        // Set response time exactly at a threshold (100 ms)
        let threshold_response_time = Duration::from_millis(100);
        metrics
            .avg_response_time
            .store(threshold_response_time.as_nanos() as u64, Ordering::Release);

        let manager = StrategyManager::new(config, metrics);
        // Force last_switch to be old enough
        *manager.last_switch.lock().unwrap() = Instant::now() - Duration::from_secs(10);

        manager.switch_strategy();

        // Should remain RoundRobin (threshold is not exceeded)
        assert_eq!(
            manager.get_current_strategy(),
            LoadBalanceStrategy::RoundRobin
        );
    }

    #[test]
    fn test_multiple_switches() {
        let config = create_test_config();
        let metrics = create_test_metrics();
        let manager = StrategyManager::new(config, metrics.clone());

        // First switch: high response time
        let high_response_time = Duration::from_millis(200);
        metrics
            .avg_response_time
            .store(high_response_time.as_nanos() as u64, Ordering::Release);

        *manager.last_switch.lock().unwrap() = Instant::now() - Duration::from_secs(10);

        manager.switch_strategy();
        assert_eq!(
            manager.get_current_strategy(),
            LoadBalanceStrategy::LeastAvgResponseTime
        );

        // Second switch: low response time
        let low_response_time = Duration::from_millis(30);
        metrics
            .avg_response_time
            .store(low_response_time.as_nanos() as u64, Ordering::Release);
        *manager.last_switch.lock().unwrap() = Instant::now() - Duration::from_secs(10);

        manager.switch_strategy();
        assert_eq!(
            manager.get_current_strategy(),
            LoadBalanceStrategy::RoundRobin
        );
    }
}
