use std::error::Error;
use trailblazer::test::APP_ADDRESS;
use trailblazer::Application;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync + 'static>> {
    let app = Application::new(APP_ADDRESS).await.expect("failed to create app");

    app.run().await
}
