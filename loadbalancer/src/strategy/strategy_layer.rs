use crate::strategy::StrategyManager;
use hyper::body::Body;
use hyper::service::Service;
use hyper::{Request, Response};
use std::sync::Arc;

#[derive(Clone)]
pub struct StrategyLayer<S> {
    inner: S,
    strategy_manager: Arc<StrategyManager>,
}

impl<S> StrategyLayer<S> {
    pub fn new(inner: S, strategy_manager: Arc<StrategyManager>) -> Self {
        Self {
            inner,
            strategy_manager,
        }
    }
}

impl<S, B, ReqBody> Service<Request<ReqBody>> for StrategyLayer<S>
where
    S: Service<Request<ReqBody>, Response = Response<B>> + Send + Sync,
    S::Error: Send,
    B: Body + Send,
    ReqBody: Send,
{
    type Response = Response<B>;
    type Error = S::Error;
    type Future = S::Future;

    fn call(&self, req: Request<ReqBody>) -> Self::Future {
        self.strategy_manager.switch_strategy();
        self.inner.call(req)
    }
}
