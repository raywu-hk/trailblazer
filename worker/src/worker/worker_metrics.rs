use crate::worker::Metrics;
use hyper::body::{Body, Incoming};
use hyper::service::Service;
use hyper::{Request, Response};
use std::pin::Pin;
use std::sync::Arc;
use tokio::time::Instant;
// use std::time::Duration;

#[derive(Debug, Clone)]
pub struct WorkerMetrics<S> {
    inner: S,
    metrics: Arc<Metrics>,
}

impl<S> WorkerMetrics<S> {
    pub fn new(inner: S, metrics: Arc<Metrics>) -> Self {
        Self { inner, metrics }
    }
}

impl<S, B> Service<Request<Incoming>> for WorkerMetrics<S>
where
    S: Service<Request<Incoming>, Response = Response<B>>,
    S::Future: Send + 'static,
    B: Body + Send,
{
    type Response = Response<B>;
    type Error = S::Error;
    // type Future = S::Future;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn call(&self, mut req: Request<Incoming>) -> Self::Future {
        let start_time = Instant::now();
        req.extensions_mut().insert(self.metrics.clone());
        let future = self.inner.call(req);
        let metrics = self.metrics.clone();
        Box::pin(async move {
            let response = future.await;
            let response_time = start_time.elapsed();
            metrics.record_response_time(response_time);
            response
        })
    }
    /*
    fn call(&self, req: Request<Incoming>) -> Self::Future {
        let mut req = req;
        req.extensions_mut().insert(self.connection.clone());


        let future = self.inner.call(req);
        Box::pin(async move {
            let response = future.await;
            response
        })
    }
    */
}
