use color_eyre::Result;
use loadbalancer::Application;
use loadbalancer::prod::APP_ADDRESS;
use std::sync::Arc;
use tokio::time::interval;

#[tokio::main]
async fn main() -> Result<()> {
    let app = Arc::new(
        Application::new(APP_ADDRESS)
            .await
            .expect("failed to create app"),
    );
    let mut interval = interval(
        app.load_balancer
            .read()
            .await
            .config
            .server_config
            .health_check_interval,
    );
    let app_clone = app.clone();
    // We start a loop to continuously accept incoming connections
    tokio::spawn(async move {
        loop {
            interval.tick().await;
            if let Err(e) = app_clone.run_health_check().await {
                println!("Error running health check: {:?}", e);
            }
        }
    });

    app.run().await
}
