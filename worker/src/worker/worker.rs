use crate::worker::{Metrics, WorkerMetrics};
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
use tokio::net::TcpListener;
use tower::ServiceBuilder;

#[derive(Serialize)]
struct HealthResponseBody {
    connection_count: usize,
    avg_response_time: u64,
}

#[derive(Clone)]
pub struct Worker {
    pub socket_addr: SocketAddr,
    metrics: Arc<Metrics>,
}
impl Worker {
    pub fn new(address: &str) -> Self {
        Self {
            socket_addr: SocketAddr::from_str(address).expect("Worker ip address invalid"),
            metrics: Arc::new(Metrics::default()),
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
            self.metrics.record_connection();
            let metrics_for_avg_response_time = self.metrics.clone();
            let metrics_for_connection_count = self.metrics.clone();
            // Spawn a tokio task to serve multiple connections concurrently
            tokio::task::spawn(async move {
                let svc = hyper::service::service_fn(move |req| Self::handler(req, port));
                let svc = ServiceBuilder::new()
                    .layer_fn(move |service| {
                        WorkerMetrics::new(service, metrics_for_avg_response_time.clone())
                    })
                    .service(svc);
                let result = http1::Builder::new().serve_connection(io, svc).await;

                metrics_for_connection_count.release_connection();

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
                let metrics = req.extensions().get::<Arc<Metrics>>();

                let json_body = match metrics {
                    Some(metrics) => {
                        println!("Health check worker {:?}, metrics:{:?}", port, metrics);
                        let res_body = HealthResponseBody {
                            connection_count: metrics.get_connection_count() as usize,
                            avg_response_time: metrics.get_avg_response_time(),
                        };
                        serde_json::to_string(&res_body)?
                    }
                    None => "".to_owned(),
                };

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
