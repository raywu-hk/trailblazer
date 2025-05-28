use std::error::Error;
use trailblazer::Application;
use trailblazer::test::APP_ADDRESS;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync + 'static>> {
    let app = Application::new(APP_ADDRESS)
        .await
        .expect("failed to create app");
    app.run_workers().await;
    app.run().await
}
