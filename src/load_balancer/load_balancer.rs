use hyper::body::Incoming;
use hyper::{Request, Response, Uri};
use hyper_util::client::legacy::Client;
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::rt::{TokioExecutor, TokioTimer};
use std::error::Error;
use std::str::FromStr;
use std::time::Duration;

type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;

pub struct LoadBalancer {
    worker_hosts: Vec<String>,
    // worker_connection_count: Arc<RwLock<HashMap<String, isize>>>,
    client: Client<HttpConnector, Incoming>,
    current_worker: usize,
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

        Ok(Self {
            worker_hosts,
            current_worker: 0,
            client,
            // worker_connection_count: Arc::new(RwLock::new(worker_health)),
        })
    }

    pub async fn forward_request(&mut self, req: Request<Incoming>) -> Result<Response<Incoming>> {
        let worker_addr = self.get_next_worker().to_string();

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

        let response = self.client.request(new_req).await;

        match response {
            Ok(res) => {
                // self.update_connection_count(&worker_addr, -1).await;
                Ok(res)
            }
            Err(err) => {
                // self.update_connection_count(&worker_addr, -1).await;
                Err(err.into())
            }
        }
    }

    /*
    async fn update_connection_count(&mut self, worker_uri: &str, count: isize) {
        if let Some(curr_count) = self.worker_connection_count.write().await.get_mut(worker_uri) {
            *curr_count += count;
        }
    }
    */

    fn get_next_worker(&mut self) -> &String {
        // Use a round-robin strategy to select a worker
        // let idx = self.worker_connections_count.read().await.iter().enumerate()
        //     .min_by_key(|&(_idx, &count)| count)
        //     .map(|(idx, _count)| idx)
        //     .unwrap();
        // *self.current_worker.write().await = idx;
        let worker_idx = (self.current_worker + 1) % self.worker_hosts.len();
        self.current_worker = worker_idx;
        self.worker_hosts.get(worker_idx).unwrap()
    }
}
