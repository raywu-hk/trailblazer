use crate::worker::WorkerMatrics;
use http_body_util::combinators::BoxBody;
use hyper::body::{Bytes, Incoming};
use hyper::server::conn::http1;
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use serde::Serialize;
use std::convert::Infallible;
use std::error::Error;
use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::net::TcpListener;
use tower::ServiceBuilder;

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
    connection_count: Arc<AtomicUsize>,
}
impl Worker {
    pub fn new(address: &str) -> Self {
        Self {
            socket_addr: SocketAddr::from_str(address).expect("Worker ip address invalid"),
            connection_count: Arc::new(AtomicUsize::new(0)),
        }
    }
    pub async fn run(&self) -> Result<(), Box<dyn Error + Send + Sync>> {
        let listener = TcpListener::bind(self.socket_addr).await?;
        // let worker_name = self.name;
        // We start a loop to continuously accept incoming connections
        loop {
            let (stream, _) = listener.accept().await?;
            // Use an adapter to access something implementing `tokio::io` traits as if they implement
            // `hyper::rt` IO traits.
            let io = TokioIo::new(stream);
            let port = self.socket_addr.port();
            let connection_count = self.connection_count.clone();
            connection_count.fetch_add(1, Ordering::Relaxed);
            // Spawn a tokio task to serve multiple connections concurrently
            tokio::task::spawn(async move {
                let svc = hyper::service::service_fn(move |req| Self::handler(req, port));
                let connection_count_for_metrics = connection_count.clone();
                let svc = ServiceBuilder::new()
                    .layer_fn(move |service| {
                        WorkerMatrics::new(service, connection_count_for_metrics.clone())
                    })
                    .service(svc);
                let result = http1::Builder::new().serve_connection(io, svc).await;

                connection_count.fetch_sub(1, Ordering::Relaxed);

                if let Err(err) = result {
                    eprintln!("Error serving connection: {:?}", err);
                };
            });
        }
    }
    async fn handler(
        req: Request<Incoming>,
        port: u16,
    ) -> Result<Response<BoxBody<Bytes, Infallible>>, Box<dyn Error + Send + Sync>> {
        // let current_thread = thread::current();
        // println!("Thread ID: {:?}, Thread Name: {:?}",
        //          current_thread.id(),
        //          current_thread.name().unwrap_or("unnamed"));
        //
        // let delay = fastrand::u64(100..2000);
        // tokio::time::sleep(Duration::from_millis(delay)).await;
        // println!("Worker {} request", port);
        match req.uri().path() {
            "/health" => {
                let connection_count = req
                    .extensions()
                    .get::<Arc<AtomicUsize>>()
                    .map(|counter| counter.load(Ordering::Relaxed))
                    .unwrap_or(0);

                println!("Health check worker {:?}, con:{}", port, connection_count);
                let res_body = HealthResponseBody {
                    status: WorkerStatus::UP,
                    connection_count,
                };

                let json_body = serde_json::to_string(&res_body)?;

                let res = Response::builder()
                    .status(StatusCode::OK)
                    .body(BoxBody::new(json_body))?;
                Ok(res)
            }
            _ => {
                println!("Worker {} default path", port);
                Ok(Response::builder()
                    .status(StatusCode::OK)
                    .body(BoxBody::new("".to_string()))?)
            }
        }
    }
}
