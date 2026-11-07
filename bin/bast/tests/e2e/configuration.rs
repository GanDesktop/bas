use super::support::Case;
use bas_client::service::pb;

#[tokio::test]
async fn configuration_edits_are_persisted_and_stale_writes_are_rejected() {
    let case = Case::new("config");
    let mut daemon = case.start("daemon", &[]).await;
    let old = daemon.create().await;
    let baseline = daemon
        .client
        .get_agent_configuration(pb::GetAgentConfigurationRequest {})
        .await
        .expect("configuration")
        .into_inner();
    let edited = br#"[{"id":"replacement","kind":"embedded"}]"#.to_vec();
    let request = pb::SaveAgentConfigurationRequest {
        profile: baseline.profile,
        revision: baseline.revision,
        registrations_json: edited,
    };
    let saved = daemon
        .client
        .save_agent_configuration(request.clone())
        .await
        .expect("save configuration")
        .into_inner();
    case.record("saved.txt", &saved);
    let error = daemon
        .client
        .save_agent_configuration(request)
        .await
        .expect_err("stale revision");
    case.record("stale.txt", &error);
    assert_eq!(error.code(), tonic::Code::Aborted);
    assert_eq!(
        daemon
            .client
            .get_agent_configuration(pb::GetAgentConfigurationRequest {})
            .await
            .expect("read configuration")
            .into_inner(),
        saved
    );
    let agents = daemon
        .client
        .list_agents(pb::ListAgentsRequest {})
        .await
        .expect("published catalog")
        .into_inner();
    assert_eq!(agents.agents.len(), 1);
    assert_eq!(agents.agents[0].id, "replacement");
    let new = daemon
        .client
        .create_session(pb::CreateSessionRequest {
            agent: "replacement".into(),
            ..Default::default()
        })
        .await
        .expect("new agent")
        .into_inner()
        .session
        .expect("session");
    assert_eq!(new.agent, "replacement");
    assert_eq!(
        daemon
            .client
            .get_session(pb::GetSessionRequest { session: old.id })
            .await
            .expect("existing session remains")
            .into_inner()
            .session
            .expect("session")
            .agent,
        "embedded"
    );
    daemon.stop().await;
    let daemon = case.start("restart", &[]).await;
    assert!(
        case.ok(&daemon, "agents", &["agents"])
            .await
            .contains("replacement")
    );
    daemon.stop().await;
}
