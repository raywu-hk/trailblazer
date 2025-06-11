use crate::lb_config::LoadBalancerConfig;
use crate::metrics::Metrics;
use crate::strategy_controller::StrategyManager;
use color_eyre::Result;
use color_eyre::eyre::Context;
use http_body_util::BodyExt;
use http_body_util::combinators::BoxBody;
use hyper::body::Buf;
use hyper::body::{Bytes, Incoming};
use hyper::{Method, Request, Response, Uri};
use hyper_util::client::legacy::Client;
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::rt::{TokioExecutor, TokioTimer};
use serde::Deserialize;
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use thiserror::Error;
use tokio::sync::RwLock;

#[derive(Error, Debug)]
pub enum LoadBalancerError {
    #[error("Configuration error")]
    ConfigError,
    #[error("Network error: {0}")]
    HyperError(#[from] hyper::Error),
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("Unexpected Error")]
    UnexpectedError,
    #[error("Worker error: {0}")]
    WorkerError(String),
    #[error("Strategy error: {0}")]
    StrategyError(String),
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub enum LoadBalanceStrategy {
    RoundRobin,
    LeastAvgResponseTime,
}

#[allow(unused)]
#[derive(Deserialize, Debug)]
struct WorkerStatus {
    connection_count: usize,
    avg_response_time: Duration,
}
impl From<HealthResponseBody> for WorkerStatus {
    fn from(value: HealthResponseBody) -> Self {
        Self {
            connection_count: value.connection_count,
            avg_response_time: Duration::from_nanos(value.avg_response_time),
        }
    }
}
pub struct LoadBalancer {
    worker_hosts: Vec<String>,
    worker_status: Arc<RwLock<HashMap<String, WorkerStatus>>>,
    current_worker: AtomicUsize,
    pub strategy_manager: Arc<StrategyManager>,
    pub metrics: Arc<Metrics>,
    client: Client<HttpConnector, BoxBody<Bytes, hyper::Error>>,
    pub config: LoadBalancerConfig,
}
#[derive(Deserialize)]
struct HealthResponseBody {
    connection_count: usize,
    avg_response_time: u64,
}

impl LoadBalancer {
    pub fn new(
        config: LoadBalancerConfig,
        strategy_manager: Arc<StrategyManager>,
        matrics: Arc<Metrics>,
    ) -> Result<Self> {
        if config.workers.is_empty() {
            return Err(LoadBalancerError::ConfigError.into());
        }
        let mut connector = HttpConnector::new();
        connector.set_nodelay(true);
        connector.set_keepalive(Some(Duration::from_secs(5)));
        connector.enforce_http(false);
        let client = Client::builder(TokioExecutor::new())
            .pool_max_idle_per_host(10)
            .pool_timer(TokioTimer::default())
            .pool_idle_timeout(Duration::from_secs(5))
            .build(connector);

        let worker_status = Arc::new(RwLock::new(
            config
                .workers
                .iter()
                .map(|host| {
                    (
                        host.clone(),
                        WorkerStatus {
                            connection_count: 0,
                            avg_response_time: Duration::ZERO,
                        },
                    )
                })
                .collect(),
        ));
        Ok(Self {
            worker_hosts: config.workers.clone(),
            current_worker: AtomicUsize::new(0),
            strategy_manager,
            worker_status,
            metrics: matrics,
            client,
            config,
        })
    }
    pub async fn forward_request(&self, req: Request<Incoming>) -> Result<Response<Incoming>> {
        let worker_addr = self.get_next_worker().await.to_string();

        let mut worker_uri = worker_addr.clone();
        // Extract the path and query from the original request
        if let Some(path_and_query) = req.uri().path_and_query() {
            worker_uri.push_str(path_and_query.as_str());
        }
        worker_uri = format!("http://{}", worker_uri);
        // Create a new URI from the worker URI
        let new_uri = Uri::from_str(worker_uri.as_str())?;
        // Extract the headers from the original request
        let headers = req.headers().clone();
        // Clone the original request's headers and method
        let mut new_req = Request::builder()
            .method(req.method())
            .uri(new_uri)
            .body(req.into_body().boxed())
            .map_err(|_| LoadBalancerError::UnexpectedError)?;

        // Copy headers from the original request
        for (key, value) in headers.iter() {
            new_req.headers_mut().insert(key, value.clone());
        }

        self.client
            .request(new_req)
            .await
            .wrap_err("No response from worker")
    }

    pub async fn update_worker_connection_count(&self) -> Result<()> {
        println!("Updating worker connection count");
        for host in &self.worker_hosts {
            if let Ok(status) = self.fetch_worker_status(host).await {
                self.worker_status
                    .write()
                    .await
                    .insert(host.clone(), status.into());
            }
        }
        println!(
            "Updated worker connection count:\n{:?}",
            self.worker_status.read().await
        );
        Ok(())
    }

    async fn fetch_worker_status(&self, uri: &str) -> Result<HealthResponseBody> {
        let request = Request::builder()
            .method(Method::GET)
            .uri(format!("http://{}/health", uri))
            .body(BoxBody::default())
            .map_err(|_| LoadBalancerError::UnexpectedError)?;

        let response = self.client.request(request).await?;

        if !response.status().is_success() {
            return Err(LoadBalancerError::WorkerError(
                "Worker returned non-200 status code".to_owned(),
            )
            .into());
        }
        let body = response.collect().await?.aggregate();
        let health_response = serde_json::from_reader(body.reader())
            .map_err(|_| LoadBalancerError::UnexpectedError)?;

        Ok(health_response)
    }

