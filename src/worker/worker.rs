use hyper::body::Incoming;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use std::error::Error;
use std::net::SocketAddr;
use std::str::FromStr;
use tokio::net::TcpListener;

pub struct Worker {
    pub socket_addr: SocketAddr,
    name: usize,
}

impl Worker {
    pub fn new(address: &str, name: usize) -> Self {
        Self {
            socket_addr: SocketAddr::from_str(address).expect("Worker ip address invalid"),
            name,
        }
    }
    pub async fn run(&self) -> Result<(), Box<dyn Error + Send + Sync + 'static>> {
        let listener = TcpListener::bind(self.socket_addr).await?;
        let worker_name = self.name;
        // We start a loop to continuously accept incoming connections
        loop {
            let (stream, _) = listener.accept().await?;

            // Use an adapter to access something implementing `tokio::io` traits as if they implement
            // `hyper::rt` IO traits.
            let io = TokioIo::new(stream);

            // Spawn a tokio task to serve multiple connections concurrently
            tokio::task::spawn(async move {
                // Finally, we bind the incoming connection to our `hello` service
                if let Err(err) = http1::Builder::new()
                    // `service_fn` converts our function in a `Service`
                    .serve_connection(io, service_fn(|req| Self::handle(req, worker_name)))
                    .await
                {
                    eprintln!("Error serving connection: {:?}", err);
                }
            });
        }
    }

    async fn handle(
        req: Request<Incoming>,
        worker_name: usize,
    ) -> Result<Response<String>, Box<dyn Error + Send + Sync>> {
        match req.uri().path() {
            "/health" => {
                println!("Health check worker {:?}", worker_name);
                Ok(Response::builder()
                    .status(StatusCode::OK)
                    .body("".to_string())?)
            }
            _ => {
                println!("Other worker {:?}", worker_name);
                Ok(Response::builder()
                    .status(StatusCode::OK)
                    .body("".to_string())?)
            }
        }
    }
}
