use crate::helper::TestApp;
use std::net::TcpListener;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn canary_test() {
    // Start a background HTTP server on a random local port
    let listener = TcpListener::bind("0.0.0.0:3000").unwrap();
    let mock_server = MockServer::builder().listener(listener).start().await;

    let listener = TcpListener::bind("0.0.0.0:3001").unwrap();
    let mock_server2 = MockServer::builder().listener(listener).start().await;

    // Arrange the behaviour of the MockServer adding a Mock:
    // when it receives a GET request on '/hello' it will respond with a 200.
    Mock::given(method("GET"))
        .and(path("/"))
        .respond_with(ResponseTemplate::new(200))
        // .expect(1)
        // Mounting the mock on the mock server - it's now effective!
        .mount(&mock_server)
        .await;
    Mock::given(method("GET"))
        .and(path("/"))
        .respond_with(ResponseTemplate::new(200))
        // .expect(1)
        // Mounting the mock on the mock server - it's now effective!
        .mount(&mock_server2)
        .await;

    let app = TestApp::new().await;

    let address = format!("http://{}", app.address.clone());

    app.http_client.get(address.as_str()).send().await.expect("HTTP request failed");
    app.http_client.get(address.as_str()).send().await.expect("HTTP request failed");

    assert_eq!(mock_server.received_requests().await.unwrap().len(), 1);
    assert_eq!(mock_server2.received_requests().await.unwrap().len(), 1);
}