mod constants;
mod lb_config;
mod load_balancer;
mod metrics;
mod strategy_controller;

use crate::lb_config::LoadBalancerConfig;
use crate::metrics::{LoadBalancerMetricsLayer, Metrics};
use crate::strategy_controller::StrategyManager;
use color_eyre::Result;
use config::{Config, ConfigError};
pub use constants::*;
use http_body_util::{BodyExt, combinators::BoxBody};
use hyper::body::{Bytes, Incoming};
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response};
use hyper_util::rt::TokioIo;
pub use load_balancer::*;
use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::Arc;
use thiserror::Error;
use tokio::net::TcpListener;
use tokio::sync::RwLock;
use tower::ServiceBuilder;

#[derive(Error, Debug)]
pub enum ApplicationError {
    #[error("Config file not found:{0}")]
    ConfigFileNotFound(String),
    #[error("Env var {0} not found")]
    EnvConfigNotFound(String),
    #[error("Config Invalid: {0}")]
    ConfigInvalid(ConfigError),
    #[error("Port {0} is not available")]
    PortCanNotBind(u16),
    #[error("Server Error")]
    ServerError,
    #[error("Health check error")]
    HealthCheckError,
}

pub struct Application {
    pub listener: TcpListener,
    pub load_balancer: Arc<RwLock<LoadBalancer>>,
}
impl Application {
    pub async fn new(address: &str) -> Result<Self> {
        let settings = Self::load_config()?;
        let mut worker_hosts = Vec::with_capacity(settings.workers.len());
        for address in &settings.workers {
            worker_hosts.push(address.to_string());
        }
        let metrics = Arc::new(Metrics::default());
        let strategy_manager = Arc::new(StrategyManager::new(
            Arc::new(settings.strategy_config.clone()),
            metrics.clone(),
        ));
        let load_balancer = Arc::new(RwLock::new(
            LoadBalancer::new(settings, strategy_manager, metrics.clone())
                .expect("failed to create load balancer"),
        ));

        let addr = SocketAddr::from_str(address)?;
        let listener = TcpListener::bind(addr)
            .await
            .map_err(|_| ApplicationError::PortCanNotBind(addr.port()))?;

        let app = Self {
            listener,
            load_balancer: load_balancer.clone(),
        };
        Ok(app)
    }
    pub async fn run(&self) -> Result<()> {
        loop {
            let (stream, _) = self.listener.accept().await?;

            // Use an adapter to access something implementing `tokio::io` traits as if they implement
            // `hyper::rt` IO traits.
            let io = TokioIo::new(stream);

            // Spawn a tokio task to serve multiple connections concurrently
            let load_balancer = self.load_balancer.clone();
            tokio::task::spawn(async move {
                let service = service_fn(|req| Self::handle(req, load_balancer.clone()));
                let matrics_lb_layer = load_balancer.read().await.metrics.clone();
                let layered_service = ServiceBuilder::new()
                    .layer_fn(move |service| {
                        LoadBalancerMetricsLayer::new(service, matrics_lb_layer.clone())
                    })
                    .service(service);
                if let Err(err) = http1::Builder::new()
                    // `service_fn` converts our function in a `Service`
                    .serve_connection(io, layered_service)
                    .await
                {
                    eprintln!("LB Error serving connection: {:?}", err);
                }
            });
        }
    }

    pub async fn run_health_check(&self) -> Result<()> {
        self.load_balancer
            .write()
            .await
            .update_worker_connection_count()
            .await
            .map_err(|_| ApplicationError::HealthCheckError)?;
        Ok(())
    }

    async fn handle(
        req: Request<Incoming>,
        load_balancer: Arc<RwLock<LoadBalancer>>,
    ) -> Result<Response<BoxBody<Bytes, hyper::Error>>, ApplicationError> {
        let res = load_balancer
            .write()
            .await
            .forward_request(req)
            .await
            .map_err(|_| ApplicationError::ServerError)?;
        Ok(res.map(|body| body.boxed()))
    }

    fn load_config() -> Result<LoadBalancerConfig> {
        let config = Config::builder()
            .add_source(config::File::with_name(&CONFIG_FILE_PATH))
            .build()
            .map_err(|_| ApplicationError::ConfigFileNotFound(CONFIG_FILE_PATH.to_string()))?;

        config
            .try_deserialize()
            .map_err(|e| ApplicationError::ConfigInvalid(e).into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn worker_config_should_loaded() {
        let setting = Application::load_config().unwrap();

        assert!(!setting.workers.is_empty());
    }
}
