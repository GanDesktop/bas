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
            callbacks_only: false,
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
            .observe_acp(pb::AttachRequest {
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
            let message: serde_json::Value =
                serde_json::from_slice(&event.json).expect("protocol JSON");
            if message.get("result").is_some() {
                assert_eq!(
                    event.turn.as_deref(),
                    Some(submission.turn.to_string().as_str())
                );
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
async fn prepared_stream_is_single_use_scoped_and_allows_multiple_subscribers() {
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
    let competing = connect(&server.client, &session, true).await;
    assert_ne!(link.prepared.id, competing.prepared.id);
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

async fn interactive(
    client: &Client,
    session: &crate::session::Session,
    elicitation: bool,
) -> (Link, tokio::sync::mpsc::Receiver<Incoming>) {
    let prepared = prepare(
        client.channel(),
        pb::PrepareAcpRequest {
            callbacks_only: false,
            target: Some(pb::prepare_acp_request::Target::Session(
                session.id().to_string(),
            )),
            interactive: true,
        },
    )
    .await
    .expect("prepare subscriber");
    let (link, events) = Link::open(prepared).await.expect("open subscriber");
    let capabilities = if elicitation {
        acp::ClientCapabilities::new().elicitation(
            acp::ElicitationCapabilities::new().form(acp::ElicitationFormCapabilities::new()),
        )
    } else {
        acp::ClientCapabilities::new()
    };
    link.peer
        .send_request(
            acp::InitializeRequest::new(sdk::schema::ProtocolVersion::V1)
                .client_capabilities(capabilities),
        )
        .block_task()
        .await
        .expect("initialize");
    link.peer
        .send_request(acp::LoadSessionRequest::new(
            session.id().to_string(),
            session.meta().cwd.expect("cwd"),
        ))
        .block_task()
        .await
        .expect("load");
    (link, events)
}

async fn callback(events: &mut tokio::sync::mpsc::Receiver<Incoming>) -> sdk::Responder {
    loop {
        if let Incoming::Request(_, responder) = events.recv().await.expect("callback stream") {
            return responder;
        }
    }
}

#[tokio::test]
async fn interactive_subscribers_share_requests_without_owning_the_agent() {
    tokio::time::timeout(Duration::from_secs(10), async {
        let registry = registry(100);
        let session = create(&registry).await;
        let server = serve(registry.clone()).await;
        let (first, mut first_events) = interactive(&server.client, &session, false).await;
        let (_second, mut second_events) = interactive(&server.client, &session, true).await;
        let host = registry.permissions().host(Arc::new(LocalHost::default()));
        let id = session.id();
        let answer = tokio::spawn(async move { host.request_permission(super::permissions::approval_request(id)).await });
        let first_reply = callback(&mut first_events).await;
        let second_reply = callback(&mut second_events).await;
        drop(first);
        drop(first_events);
        drop(first_reply);
        second_reply.respond_with_result(Ok(serde_json::json!({"outcome":{"outcome":"selected","optionId":"allow"}}))).expect("answer from remaining frontend");
        assert_eq!(answer.await.expect("task").expect("answer"), acp::RequestPermissionOutcome::Selected(acp::SelectedPermissionOutcome::new("allow")));

        let (_third, mut third_events) = interactive(&server.client, &session, false).await;
        let host = registry.permissions().host(Arc::new(LocalHost::default()));
        let answer = tokio::spawn(async move { host.request_permission(super::permissions::approval_request(id)).await });
        let second_reply = callback(&mut second_events).await;
        let third_reply = callback(&mut third_events).await;
        second_reply.respond_with_result(Ok(serde_json::json!({"outcome":{"outcome":"selected","optionId":"allow"}}))).expect("first answer");
        assert_eq!(answer.await.expect("task").expect("answer"), acp::RequestPermissionOutcome::Selected(acp::SelectedPermissionOutcome::new("allow")));
        let _ = third_reply.respond_with_result(Ok(serde_json::json!({"outcome":{"outcome":"cancelled"}})));

        let interactions = registry.elicitations().clone();
        let connection = uuid::Uuid::new_v4();
        interactions.open_connection(id, connection);
        let answer = tokio::spawn(async move { interactions.request(id, connection, serde_json::json!({"sessionId":id.to_string(),"mode":"form","message":"Name?","requestedSchema":{"type":"object","properties":{"name":{"type":"string"}},"required":["name"]}})).await });
        callback(&mut second_events).await.respond_with_result(Ok(serde_json::json!({"action":"accept","content":{"name":"BAS"}}))).expect("supported frontend answers");
        assert_eq!(answer.await.expect("task").expect("answer")["action"], "accept");
        registry.shutdown().await;
    }).await.expect("multiplexing deadline");
}

#[tokio::test]
async fn management_and_protocol_observers_are_disjoint() {
    tokio::time::timeout(Duration::from_secs(10), async {
        let registry = registry(1000);
        let session = create(&registry).await;
        let mut server = serve(registry.clone()).await;
        let request = pb::AttachRequest {
            session: session.id().to_string(),
            cursor: Some(session.meta().revision.raw()),
            replay: 0,
        };
        let mut management = server
            .client
            .attach(request.clone())
            .await
            .expect("management")
            .into_inner();
        let mut first = server
            .client
            .observe_acp(request.clone())
            .await
            .expect("first observer")
            .into_inner();
        let mut second = server
            .client
            .observe_acp(request)
            .await
            .expect("second observer")
            .into_inner();
        let submission = PromptSubmission::new(acp::PromptRequest::new(
            session.id().to_string(),
            vec!["first block".into(), "second block".into()],
        ));
        let PromptSubmissionOutcome::Accepted(receipt) = server
            .client
            .submit_prompt(&submission)
            .await
            .expect("submit")
        else {
            panic!("accepted")
        };
        let mut messages = Vec::new();
        loop {
            let a = first.message().await.expect("first stream").expect("open");
            let b = second
                .message()
                .await
                .expect("second stream")
                .expect("open");
            assert_eq!(
                a, b,
                "observers receive the same protocol message and position"
            );
            let message: serde_json::Value = serde_json::from_slice(&a.json).expect("JSON-RPC");
            assert_eq!(message["jsonrpc"], "2.0");
            assert!(message.get("kind").is_none());
            let done = message.get("result").is_some();
            messages.push((a, message));
            if done {
                break;
            }
        }
        let (input, message) = &messages[0];
        assert_eq!(message["method"], "session/prompt");
        assert_eq!(input.start, receipt.cursor.raw());
        assert_eq!(input.history_events().expect("history projection").len(), 3);
        assert_eq!(message["id"], messages.last().expect("response").1["id"]);
        assert_eq!(
            message["params"],
            serde_json::to_value(&submission.request).expect("request")
        );
        // A command/callback connection must not replay this conversation again.
        let prepared = prepare(
            server.client.channel(),
            pb::PrepareAcpRequest {
                target: Some(pb::prepare_acp_request::Target::Session(
                    session.id().to_string(),
                )),
                interactive: true,
                callbacks_only: true,
            },
        )
        .await
        .expect("callback preparation");
        let (callback, mut incoming) = Link::open(prepared).await.expect("callback link");
        callback
            .peer
            .send_request(acp::InitializeRequest::new(
                sdk::schema::ProtocolVersion::V1,
            ))
            .block_task()
            .await
            .expect("callback initialize");
        callback
            .peer
            .send_request(acp::LoadSessionRequest::new(
                session.id().to_string(),
                session.meta().cwd.expect("cwd"),
            ))
            .block_task()
            .await
            .expect("callback load without replay");
        assert!(
            incoming.try_recv().is_err(),
            "history is read only through the history/observation paths"
        );
        session.set_title("BAS title").await.expect("rename");
        loop {
            let event = management
                .message()
                .await
                .expect("management event")
                .expect("open");
            let event = bas::model::Event::try_from(event).expect("management model");
            assert!(!matches!(
                event.kind,
                bas::model::EventKind::Acp(_)
                    | bas::model::EventKind::Subagent(_)
                    | bas::model::EventKind::TurnStarted { .. }
                    | bas::model::EventKind::TurnFinished { .. }
            ));
            if matches!(event.kind, bas::model::EventKind::TitleChanged { .. }) {
                break;
            }
        }
        registry.shutdown().await;
    })
    .await
    .expect("stream separation deadline");
}
