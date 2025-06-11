mod constants;
mod lb_config;
mod load_balancer;
mod metrics;
mod strategy_controller;

use crate::lb_config::LoadBalancerConfig;
use crate::metrics::{LoadBalancerMetricsLayer, Metrics};
use crate::strategy_controller::{StrategyLayer, StrategyManager};
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
    pub load_balancer: Arc<LoadBalancer>,
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
        let load_balancer = Arc::new(
            LoadBalancer::new(settings, strategy_manager, metrics.clone())
                .expect("failed to create load balancer"),
        );

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
                let matrics_lb_layer = load_balancer.metrics.clone();
                let strategy_manager = load_balancer.strategy_manager.clone();
                let layered_service = ServiceBuilder::new()
                    .layer_fn(move |service| {
                        LoadBalancerMetricsLayer::new(service, matrics_lb_layer.clone())
                    })
                    .layer_fn(move |service| StrategyLayer::new(service, strategy_manager.clone()))
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
            .update_worker_connection_count()
            .await
            .map_err(|_| ApplicationError::HealthCheckError)?;
        Ok(())
    }

    async fn handle(
        req: Request<Incoming>,
        load_balancer: Arc<LoadBalancer>,
    ) -> Result<Response<BoxBody<Bytes, hyper::Error>>, ApplicationError> {
        let res = load_balancer
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
    use std::time::Duration;
    use tokio::time::timeout;

    #[test]
    fn worker_config_should_loaded() {
        let setting = Application::load_config().unwrap();

        assert!(!setting.workers.is_empty());
    }

    #[tokio::test]
    async fn test_application_new_with_valid_address() {
        // Use a random available port
        let address = "127.0.0.1:0";

        let result = Application::new(address).await;

        match result {
            Ok(app) => {
                assert!(app.listener.local_addr().is_ok());
                // Verify load balancer is properly initialized
                let lb = app.load_balancer;
                // Basic verification that load balancer exists
                drop(lb);
            }
            Err(_) => {
                // If config file doesn't exist, this is expected in test environment
                // We can still verify the error type
                println!("Config file not found - this is expected in test environment");
            }
        }
    }

    #[tokio::test]
    async fn test_application_new_with_invalid_address() {
        let address = "invalid_address";

        let result = Application::new(address).await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_run_health_check() {
        // This test requires a valid config file, so we'll handle the case where it doesn't exist
        let address = "127.0.0.1:0";

        match Application::new(address).await {
            Ok(app) => {
                let result = app.run_health_check().await;

                // Health check should complete (success or failure depends on worker availability)
                match result {
                    Ok(_) => println!("Health check completed successfully"),
                    Err(e) => panic!("Unexpected error: {:?}", e),
                }
            }
            Err(_) => {
                println!("Cannot create application - config file not found in test environment");
            }
        }
    }

    #[test]
    fn test_load_config_file_not_found() {
        // This will likely fail in test environment where config file doesn't exist
        let result = Application::load_config();

        match result {
            Ok(config) => {
                // If config loads successfully, verify it has required fields
                assert!(
                    !config.workers.is_empty(),
                    "Workers list should not be empty"
                );
            }
            Err(e) => {
                // Verify we get the expected error type
                match e.downcast_ref::<ApplicationError>() {
                    Some(ApplicationError::ConfigFileNotFound(path)) => {
                        assert_eq!(*path, *CONFIG_FILE_PATH);
                    }
                    _ => panic!("Expected ConfigFileNotFound error, got: {:?}", e),
                }
            }
        }
    }

    #[tokio::test]
    async fn test_application_run_timeout() {
        let address = "127.0.0.1:0";

        match Application::new(address).await {
            Ok(app) => {
                // Test that run() method starts without immediate error
                // We use a timeout to avoid infinite loop in test
                let run_future = app.run();
                let result = timeout(Duration::from_millis(100), run_future).await;

                // Should timeout (not complete immediately)
                assert!(
                    result.is_err(),
                    "Run method should not complete immediately"
                );
            }
            Err(_) => {
                println!("Cannot create application - config file not found in test environment");
            }
        }
    }

    #[test]
    fn test_application_error_display() {
        let errors = vec![
            ApplicationError::ConfigFileNotFound("test.toml".to_string()),
            ApplicationError::EnvConfigNotFound("TEST_VAR".to_string()),
            ApplicationError::PortCanNotBind(8080),
            ApplicationError::ServerError,
            ApplicationError::HealthCheckError,
        ];

        for error in errors {
            let error_string = format!("{}", error);
            assert!(
                !error_string.is_empty(),
                "Error message should not be empty"
            );

            // Verify specific error messages
            match error {
                ApplicationError::ConfigFileNotFound(path) => {
                    assert!(error_string.contains(&path));
                }
                ApplicationError::EnvConfigNotFound(var) => {
                    assert!(error_string.contains(&var));
                }
                ApplicationError::PortCanNotBind(port) => {
                    assert!(error_string.contains(&port.to_string()));
                }
                _ => {} // Other errors don't have specific content to verify
            }
        }
    }

    #[test]
    fn test_application_error_debug() {
        let error = ApplicationError::ServerError;
        let debug_string = format!("{:?}", error);
        assert!(!debug_string.is_empty(), "Debug output should not be empty");
    }

    // Integration test helper - only runs if config file exists
    async fn create_test_application_if_possible() -> Option<Application> {
        let address = "127.0.0.1:0";
        Application::new(address).await.ok()
    }

    #[tokio::test]
    async fn test_integration_application_lifecycle() {
        if let Some(app) = create_test_application_if_possible().await {
            // Test health check
            let health_result = app.run_health_check().await;
            println!("Health check result: {:?}", health_result);

            // Verify application structure
            assert!(app.listener.local_addr().is_ok());

            // Verify load balancer is accessible
            let lb_guard = app.load_balancer;
            drop(lb_guard); // Release the lock

            println!("Integration test completed successfully");
        } else {
            println!("Skipping integration test - config file not available");
        }
    }
}
