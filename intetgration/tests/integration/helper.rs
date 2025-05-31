use loadbalancer::prod::APP_ADDRESS;
use std::sync::Arc;

pub struct TestApp {
    pub load_balancer: Arc<loadbalancer::Application>,
    pub worker: Arc<worker::Application>,
    pub lb_address: String,
}
impl TestApp {
    pub async fn new() -> Self {
        let load_balancer = Arc::new(
            loadbalancer::Application::new(APP_ADDRESS)
                .await
                .expect("failed to create app"),
        );

        let worker = Arc::new(worker::Application::new());

        let address = &*load_balancer.listener.local_addr().unwrap().to_string();

        Self {
            load_balancer,
            worker,
            lb_address: address.to_string(),
        }
    }

    pub async fn run(&mut self) {}
}
