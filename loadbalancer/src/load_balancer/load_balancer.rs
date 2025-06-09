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

#[derive(Debug, Deserialize, Clone)]
pub enum LoadBalanceStrategy {
    RoundRobin,
    LeastConnection,
}
#[derive(Deserialize, Debug)]
struct WorkerStatus {
    connection_count: usize,
    avg_response_time: Duration,
}
pub struct LoadBalancer {
    worker_hosts: Vec<String>,
    worker_status: Arc<RwLock<HashMap<String, WorkerStatus>>>,
    current_worker: AtomicUsize,
    strategy_manager: Arc<StrategyManager>,
    pub metrics: Arc<Metrics>,
    client: Client<HttpConnector, BoxBody<Bytes, hyper::Error>>,
    pub config: LoadBalancerConfig,
}
#[derive(Deserialize)]
struct HealthResponseBody {
    connection_count: usize,
    avg_response_time: u64,
}
impl Into<WorkerStatus> for HealthResponseBody {
    fn into(self) -> WorkerStatus {
        WorkerStatus {
            connection_count: self.connection_count,
            avg_response_time: Duration::from_nanos(self.avg_response_time),
        }
    }
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
    pub async fn forward_request(&mut self, req: Request<Incoming>) -> Result<Response<Incoming>> {
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

    pub async fn update_worker_connection_count(&mut self) -> Result<()> {
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
        match self.strategy_manager.get_current_strategy().await {
            LoadBalanceStrategy::RoundRobin => {
                let worker_idx =
                    self.current_worker.fetch_add(1, Ordering::SeqCst) % self.worker_hosts.len();
                self.worker_hosts[worker_idx].clone()
            }
            LoadBalanceStrategy::LeastConnection => self
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
