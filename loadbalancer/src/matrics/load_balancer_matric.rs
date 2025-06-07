use hyper::body::{Body, Incoming};
use hyper::service::Service;
use hyper::{Request, Response};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tokio::time::Instant;

#[derive(Debug, Clone)]
pub struct LoadBalancerMetrics<S> {
    inner: S,
    pub avg_response_time: Arc<AtomicU64>,
}

impl<S> LoadBalancerMetrics<S> {
    pub(crate) fn new(inner: S) -> Self {
        Self {
            inner,
            avg_response_time: Arc::new(AtomicU64::new(0)),
        }
    }
}
impl<S, B> Service<Request<Incoming>> for LoadBalancerMetrics<S>
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
        let avg_response_time = self.avg_response_time.clone();
        Box::pin(async move {
            let response = future.await;
            let new_response_time = start_time.elapsed();
            let current_avg = avg_response_time.load(Ordering::Relaxed);
            let new_avg_response_time = if current_avg == 0 {
                new_response_time.as_nanos() as u64
            } else {
                (current_avg + new_response_time.as_nanos() as u64) / 2
            };
            avg_response_time.store(new_avg_response_time, Ordering::Relaxed);
            println!(
                "Avg response time: {} ms",
                Duration::from_nanos(new_avg_response_time).as_millis()
            );
            response
        })
    }
}
