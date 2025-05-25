mod constants;
mod load_balancer;
mod worker;

pub use constants::*;
use hyper::body::Incoming;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response};
use hyper_util::rt::TokioIo;
pub use load_balancer::*;
use std::error::Error;
use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::RwLock;

pub struct Application {
    pub listener: TcpListener,
    load_balancer: Arc<RwLock<LoadBalancer>>,
}
impl Application {
    pub async fn new(address: &str) -> Result<Self, Box<dyn Error>> {
        let worker_hosts = vec![
            "localhost:3000".to_string(),
            "localhost:3001".to_string(),
        ];
        let load_balancer = Arc::new(RwLock::new(
            LoadBalancer::new(worker_hosts).expect("failed to create load balancer"),
        ));

        let addr = SocketAddr::from_str(address).unwrap();
        let listener = TcpListener::bind(addr).await?;

        let app = Self {
            listener,
            load_balancer: load_balancer.clone(),
        };
        Ok(app)
    }
    pub async fn run(&self) -> Result<(), Box<dyn Error + Send + Sync + 'static>> {
        // We start a loop to continuously accept incoming connections
        loop {
            let (stream, _) = self.listener.accept().await?;

            // Use an adapter to access something implementing `tokio::io` traits as if they implement
            // `hyper::rt` IO traits.
            let io = TokioIo::new(stream);

            // Spawn a tokio task to serve multiple connections concurrently
            let load_balancer = self.load_balancer.clone();
            tokio::task::spawn(async move {
                // Finally, we bind the incoming connection to our `hello` service
                if let Err(err) = http1::Builder::new()
                    // `service_fn` converts our function in a `Service`
                    .serve_connection(io, service_fn(|req| handle(req, load_balancer.clone())))
                    .await
                {
                    eprintln!("Error serving connection: {:?}", err);
                }
            });
        }
    }
}
async fn handle(
    req: Request<Incoming>,
    load_balancer: Arc<RwLock<LoadBalancer>>,
) -> Result<Response<Incoming>, Box<dyn Error + Send + Sync>> {
    load_balancer.write().await.forward_request(req).await
}