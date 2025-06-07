use crate::LoadBalanceStrategy;
use hyper::body::{Body, Incoming};

use crate::matrics::Metrics;
use hyper::service::Service;
use hyper::{Request, Response};
use std::ops::Sub;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

pub struct StrategyController<S> {
    inner: S,
    current_strategy: Arc<RwLock<LoadBalanceStrategy>>,
    matrics: Arc<Metrics>,
    threshold: Duration,
    last_switch_at: Arc<Mutex<Instant>>,
    interval: Duration,
}

impl<S> StrategyController<S> {
    pub fn new(
        inner: S,
        current_strategy: Arc<RwLock<LoadBalanceStrategy>>,
        matrics: Arc<Metrics>,
    ) -> Self {
        Self {
            inner,
            current_strategy,
            matrics,
            threshold: Duration::from_millis(500),
            last_switch_at: Arc::new(Mutex::new(Instant::now())),
            interval: Duration::from_secs(5),
        }
    }
}
impl<S, B> Service<Request<Incoming>> for StrategyController<S>
where
    S: Service<Request<Incoming>, Response = Response<B>> + Send + Sync,
    S::Error: Send,
    B: Body + Send,
{
    type Response = Response<B>;
    type Error = S::Error;
    type Future = S::Future;
    fn call(&self, req: Request<Incoming>) -> Self::Future {
        if Instant::now()
            .sub(*self.last_switch_at.lock().unwrap())
            .lt(&self.interval)
        {
            return self.inner.call(req);
        }

        let avg_response_time = self.matrics.avg_response_time.clone();
        let threshold = self.threshold;
        let current_strategy = self.current_strategy.clone();
        let last_switch_at = self.last_switch_at.clone();

        tokio::spawn(async move {
            // Strategy switching logic would need to be handled differently
            // since we can't modify self from within this async block
            match *current_strategy.read().await {
                LoadBalanceStrategy::RoundRobin
                    if avg_response_time.read().await.gt(&threshold) =>
                {
                    *current_strategy.write().await = LoadBalanceStrategy::LeastConnection;
                    *last_switch_at.lock().unwrap() = Instant::now();
                }
                LoadBalanceStrategy::LeastConnection
                    if avg_response_time.read().await.lt(&threshold) =>
                {
                    *current_strategy.write().await = LoadBalanceStrategy::RoundRobin;
                    *last_switch_at.lock().unwrap() = Instant::now();
                }
                _ => (),
            }
        });

        self.inner.call(req)
    }
}
