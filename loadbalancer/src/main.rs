use loadbalancer::Application;
use loadbalancer::prod::APP_ADDRESS;
use std::error::Error;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync + 'static>> {
    let app = Application::new(APP_ADDRESS)
        .await
        .expect("failed to create app");
    app.run().await
}
