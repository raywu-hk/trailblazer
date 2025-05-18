use reqwest::StatusCode;
use trailblazer::test::APP_ADDRESS;
use trailblazer::Application;

#[tokio::test]
async fn canary() {
    let app = Application::new(APP_ADDRESS).await.expect("Failed to create application");
    let server_address = app.address.clone();
    #[allow(clippy::let_underscore_future)]
    let _ = tokio::spawn(app.run());

    let client = reqwest::Client::builder().build().unwrap();
    let address = format!("http://{}", server_address);

    let result = client.get(address).send().await.unwrap();
    assert_eq!(result.status(), StatusCode::OK)
}