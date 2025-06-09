use crate::metrics::metrics::Metrics;
use hyper::body::{Body, Incoming};
use hyper::service::Service;
use hyper::{Request, Response};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use tokio::time::Instant;

#[derive(Debug, Clone)]
pub struct LoadBalancerMetricsLayer<S> {
    inner: S,
    pub matrics: Arc<Metrics>,
}

impl<S> LoadBalancerMetricsLayer<S> {
    pub fn new(inner: S, matrics: Arc<Metrics>) -> Self {
        Self { inner, matrics }
    }
}
impl<S, B> Service<Request<Incoming>> for LoadBalancerMetricsLayer<S>
where
    S: Service<Request<Incoming>, Response = Response<B>> + Send + Sync,
    S::Future: Send + 'static,
    S::Error: Send,
    B: Body + Send,
{
    type Response = Response<B>;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn call(&self, req: Request<Incoming>) -> Self::Future {
        let start_time = Instant::now();
        let future = self.inner.call(req);
        let matrics = self.matrics.clone();
        Box::pin(async move {
            let response = future.await;
            let response_time = start_time.elapsed();
            matrics.record_response_time(response_time);
            response
        })
    }
}
