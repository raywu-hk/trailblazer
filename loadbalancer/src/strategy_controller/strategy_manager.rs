use crate::LoadBalanceStrategy;
use crate::lb_config::StrategyConfig;
use crate::metrics::Metrics;
use std::ops::Sub;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

pub struct StrategyManager {
    current_strategy: Arc<RwLock<LoadBalanceStrategy>>,
    metrics: Arc<Metrics>,
    last_switch: Arc<RwLock<Instant>>,
    config: Arc<StrategyConfig>,
}
impl StrategyManager {
    pub(crate) fn new(config: Arc<StrategyConfig>, metrics: Arc<Metrics>) -> Self {
        let current_strategy = Arc::new(RwLock::new(config.default_strategy.clone()));
        Self {
            current_strategy,
            metrics,
            last_switch: Arc::new(RwLock::new(Instant::now())),
            config,
        }
    }

    pub async fn get_current_strategy(&self) -> LoadBalanceStrategy {
        self.current_strategy.read().await.clone()
    }
    pub async fn evaluate(&self) {
        if self.should_switch().await {
            self.switch_strategy().await;
        }
    }
    async fn should_switch(&self) -> bool {
        let interval = Instant::now().sub(*self.last_switch.read().await);
        interval.ge(&self.config.switch_interval)
    }
    async fn switch_strategy(&self) {
        let threshold = &self.config.response_time_threshold;
        let curr_avg_response_time =
            Duration::from_nanos(self.metrics.avg_response_time.load(Ordering::Relaxed));

        let curr_strategy = self.current_strategy.read().await.clone();
        match curr_strategy {
            LoadBalanceStrategy::RoundRobin if curr_avg_response_time.gt(threshold) => {
                *self.current_strategy.write().await = LoadBalanceStrategy::LeastConnection;
                *self.last_switch.write().await = Instant::now();
            }
            LoadBalanceStrategy::LeastConnection if curr_avg_response_time.lt(threshold) => {
                *self.current_strategy.write().await = LoadBalanceStrategy::RoundRobin;
                *self.last_switch.write().await = Instant::now();
            }
            _ => (),
        }
    }
}
