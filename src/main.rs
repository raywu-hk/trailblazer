use trailblazer::prod::APP_ADDRESS;
use trailblazer::Application;

#[tokio::main]
async fn main() {
    let app = Application::new(APP_ADDRESS).await.expect("Failed to create application");
    app.run().await.expect("Failed to run application");
}
