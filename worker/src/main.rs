use worker::Application;

#[tokio::main(flavor = "multi_thread", worker_threads = 10)]
async fn main() {
    let app = Application::new();
    app.run().await
}
