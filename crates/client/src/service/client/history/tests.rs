use super::*;

#[tokio::test]
async fn window_rpc_errors_are_propagated() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener");
    let address = listener.local_addr().expect("address");
    let task = tokio::spawn(async move {
        tonic::transport::Server::builder()
            .add_routes(tonic::service::Routes::default())
            .serve_with_incoming(tokio_stream::wrappers::TcpListenerStream::new(listener))
            .await
            .expect("server");
    });
    let mut client = Client::connect_at(address).await.expect("client");
    let result = client
        .history_window(SessionId::new(), None, 1, Duration::from_secs(5))
        .await;
    let overview = client
        .history_overview(SessionId::new(), Duration::from_secs(5))
        .await;
    let location = client
        .history_window_at(SessionId::new(), 1, Duration::from_secs(5))
        .await;
    task.abort();
    assert_eq!(
        result.expect_err("no history window RPC").code(),
        tonic::Code::Unimplemented
    );
    assert_eq!(
        overview.expect_err("no overview RPC").code(),
        tonic::Code::Unimplemented
    );
    assert_eq!(
        location.expect_err("no location RPC").code(),
        tonic::Code::Unimplemented
    );
}

#[tokio::test]
async fn each_history_window_rpc_bounds_a_stalled_request() {
    // A listening socket that never speaks HTTP/2 stalls every request without
    // requiring a second fake implementation of the complete Bas service.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener");
    let endpoint = tonic::transport::Endpoint::from_shared(format!(
        "http://{}",
        listener.local_addr().expect("address")
    ))
    .expect("endpoint");
    let mut client = Client::new(endpoint.connect_lazy());
    let session = SessionId::new();
    let timeout = Duration::from_millis(25);
    for before in [None, Some(8)] {
        let status = client
            .history_window(session, before, 1, timeout)
            .await
            .expect_err("stalled RPC must fail");
        assert_eq!(status.code(), tonic::Code::DeadlineExceeded);
    }
    assert_eq!(
        client
            .history_overview(session, timeout)
            .await
            .expect_err("stalled overview")
            .code(),
        tonic::Code::DeadlineExceeded
    );
    assert_eq!(
        client
            .history_window_at(session, 1, timeout)
            .await
            .expect_err("stalled location")
            .code(),
        tonic::Code::DeadlineExceeded
    );
    assert_eq!(
        client
            .history_window_at(session, 0, timeout)
            .await
            .expect_err("zero cursor rejected locally")
            .code(),
        tonic::Code::InvalidArgument
    );
}
