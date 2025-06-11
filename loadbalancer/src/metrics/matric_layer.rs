use crate::metrics::metrics::Metrics;
use hyper::body::Body;
use hyper::service::Service;
use hyper::{Request, Response};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use tokio::time::Instant;

#[derive(Debug, Clone)]
pub struct MetricsLayer<S> {
    inner: S,
    pub matrics: Arc<Metrics>,
}

impl<S> MetricsLayer<S> {
    pub fn new(inner: S, matrics: Arc<Metrics>) -> Self {
        Self { inner, matrics }
    }
}
impl<S, B, ReqBody> Service<Request<ReqBody>> for MetricsLayer<S>
where
    S: Service<Request<ReqBody>, Response = Response<B>> + Send + Sync,
    S::Future: Send + 'static,
    S::Error: Send,
    B: Body + Send,
    ReqBody: Send,
{
    type Response = Response<B>;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn call(&self, req: Request<ReqBody>) -> Self::Future {
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
