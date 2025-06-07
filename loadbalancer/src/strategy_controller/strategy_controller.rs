use crate::LoadBalanceStrategy;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

pub struct StrategyController {}

impl StrategyController {
    pub async fn evaluate_strategy_by_avg_response_time(
        avg_response_time: Arc<RwLock<Duration>>,
    ) -> LoadBalanceStrategy {
        match avg_response_time.read().await.as_millis() {
            0..500 => LoadBalanceStrategy::RoundRobin,
            _ => LoadBalanceStrategy::LeastConnection,
        }
    }
}
