use http_body_util::{BodyExt, Empty};
use hyper::body::Buf;
use hyper::body::{Bytes, Incoming};
use hyper::{Method, Request, Response, Uri};
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::rt::TokioIo;
use serde::Deserialize;
use std::collections::HashMap;
use std::error::Error;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpStream;
use tokio::sync::RwLock;

type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;

#[derive(Debug)]
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
    current_lb_strategy: LoadBalanceStrategy,
}
#[derive(Deserialize)]
struct HealthResponseBody {
    status: WorkerStatus,
    connection_count: usize,
}
impl LoadBalancer {
    pub fn new(worker_hosts: Vec<String>) -> Result<Self> {
        if worker_hosts.is_empty() {
            return Err("No worker hosts provided".into());
        }
        // let worker_health = worker_hosts.iter().map(|host| (host.clone(), 0)).collect();
        let mut connector = HttpConnector::new();
        connector.set_nodelay(true);
        connector.set_keepalive(Some(Duration::from_secs(5)));
        connector.enforce_http(false);

        let worker_connection_count = Arc::new(RwLock::new(
            worker_hosts.iter().map(|host| (host.clone(), 0)).collect(),
        ));
        Ok(Self {
            worker_hosts,
            current_worker: 0,
            current_lb_strategy: LoadBalanceStrategy::RoundRobin,
            worker_connection_count,
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
        let new_uri = Uri::from_str(worker_uri.as_str()).unwrap();

        let stream = TcpStream::connect(worker_addr).await?;
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
            .body(req.into_body())?;

        // Copy headers from the original request
        for (key, value) in headers.iter() {
            new_req.headers_mut().insert(key, value.clone());
        }

        let response = sender
            .send_request(new_req)
            .await
            .map_err(|e| -> Box<dyn Error + Send + Sync> { Box::new(e) })?;
        Ok(response)
    }

    pub async fn switch_current_lb_strategy(&mut self) {
        println!("Switching current lb");
        match self.current_lb_strategy {
            LoadBalanceStrategy::RoundRobin => {
                self.current_lb_strategy = LoadBalanceStrategy::LeastConnection
            }
            LoadBalanceStrategy::LeastConnection => {
                self.current_lb_strategy = LoadBalanceStrategy::RoundRobin
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
            .body(Empty::<Bytes>::new())?;
        let response = sender.send_request(request).await?;
        if !response.status().is_success() {
            return Err("Failed to fetch worker status".into());
        }
        let body = response.collect().await?.aggregate();
        let health_response = serde_json::from_reader(body.reader())?;

        Ok(health_response)
    }

    async fn get_next_worker(&mut self) -> String {
        match self.current_lb_strategy {
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
