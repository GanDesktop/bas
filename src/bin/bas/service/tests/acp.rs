use super::*;
use bas::{
    acp,
    model::{PromptSubmission, PromptSubmissionOutcome},
    service::{
        Client,
        acp::{Incoming, Link, prepare, sdk, server::Gateway},
    },
};

struct Server {
    task: tokio::task::JoinHandle<()>,
    client: Client,
}
impl Drop for Server {
    fn drop(&mut self) {
        self.task.abort();
    }
}
async fn serve(registry: Arc<Registry>) -> Server {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener");
    let address = listener.local_addr().expect("address");
    let service = Service::new(registry);
    let gateway = Gateway::new(service.clone()).with_authentication(service.clone());
    let task = tokio::spawn(async move {
        tonic::transport::Server::builder()
            .add_service(service.into_server())
            .add_service(gateway.into_server())
            .serve_with_incoming(tokio_stream::wrappers::TcpListenerStream::new(listener))
            .await
            .expect("server");
    });
    Server {
        task,
        client: Client::connect_at(address).await.expect("client"),
    }
}
async fn connect(client: &Client, session: &crate::session::Session, interactive: bool) -> Link {
    let prepared = prepare(
        client.channel(),
        pb::PrepareAcpRequest {
            target: Some(pb::prepare_acp_request::Target::Session(
                session.id().to_string(),
            )),
            interactive,
        },
    )
    .await
    .expect("prepare");
    let (link, mut events) = Link::open(prepared).await.expect("open");
    link.peer
        .spawn(async move {
            while let Some(event) = events.recv().await {
                match event {
                    Incoming::Notification(notification) => {
                        assert_eq!(notification.method, "session/update");
                    }
                    Incoming::Request(_, _) => panic!("no callback expected"),
                }
            }
            Ok(())
        })
        .expect("drain events");
    let initialized = link
        .peer
        .send_request(acp::InitializeRequest::new(
            sdk::schema::ProtocolVersion::V1,
        ))
        .block_task()
        .await
        .expect("initialize");
    assert!(initialized.agent_capabilities.load_session);
    let cwd = session.snapshot().meta.cwd.expect("session cwd");
    link.peer
        .send_request(acp::LoadSessionRequest::new(session.id().to_string(), cwd))
        .block_task()
        .await
        .expect("load");
    link
}

#[tokio::test]
async fn sdk_prompt_survives_disconnect_and_reconnect_without_redispatch() {
    tokio::time::timeout(Duration::from_secs(10), async {
        let registry = registry(1000);
        let session = create(&registry).await;
        let mut server = serve(registry.clone()).await;
        let link = connect(&server.client, &session, false).await;
        let submission = PromptSubmission::new(acp::PromptRequest::new(
            session.id().to_string(),
            vec!["background ".repeat(100).into()],
        ));
        let PromptSubmissionOutcome::Accepted(receipt) =
            link.submit(&submission).await.expect("receipt")
        else {
            panic!("accepted")
        };
        drop(link);
        let mut journal = server
            .client
            .attach(pb::AttachRequest {
                session: session.id().to_string(),
                cursor: Some(receipt.cursor.raw()),
                replay: 0,
            })
            .await
            .expect("journal")
            .into_inner();
        loop {
            let event = journal
                .message()
                .await
                .expect("journal event")
                .expect("running");
            if let Some(pb::event::Kind::TurnFinished(finished)) = event.kind {
                assert_eq!(finished.turn, submission.turn.to_string());
                assert!(matches!(
                    finished.outcome,
                    Some(pb::turn_finished::Outcome::Response(_))
                ));
                break;
            }
        }
        let link = connect(&server.client, &session, false).await;
        assert_eq!(
            link.submit(&submission).await.expect("idempotent receipt"),
            PromptSubmissionOutcome::Accepted(receipt)
        );
        let response = link
            .peer
            .send_request(acp::PromptRequest::new(
                session.id().to_string(),
                vec!["native SDK prompt".into()],
            ))
            .block_task()
            .await
            .expect("native response");
        assert_eq!(response.stop_reason, acp::StopReason::EndTurn);
        registry.shutdown().await;
    })
    .await
    .expect("end-to-end deadline");
}

#[tokio::test]
async fn prepared_stream_is_single_use_scoped_and_has_one_controller() {
    let registry = registry(100);
    let session = create(&registry).await;
    let other = create(&registry).await;
    let server = serve(registry.clone()).await;
    let link = connect(&server.client, &session, true).await;
    let mut reused = bas::service::acp::Prepared {
        id: link.prepared.id.clone(),
        control: link.prepared.control.clone(),
    };
    assert_eq!(
        reused.connect().await.err().expect("used ticket").code(),
        Code::NotFound
    );
    let mut competing = prepare(
        server.client.channel(),
        pb::PrepareAcpRequest {
            target: Some(pb::prepare_acp_request::Target::Session(
                session.id().to_string(),
            )),
            interactive: true,
        },
    )
    .await
    .expect("prepare second");
    assert_eq!(
        competing
            .connect()
            .await
            .err()
            .expect("single controller")
            .code(),
        Code::AlreadyExists
    );
    let error = link
        .peer
        .send_request(acp::PromptRequest::new(
            other.id().to_string(),
            vec!["wrong session".into()],
        ))
        .block_task()
        .await
        .expect_err("scoped request");
    assert_eq!(error.code, acp::ErrorCode::InvalidParams);
    drop(link);
    registry.shutdown().await;
}
