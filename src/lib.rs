mod constants;
mod load_balancer;
mod worker;

use crate::worker::Worker;
use config::Config;
pub use constants::*;
use hyper::body::Incoming;

use hyper::service::service_fn;
use hyper::{Request, Response};
use hyper_util::rt::{TokioExecutor, TokioIo};
use hyper_util::server::conn::auto;
pub use load_balancer::*;
use serde::Deserialize;
use std::error::Error;
use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::Arc;
use thiserror::Error;
use tokio::net::TcpListener;
use tokio::sync::RwLock;

#[derive(Error, Debug)]
pub enum ApplicationError {
    #[error("Config file not found:{0}")]
    ConfigFileNotFound(String),
    #[error("Env var {0} not found")]
    EnvConfigNotFound(String),
    #[error("Config Invalid")]
    ConfigInvalid,
    #[error("Unexpected Error")]
    UnexpectedError,
}

#[derive(Debug, Deserialize)]
struct Settings {
    workers: Vec<String>,
}

pub struct Application {
    pub listener: TcpListener,
    pub workers: Vec<Arc<Worker>>,
    load_balancer: Arc<RwLock<LoadBalancer>>,
}
impl Application {
    pub async fn new(address: &str) -> Result<Self, ApplicationError> {
        let settings = Self::load_config()?;
        let mut workers = Vec::with_capacity(settings.workers.len());
        let mut worker_hosts = Vec::with_capacity(settings.workers.len());
        for (idx, address) in settings.workers.iter().enumerate() {
            workers.push(Arc::new(Worker::new(address, idx)));
            worker_hosts.push(address.to_string());
        }

        let load_balancer = Arc::new(RwLock::new(
            LoadBalancer::new(worker_hosts).expect("failed to create load balancer"),
        ));

        let addr = SocketAddr::from_str(address).unwrap();
        let listener = TcpListener::bind(addr)
            .await
            .map_err(|_| ApplicationError::UnexpectedError)?;

        let app = Self {
            listener,
            workers,
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
                if let Err(err) = auto::Builder::new(TokioExecutor::new())
                    // `service_fn` converts our function in a `Service`
                    .serve_connection(
                        io,
                        service_fn(|req| Self::handle(req, load_balancer.clone())),
                    )
                    .await
                {
                    eprintln!("Error serving connection: {:?}", err);
                }
            });
        }
    }

    async fn handle(
        req: Request<Incoming>,
        load_balancer: Arc<RwLock<LoadBalancer>>,
    ) -> Result<Response<Incoming>, Box<dyn Error + Send + Sync>> {
        load_balancer.write().await.forward_request(req).await
    }

    fn load_config() -> Result<Settings, ApplicationError> {
        let config = Config::builder()
            .add_source(config::File::with_name(&CONFIG_FILE_PATH))
            .build()
            .map_err(|_| ApplicationError::ConfigFileNotFound(CONFIG_FILE_PATH.to_string()))?;

        config
            .try_deserialize()
            .map_err(|_| ApplicationError::ConfigInvalid)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn worker_config_should_loaded() {
        let setting = Application::load_config().unwrap();

        assert_eq!(setting.workers.is_empty(), false);
    }
}
