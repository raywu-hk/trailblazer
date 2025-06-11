use crate::worker::Worker;
use config::Config;
use futures::future::join_all;
use serde::Deserialize;
use std::sync::Arc;
use thiserror::Error;

mod constants;
mod worker;
pub use constants::*;
#[derive(Error, Debug)]
pub enum ApplicationError {
    #[error("Config file not found:{0}")]
    ConfigFileNotFound(String),
    #[error("Env var {0} not found")]
    EnvConfigNotFound(String),
    #[error("Config Invalid")]
    ConfigInvalid,
    #[error("Port {0} is not available")]
    PortCanNotBind(u16),
    #[error("Server Error")]
    ServerError,
}
#[derive(Debug, Deserialize)]
struct Settings {
    workers: Vec<String>,
}
pub struct Application {
    pub workers: Vec<Arc<Worker>>,
}
impl Default for Application {
    fn default() -> Self {
        Self::new()
    }
}

impl Application {
    pub fn new() -> Application {
        let settings = Self::load_config().expect("Config file error");
        let mut workers = Vec::with_capacity(settings.workers.len());
        for workers_address in &settings.workers {
            workers.push(Arc::new(Worker::new(workers_address)));
        }
        Self { workers }
    }

    pub async fn run(&self) {
        let handle = self
            .workers
            .iter()
            .map(async move |worker| worker.run().await)
            .collect::<Vec<_>>();
        join_all(handle).await;
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
