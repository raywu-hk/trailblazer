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
    pub async fn run_worker(&self) {
        // Run the auth service in a separate async task
        // to avoid blocking the main test thread.
        for worker in self.app.workers.clone() {
            tokio::spawn(async move { worker.run().await });
        }
    }
}
