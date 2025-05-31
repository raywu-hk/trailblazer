use hyper::body::Incoming;
use hyper::{Request, Response, Uri};
use hyper_util::client::legacy::Client;
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::rt::{TokioExecutor, TokioTimer};
use std::collections::HashMap;
use std::error::Error;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;

#[derive(Debug)]
enum LoadBalanceStrategy {
    RoundRobin,
    LeastConnection,
}
pub struct LoadBalancer {
    worker_hosts: Vec<String>,
    worker_connection_count: Arc<RwLock<HashMap<String, isize>>>,
    client: Client<HttpConnector, Incoming>,
    current_worker: usize,
    current_lb_strategy: LoadBalanceStrategy,
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
        let client = Client::builder(TokioExecutor::new())
            .pool_timer(TokioTimer::new())
            .pool_idle_timeout(Duration::from_secs(30))
            .pool_max_idle_per_host(10)
            .build(connector);

        let worker_connection_count = worker_hosts.iter().map(|host| (host.clone(), 0)).collect();

        Ok(Self {
            worker_hosts,
            current_worker: 0,
            client,
            current_lb_strategy: LoadBalanceStrategy::RoundRobin,
            worker_connection_count: Arc::new(RwLock::new(worker_connection_count)),
        })
    }

    pub async fn forward_request(&mut self, req: Request<Incoming>) -> Result<Response<Incoming>> {
        let worker_addr = self.get_next_worker().await.to_string();

        self.update_connection_count(&worker_addr, 1).await;

        let mut worker_uri = worker_addr.clone();
        // Extract the path and query from the original request
        if let Some(path_and_query) = req.uri().path_and_query() {
            worker_uri.push_str(path_and_query.as_str());
        }
        worker_uri = format!("http://{}", worker_uri);
        // Create a new URI from the worker URI
        let new_uri = Uri::from_str(worker_uri.as_str()).unwrap();

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

        // self.update_connection_count(&worker_addr, 1).await;
        // println!("{:?}", self.worker_connection_count.read().await);

        let response = self
            .client
            .request(new_req)
            .await
            .map_err(|e| -> Box<dyn Error + Send + Sync> { Box::new(e) })?;

        self.update_connection_count(&worker_addr, -1).await;

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

    async fn update_connection_count(&mut self, worker_uri: &str, count: isize) {
        if let Some(curr_count) = self
            .worker_connection_count
            .write()
            .await
            .get_mut(worker_uri)
        {
            *curr_count += count;
        }
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
