use reqwest::Client;
use std::sync::Arc;
use trailblazer::Application;
use trailblazer::test::APP_ADDRESS;

pub struct TestApp {
    pub http_client: Client,
    pub address: String,
}
impl TestApp {
    pub async fn new() -> Self {
        let app = Arc::new(
            Application::new(APP_ADDRESS)
                .await
                .expect("failed to create app"),
        );
        // Run the auth service in a separate async task
        // to avoid blocking the main test thread.
        for worker in app.workers.clone() {
            tokio::spawn(async move { worker.run().await });
        }

        let app_clone = app.clone();
        #[allow(clippy::let_underscore_future)]
        let _ = tokio::spawn(async move { app_clone.run().await });

        let address = app.listener.local_addr().unwrap().to_string();

        let http_client = Client::builder().build().unwrap(); // Create a Reqwest http client instance

        Self {
            http_client,
            address,
        }
    }
}
