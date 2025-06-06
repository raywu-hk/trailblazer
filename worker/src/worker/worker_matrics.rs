use hyper::body::{Body, Incoming};
use hyper::service::Service;
use hyper::{Request, Response};
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
// use std::time::Duration;

#[derive(Debug, Clone)]
pub struct WorkerMatrics<S> {
    inner: S,
    connection_count: Arc<AtomicUsize>,
}

impl<S> WorkerMatrics<S> {
    pub fn new(inner: S, connection_count: Arc<AtomicUsize>) -> Self {
        Self {
            inner,
            connection_count,
        }
    }
}

impl<S, B> Service<Request<Incoming>> for WorkerMatrics<S>
where
    S: Service<Request<Incoming>, Response = Response<B>>,
    // S::Future: Send + 'static,
    B: Body + Send,
{
    type Response = Response<B>;
    type Error = S::Error;
    type Future = S::Future;
    // type Future = Pin<Box<dyn Future<Output=Result<Self::Response, Self::Error>> + Send>>;

    fn call(&self, req: Request<Incoming>) -> Self::Future {
        let mut req = req;
        req.extensions_mut().insert(self.connection_count.clone());
        self.inner.call(req)
    }
}
