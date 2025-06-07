use crate::LoadBalanceStrategy;
use serde::{Deserialize, Deserializer};
use std::time::Duration;

#[derive(Debug, Deserialize, Clone)]
pub struct LoadBalancerConfig {
    pub server_config: ServerConfig,
    pub workers: Vec<String>,
    pub strategy: StrategyConfig,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ServerConfig {
    pub address: String,
    #[serde(deserialize_with = "from_millis")]
    pub health_check_interval: Duration,
}

#[derive(Debug, Deserialize, Clone)]
pub struct StrategyConfig {
    pub default_strategy: LoadBalanceStrategy,
    #[serde(deserialize_with = "from_millis")]
    pub response_time_threshold: Duration,
    #[serde(deserialize_with = "from_millis")]
    pub switch_interval: Duration,
}

fn from_millis<'de, D>(deserializer: D) -> Result<Duration, D::Error>
where
    D: Deserializer<'de>,
{
    let millis = u64::deserialize(deserializer)?;
    Ok(Duration::from_millis(millis))
}
