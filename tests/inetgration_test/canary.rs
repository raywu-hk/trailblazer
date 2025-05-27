use crate::helper::TestApp;
use futures::future::join_all;
use reqwest::Client;

#[tokio::test]
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
    let address = format!("http://{}/health", test_app.address.clone());
    let app = test_app.app.clone();

    tokio::spawn(async move { test_app.run_worker().await });
    tokio::spawn(async move { app.run().await });

    let handles: Vec<_> = (0..20)
        .map(|_| {
            let client = Client::new();
            let uri = address.clone();
            tokio::spawn(async move { client.get(uri).send().await.unwrap().status().is_success() })
        })
        .collect();
    let results = join_all(handles).await;

    assert_eq!(
        results.iter().all(|x| match x {
            Ok(success) => success.eq(&true),
            _ => false,
        }),
        true
    );
    // assert_eq!(mock_server.received_requests().await.unwrap().len(), 1);
    // assert_eq!(mock_server2.received_requests().await.unwrap().len(), 1);
}
