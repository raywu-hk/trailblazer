use crate::LoadBalanceStrategy;
use serde::{Deserialize, Deserializer};
use std::time::Duration;

#[derive(Default, Debug, Deserialize, Clone)]
pub struct LoadBalancerConfig {
    pub server_config: ServerConfig,
    pub workers: Vec<String>,
    pub strategy_config: StrategyConfig,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ServerConfig {
    pub address: String,
    #[serde(deserialize_with = "from_millis")]
    pub health_check_interval: Duration,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            address: "0.0.0.0:8000".to_string(),
            health_check_interval: Duration::from_millis(1000),
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct StrategyConfig {
    pub default_strategy: LoadBalanceStrategy,
    #[serde(deserialize_with = "from_millis")]
    pub response_time_threshold: Duration,
    #[serde(deserialize_with = "from_millis")]
    pub switch_interval: Duration,
}

impl Default for StrategyConfig {
    fn default() -> Self {
        Self {
            default_strategy: LoadBalanceStrategy::RoundRobin,
            response_time_threshold: Duration::from_millis(500),
            switch_interval: Duration::from_millis(5000),
        }
    }
}

fn from_millis<'de, D>(deserializer: D) -> Result<Duration, D::Error>
where
    D: Deserializer<'de>,
{
    let millis = u64::deserialize(deserializer)?;
    Ok(Duration::from_millis(millis))
}
