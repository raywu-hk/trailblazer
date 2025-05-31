mod constants;
mod load_balancer;
use config::Config;
pub use constants::*;
use http_body_util::{BodyExt, Empty, combinators::BoxBody};
use hyper::body::{Bytes, Incoming};
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
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
    #[error("Port{0} is not available")]
    PortCanNotBind(u16),
    #[error("Server Error")]
    ServerError,
}

#[derive(Debug, Deserialize)]
struct Settings {
    workers: Vec<String>,
}

pub struct Application {
    pub listener: TcpListener,
    load_balancer: Arc<RwLock<LoadBalancer>>,
}
impl Application {
    pub async fn new(address: &str) -> Result<Self, ApplicationError> {
        let settings = Self::load_config()?;
        let mut worker_hosts = Vec::with_capacity(settings.workers.len());
        for address in &settings.workers {
            worker_hosts.push(address.to_string());
        }

        let load_balancer = Arc::new(RwLock::new(
            LoadBalancer::new(worker_hosts).expect("failed to create load balancer"),
        ));

        let addr = SocketAddr::from_str(address).unwrap();
        let listener = TcpListener::bind(addr)
            .await
            .map_err(|_| ApplicationError::PortCanNotBind(addr.port()))?;

        let app = Self {
            listener,
            load_balancer: load_balancer.clone(),
        };
        Ok(app)
    }
    pub async fn run(&self) -> Result<(), Box<dyn Error + Send + Sync>> {
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
    ) -> Result<Response<BoxBody<Bytes, hyper::Error>>, ApplicationError> {
        match req.uri().path() {
            "/switch" => {
                load_balancer
                    .write()
                    .await
                    .switch_current_lb_strategy()
                    .await;
                Ok(Response::builder()
                    .status(StatusCode::OK)
                    .body(
                        Empty::<Bytes>::new()
                            .map_err(|never| match never {})
                            .boxed(),
                    )
                    .map_err(|_| ApplicationError::ServerError)?)
            }
            _ => {
                let res = load_balancer
                    .write()
                    .await
                    .forward_request(req)
                    .await
                    .map_err(|_| ApplicationError::ServerError)?;
                Ok(res.map(|body| body.boxed()))
            }
        }
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
