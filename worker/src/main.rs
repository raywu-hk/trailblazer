use worker::Application;

#[tokio::main]
async fn main() {
    let app = Application::new();
    app.run().await
}
