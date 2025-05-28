use std::sync::Arc;
use trailblazer::Application;
use trailblazer::test::APP_ADDRESS;

pub struct TestApp {
    pub app: Arc<Application>,
    pub address: String,
}
impl TestApp {
    pub async fn new() -> Self {
        let app = Arc::new(
            Application::new(APP_ADDRESS)
                .await
                .expect("failed to create app"),
        );

        let address = app.listener.local_addr().unwrap().to_string();

        Self { app, address }
    }
}
