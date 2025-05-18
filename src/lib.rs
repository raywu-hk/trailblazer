use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::serve::Serve;
use axum::Router;
use std::error::Error;
use tokio::net::TcpListener;

mod constants;
pub use constants::*;
pub struct Application {
    server: Serve<TcpListener, Router, Router>,
    pub address: String,
}

impl Application {
    pub async fn new(address: &str) -> Result<Self, Box<dyn Error>> {
        let router = Router::new()
            .route("/", get(canary));
        let listener = TcpListener::bind(address).await?;
        let address = listener.local_addr()?.to_string();
        let server = axum::serve(listener, router);
        let app = Self { server, address };
        Ok(app)
    }
    pub async fn run(self) -> Result<(), std::io::Error> {
        self.server.await
    }
}

pub async fn canary() -> impl IntoResponse {
    StatusCode::OK
}