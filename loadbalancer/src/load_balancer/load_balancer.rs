use crate::lb_config::LoadBalancerConfig;
use crate::matrics::Metrics;
use color_eyre::Result;
use color_eyre::eyre::Context;
use http_body_util::{BodyExt, Empty};
use hyper::body::Buf;
use hyper::body::{Bytes, Incoming};
use hyper::{Method, Request, Response, Uri};
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::rt::TokioIo;
use serde::Deserialize;
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tokio::net::TcpStream;
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
#[derive(Deserialize)]
enum WorkerStatus {
    UP,
    DOWN,
}
pub struct LoadBalancer {
    worker_hosts: Vec<String>,
    worker_connection_count: Arc<RwLock<HashMap<String, usize>>>,
    current_worker: usize,
    pub current_lb_strategy: Arc<RwLock<LoadBalanceStrategy>>,
    pub matrics: Arc<Metrics>,
}
#[derive(Deserialize)]
struct HealthResponseBody {
    status: WorkerStatus,
    connection_count: usize,
}
impl LoadBalancer {
    pub fn new(config: &LoadBalancerConfig) -> Result<Self> {
        if config.workers.is_empty() {
            return Err(LoadBalancerError::ConfigError.into());
        }
        let mut connector = HttpConnector::new();
        connector.set_nodelay(true);
        connector.set_keepalive(Some(Duration::from_secs(5)));
        connector.enforce_http(false);

        let worker_connection_count = Arc::new(RwLock::new(
            config
                .workers
                .iter()
                .map(|host| (host.clone(), 0))
                .collect(),
        ));
        Ok(Self {
            worker_hosts: config.workers.clone(),
            current_worker: 0,
            current_lb_strategy: Arc::new(RwLock::new(config.strategy.default_strategy.clone())),
            worker_connection_count,
            matrics: Arc::new(Metrics::default()),
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

        let stream = TcpStream::connect(worker_addr)
            .await
            .map_err(LoadBalancerError::IoError)?;
        let io = TokioIo::new(stream);

        let (mut sender, conn) = hyper::client::conn::http1::handshake(io).await?;
        tokio::task::spawn(async move {
            if let Err(err) = conn.await {
                println!("Connection failed: {:?}", err);
            }
        });
        // Extract the headers from the original request
        let headers = req.headers().clone();
        // Clone the original request's headers and method
        let mut new_req = Request::builder()
            .method(req.method())
            .uri(new_uri)
            .body(req.into_body())
            .map_err(|_| LoadBalancerError::UnexpectedError)?;

        // Copy headers from the original request
        for (key, value) in headers.iter() {
            new_req.headers_mut().insert(key, value.clone());
        }

        sender
            .send_request(new_req)
            .await
            .wrap_err("No response from worker")
    }

    pub async fn switch_current_lb_strategy(&mut self) {
        println!("Switching current lb");
        match *self.current_lb_strategy.read().await {
            LoadBalanceStrategy::RoundRobin => {
                *self.current_lb_strategy.write().await = LoadBalanceStrategy::LeastConnection
            }
            LoadBalanceStrategy::LeastConnection => {
                *self.current_lb_strategy.write().await = LoadBalanceStrategy::RoundRobin
            }
        }
        println!("Switched to {:?}", self.current_lb_strategy);
    }

    pub async fn update_worker_connection_count(&mut self) -> Result<()> {
        println!("Updating worker connection count");
        for host in &self.worker_hosts {
            if let Ok(status) = self.fetch_worker_status(host).await {
                self.worker_connection_count
                    .write()
                    .await
                    .insert(host.clone(), status.connection_count);
            }
        }
        println!(
            "Updated worker connection count:\n{:?}",
            self.worker_connection_count.read().await
        );
        Ok(())
    }

    async fn fetch_worker_status(&self, uri: &str) -> Result<HealthResponseBody> {
        let stream = TcpStream::connect(uri).await?;
        let io = TokioIo::new(stream);

        let (mut sender, conn) = hyper::client::conn::http1::handshake(io).await?;
        tokio::task::spawn(async move {
            if let Err(err) = conn.await {
                println!("Connection failed: {:?}", err);
            }
        });
        let request = Request::builder()
            .method(Method::GET)
            .uri("/health")
            .body(Empty::<Bytes>::new())
            .map_err(|_| LoadBalancerError::UnexpectedError)?;

        let response = sender.send_request(request).await?;

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

    async fn get_next_worker(&mut self) -> String {
        match *self.current_lb_strategy.read().await {
            LoadBalanceStrategy::RoundRobin => {
                let worker_idx = (self.current_worker + 1) % self.worker_hosts.len();
                self.current_worker = worker_idx;
                self.worker_hosts[worker_idx].clone()
            }
            LoadBalanceStrategy::LeastConnection => self
                .worker_connection_count
                .read()
                .await
                .iter()
                .min_by_key(|(_, count)| *count)
                .map(|(host, _)| host.clone())
                .unwrap_or_else(|| self.worker_hosts[0].clone()),
        }
    }
}
