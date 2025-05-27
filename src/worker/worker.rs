use http_body_util::combinators::BoxBody;
use hyper::body::{Bytes, Incoming};
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use serde::Serialize;
use std::convert::Infallible;
use std::error::Error;
use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::sync::RwLock;

#[derive(Serialize)]
enum WorkerStatus {
    UP,
    DOWN,
}
#[derive(Serialize)]
struct HealthResponseBody {
    status: WorkerStatus,
    connection_count: usize,
}

#[derive(Clone)]
pub struct Worker {
    pub socket_addr: SocketAddr,
    connection_count: Arc<RwLock<usize>>,
}
impl Worker {
    pub fn new(address: &str) -> Self {
        Self {
            socket_addr: SocketAddr::from_str(address).expect("Worker ip address invalid"),
            connection_count: Arc::new(RwLock::new(0)),
        }
    }
    pub async fn run(&self) -> Result<(), Box<dyn Error + Send + Sync + 'static>> {
        let listener = TcpListener::bind(self.socket_addr).await?;
        // let worker_name = self.name;
        // We start a loop to continuously accept incoming connections

        loop {
            let (stream, _) = listener.accept().await?;
            println!("worker {} received request", self.socket_addr.port());
            // Use an adapter to access something implementing `tokio::io` traits as if they implement
            // `hyper::rt` IO traits.
            let io = TokioIo::new(stream);
            let connection_count = self.connection_count.clone();
            // Spawn a tokio task to serve multiple connections concurrently
            tokio::task::spawn(async move {
                *connection_count.write().await += 1;
                let count_snapshot = *connection_count.read().await;
                if let Err(err) = http1::Builder::new()
                    .serve_connection(
                        io,
                        service_fn(move |req| Self::handler(req, count_snapshot)),
                    )
                    .await
                {
                    eprintln!("Error serving connection: {:?}", err);
                };
                *connection_count.write().await -= 1;
            });
        }
    }
    async fn handler(
        req: Request<Incoming>,
        count_snapshot: usize,
    ) -> Result<Response<BoxBody<Bytes, Infallible>>, Box<dyn Error + Send + Sync>> {
        match req.uri().path() {
            "/health" => {
                // println!("Health check worker {:?}", worker_name);
                tokio::time::sleep(Duration::from_millis(10)).await;

                let res_body = HealthResponseBody {
                    status: WorkerStatus::UP,
                    connection_count: count_snapshot,
                };

                let json_body = serde_json::to_string(&res_body)?;

                let res = Response::builder()
                    .status(StatusCode::OK)
                    .body(BoxBody::new(json_body))?;

                Ok(res)
            }
            _ => {
                // println!("Other worker {:?}", worker_name);
                Ok(Response::builder()
                    .status(StatusCode::OK)
                    .body(BoxBody::new("".to_string()))?)
            }
        }
    }
}