    async fn get_next_worker(&self) -> String {
        match self.strategy_manager.get_current_strategy() {
            LoadBalanceStrategy::RoundRobin => {
                let worker_idx =
                    self.current_worker.fetch_add(1, Ordering::SeqCst) % self.worker_hosts.len();
                self.worker_hosts[worker_idx].clone()
            }
            LoadBalanceStrategy::LeastAvgResponseTime => self
                .worker_status
                .read()
                .await
                .iter()
                .min_by_key(|(_, count)| count.avg_response_time)
                .map(|(host, _)| host.clone())
                .unwrap_or_else(|| self.worker_hosts[0].clone()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lb_config::{LoadBalancerConfig, StrategyConfig};
    use crate::metrics::Metrics;
    use crate::strategy_controller::StrategyManager;
    use std::time::Duration;

    fn create_test_config(load_balance_strategy: LoadBalanceStrategy) -> LoadBalancerConfig {
        LoadBalancerConfig {
            workers: vec![
                "127.0.0.1:8001".to_string(),
                "127.0.0.1:8002".to_string(),
                "127.0.0.1:8003".to_string(),
            ],
            strategy_config: StrategyConfig {
                default_strategy: load_balance_strategy,
                ..Default::default()
            },
            ..Default::default()
        }
    }

    fn create_test_load_balancer(
        load_balance_strategy: LoadBalanceStrategy,
    ) -> Result<LoadBalancer> {
        let config = create_test_config(load_balance_strategy);
        let metrics = Arc::new(Metrics::default());
        let strategy_manager = Arc::new(StrategyManager::new(
            Arc::new(config.strategy_config.clone()),
            metrics.clone(),
        ));
        LoadBalancer::new(config, strategy_manager, metrics)
    }

    #[tokio::test]
    async fn test_load_balancer_creation_success() {
        let lb = create_test_load_balancer(LoadBalanceStrategy::RoundRobin);
        assert!(lb.is_ok());

        let lb = lb.unwrap();
        assert_eq!(lb.worker_hosts.len(), 3);
        assert_eq!(lb.worker_hosts[0], "127.0.0.1:8001");
        assert_eq!(lb.worker_hosts[1], "127.0.0.1:8002");
        assert_eq!(lb.worker_hosts[2], "127.0.0.1:8003");
    }

    #[tokio::test]
    async fn test_load_balancer_creation_with_empty_workers() {
        let config = LoadBalancerConfig {
            workers: vec![],
            ..Default::default()
        };
        let metrics = Arc::new(Metrics::default());
        let strategy_manager = Arc::new(StrategyManager::new(
            Arc::new(config.strategy_config.clone()),
            metrics.clone(),
        ));

        let result = LoadBalancer::new(config, strategy_manager, metrics);
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_round_robin_strategy() {
        let lb = create_test_load_balancer(LoadBalanceStrategy::RoundRobin).unwrap();

        // Test that workers are selected in round-robin
        let worker1 = lb.get_next_worker().await;
        let worker2 = lb.get_next_worker().await;
        let worker3 = lb.get_next_worker().await;
        let worker4 = lb.get_next_worker().await; // Should wrap around to first

        assert_eq!(worker1, "127.0.0.1:8001");
        assert_eq!(worker2, "127.0.0.1:8002");
        assert_eq!(worker3, "127.0.0.1:8003");
        assert_eq!(worker4, "127.0.0.1:8001");
    }

    #[tokio::test]
    async fn test_least_avg_response_time_strategy() {
        let lb = create_test_load_balancer(LoadBalanceStrategy::LeastAvgResponseTime).unwrap();

        // Manually set worker statuses with different response times
        {
            let mut status_map = lb.worker_status.write().await;
            status_map.insert(
                "127.0.0.1:8001".to_string(),
                WorkerStatus {
                    connection_count: 5,
                    avg_response_time: Duration::from_millis(100),
                },
            );
            status_map.insert(
                "127.0.0.1:8002".to_string(),
                WorkerStatus {
                    connection_count: 3,
                    avg_response_time: Duration::from_millis(50), // Fastest
                },
            );
            status_map.insert(
                "127.0.0.1:8003".to_string(),
                WorkerStatus {
                    connection_count: 2,
                    avg_response_time: Duration::from_millis(200),
                },
            );
        }

        // Should select the worker with least average response time
        let selected_worker = lb.get_next_worker().await;
        assert_eq!(selected_worker, "127.0.0.1:8002");
    }

    #[test]
    fn test_worker_status_from_health_response_body() {
        let health_response = HealthResponseBody {
            connection_count: 10,
            avg_response_time: 150_000_000, // 150ms in nanoseconds
        };

        let worker_status: WorkerStatus = health_response.into();
        assert_eq!(worker_status.connection_count, 10);
        assert_eq!(worker_status.avg_response_time, Duration::from_millis(150));
    }

    #[tokio::test]
    async fn test_worker_status_initialization() {
        let lb = create_test_load_balancer(LoadBalanceStrategy::RoundRobin).unwrap();
        let status_map = lb.worker_status.read().await;

        // All workers should be initialized with zero values
        for worker in &lb.worker_hosts {
            let status = status_map.get(worker).unwrap();
            assert_eq!(status.connection_count, 0);
            assert_eq!(status.avg_response_time, Duration::ZERO);
        }
    }

    #[test]
    fn test_load_balancer_error_display() {
        let config_error = LoadBalancerError::ConfigError;
        assert_eq!(config_error.to_string(), "Configuration error");

        let worker_error = LoadBalancerError::WorkerError("Test error".to_string());
        assert_eq!(worker_error.to_string(), "Worker error: Test error");

        let strategy_error = LoadBalancerError::StrategyError("Strategy failed".to_string());
        assert_eq!(
            strategy_error.to_string(),
            "Strategy error: Strategy failed"
        );
    }
}
