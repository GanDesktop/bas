use super::support::{Case, DEADLINE};
use bas_client::service::{
    acp::{Link, prepare, sdk},
    pb,
};
use bas_client::{
    acp,
    model::{PromptSubmission, PromptSubmissionOutcome},
};

#[tokio::test]
async fn acp_client_loads_a_bas_session_and_cannot_address_another() {
    let case = Case::new("gateway");
    let mut daemon = case.start("daemon", &[]).await;
    let session = daemon.create().await;
    let other = daemon.create().await;
    let prepared = prepare(
        daemon.client.channel(),
        pb::PrepareAcpRequest {
            target: Some(pb::prepare_acp_request::Target::Session(session.id.clone())),
            interactive: false,
            callbacks_only: false,
        },
    )
    .await
    .expect("prepare ACP");
    let (link, _updates) = Link::open(prepared).await.expect("open ACP");
    let response = link
        .peer
        .send_request(acp::InitializeRequest::new(
            sdk::schema::ProtocolVersion::V1,
        ))
        .block_task()
        .await
        .expect("initialize");
    assert!(response.agent_capabilities.load_session);
    link.peer
        .send_request(acp::LoadSessionRequest::new(
            session.id.clone(),
            session.cwd.expect("cwd"),
        ))
        .block_task()
        .await
        .expect("load session");
    let response = tokio::time::timeout(
        DEADLINE,
        link.peer
            .send_request(acp::PromptRequest::new(
                session.id.clone(),
                vec!["ACP hello".into()],
            ))
            .block_task(),
    )
    .await
    .expect("prompt deadline")
    .expect("ACP prompt");
    case.record("result.txt", &response);
    assert_eq!(response.stop_reason, acp::StopReason::EndTurn);
    let error = link
        .peer
        .send_request(acp::PromptRequest::new(
            other.id,
            vec!["wrong session".into()],
        ))
        .block_task()
        .await
        .expect_err("connection scope");
    case.record("scope-error.txt", &error);
    assert_eq!(error.code, acp::ErrorCode::InvalidParams);
    drop(link);
    let cursor = daemon
        .client
        .get_session(pb::GetSessionRequest {
            session: session.id.clone(),
        })
        .await
        .expect("snapshot")
        .into_inner()
        .session
        .expect("session")
        .revision;
    let submission = PromptSubmission::new(acp::PromptRequest::new(
        session.id.clone(),
        vec!["still available".into()],
    ));
    let result = daemon
        .client
        .submit_prompt(&submission)
        .await
        .expect("submit after disconnect");
    assert!(matches!(result, PromptSubmissionOutcome::Accepted(_)));
    case.record(
        "after-disconnect.txt",
        super::support::completion(&mut daemon.client, &session.id, cursor).await,
    );
    daemon.stop().await;
}
