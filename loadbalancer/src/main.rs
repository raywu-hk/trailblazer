use loadbalancer::Application;
use loadbalancer::prod::APP_ADDRESS;
use std::error::Error;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::interval;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync + 'static>> {
    let app = Arc::new(
        Application::new(APP_ADDRESS)
            .await
            .expect("failed to create app"),
    );

    let app_clone = app.clone();
    tokio::spawn(async move {
        let mut interval = interval(Duration::from_secs(60));
        loop {
            interval.tick().await;
            if let Err(e) = app_clone.run_health_check().await {
                println!("Error running health check: {:?}", e);
            }
        }
    });

    app.run().await
}
