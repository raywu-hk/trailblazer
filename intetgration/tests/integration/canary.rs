use crate::helper::TestApp;
use futures::future::join_all;
use reqwest::Client;

#[tokio::test(flavor = "multi_thread", worker_threads = 10)]
async fn canary_test() {
    // // Start a background HTTP server on a random local port
    // let listener = TcpListener::bind("0.0.0.0:3000").unwrap();
    // let mock_server = MockServer::builder().listener(listener).start().await;
    //
    // let listener = TcpListener::bind("0.0.0.0:3001").unwrap();
    // let mock_server2 = MockServer::builder().listener(listener).start().await;
    //
    // // Arrange the behaviour of the MockServer adding a Mock:
    // // when it receives a GET request on '/hello' it will respond with a 200.
    // Mock::given(method("GET"))
    //     .and(path("/"))
    //     .respond_with(ResponseTemplate::new(200))
    //     // .expect(1)
    //     // Mounting the mock on the mock server - it's now effective!
    //     .mount(&mock_server)
    //     .await;
    // Mock::given(method("GET"))
    //     .and(path("/"))
    //     .respond_with(ResponseTemplate::new(200))
    //     // .expect(1)
    //     // Mounting the mock on the mock server - it's now effective!
    //     .mount(&mock_server2)
    //     .await;

    let test_app = TestApp::new().await;
    let address = format!("http://{}/health", test_app.lb_address.clone());
    let worker = test_app.worker.clone();
    tokio::spawn(async move { worker.run().await });
    let load_balancer = test_app.load_balancer.clone();
    tokio::spawn(async move { load_balancer.run().await });

    let handles: Vec<_> = (0..20)
        .map(|_| {
            let client = Client::new();
            let uri = address.clone();
            tokio::spawn(async move { client.get(uri).send().await.unwrap().status().is_success() })
        })
        .collect();
    let results = join_all(handles).await;

    assert_eq!(
        results
            .iter()
            .all(|http_status_success| match http_status_success {
                Ok(success) => success.eq(&true),
                _ => false,
            }),
        true
    );
    // assert_eq!(mock_server.received_requests().await.unwrap().len(), 1);
    // assert_eq!(mock_server2.received_requests().await.unwrap().len(), 1);
}
