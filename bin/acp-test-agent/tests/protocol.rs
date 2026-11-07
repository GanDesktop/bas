use agent_client_protocol::schema::{ProtocolVersion, v1, v2};
use agent_client_protocol::{AcpAgent, AcpAgentConfig, Client, Error, ErrorCode};
use std::{
    fs::File,
    io::Write,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};

struct Case {
    root: PathBuf,
}

impl Case {
    fn new(name: &str) -> Self {
        let base = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/e2e");
        std::fs::create_dir_all(&base).expect("artifact directory");
        let base = base.canonicalize().expect("absolute artifact directory");
        let root = tempfile::Builder::new()
            .prefix(name)
            .tempdir_in(base)
            .expect("isolated case")
            .keep();
        eprintln!("artifacts: {}", root.display());
        Self { root }
    }

    fn agent(&self) -> AcpAgent {
        AcpAgent::new(
            AcpAgentConfig::new(env!("CARGO_BIN_EXE_acp-test-agent")).args(["--require-auth"]),
        )
    }

    fn record(&self, name: &str, value: impl std::fmt::Debug) {
        std::fs::write(self.root.join(name), format!("{value:#?}")).expect("result artifact");
    }
}

#[tokio::test]
async fn v1_authentication_prompt_and_history_replay() {
    let case = Case::new("v1-");
    let log = Arc::new(Mutex::new(
        File::create(case.root.join("updates.jsonl")).expect("notification log"),
    ));
    tokio::time::timeout(
        Duration::from_secs(20),
        Client
            .builder()
            .on_receive_notification(
                async move |notification: v1::SessionNotification, _cx| {
                    writeln!(
                        log.lock().expect("log lock"),
                        "{}",
                        serde_json::to_string(&notification).expect("notification JSON")
                    )
                    .expect("write notification");
                    Ok(())
                },
                agent_client_protocol::on_receive_notification!(),
            )
            .connect_with(case.agent(), async |cx| {
                cx.send_request(v1::InitializeRequest::new(ProtocolVersion::V1))
                    .block_task()
                    .await?;
                let rejected = cx
                    .send_request(v1::NewSessionRequest::new(case.root.clone()))
                    .block_task()
                    .await
                    .expect_err("login required");
                assert_eq!(rejected.code, ErrorCode::AuthRequired);
                cx.send_request(v1::AuthenticateRequest::new("demo-login"))
                    .block_task()
                    .await?;
                let session = cx
                    .send_request(v1::NewSessionRequest::new(case.root.clone()))
                    .block_task()
                    .await?;
                case.record("session.txt", &session);
                let result = cx
                    .send_request(v1::PromptRequest::new(
                        session.session_id.clone(),
                        vec!["/echo hello".into()],
                    ))
                    .block_task()
                    .await?;
                case.record("prompt.txt", &result);
                assert_eq!(result.stop_reason, v1::StopReason::EndTurn);
                cx.send_request(v1::LoadSessionRequest::new(
                    session.session_id,
                    case.root.clone(),
                ))
                .block_task()
                .await?;
                cx.send_request(v1::LogoutRequest::new())
                    .block_task()
                    .await?;
                assert_eq!(
                    cx.send_request(v1::NewSessionRequest::new(case.root.clone()))
                        .block_task()
                        .await
                        .expect_err("logged out")
                        .code,
                    ErrorCode::AuthRequired
                );
                Ok::<(), Error>(())
            }),
    )
    .await
    .expect("scenario deadline")
    .expect("v1 scenario");
    let updates =
        std::fs::read_to_string(case.root.join("updates.jsonl")).expect("observed updates");
    let text: String = updates
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).expect("notification JSON"))
        .filter_map(|value| {
            value["update"]["content"]["text"]
                .as_str()
                .map(str::to_owned)
        })
        .collect();
    assert_eq!(text, "hellohello", "live output followed by load replay");
}

#[tokio::test]
async fn v2_authentication_and_session_lifecycle() {
    let case = Case::new("v2-");
    tokio::time::timeout(
        Duration::from_secs(20),
        Client.v2().connect_with(case.agent(), async |cx| {
            cx.send_request(v2::InitializeRequest::new(
                ProtocolVersion::V2,
                v2::Implementation::new("e2e", "1"),
            ))
            .block_task()
            .await?;
            assert_eq!(
                cx.send_request(v2::NewSessionRequest::new(case.root.clone()))
                    .block_task()
                    .await
                    .expect_err("login required")
                    .code,
                ErrorCode::AuthRequired
            );
            cx.send_request(v2::LoginAuthRequest::new("demo-login"))
                .block_task()
                .await?;
            let session = cx
                .send_request(v2::NewSessionRequest::new(case.root.clone()))
                .block_task()
                .await?;
            case.record("session.txt", &session);
            let listed = cx
                .send_request(v2::ListSessionsRequest::new())
                .block_task()
                .await?;
            case.record("sessions.txt", &listed);
            assert_eq!(listed.sessions.len(), 1);
            assert_eq!(listed.sessions[0].session_id, session.session_id);
            cx.send_request(v2::DeleteSessionRequest::new(session.session_id))
                .block_task()
                .await?;
            assert!(
                cx.send_request(v2::ListSessionsRequest::new())
                    .block_task()
                    .await?
                    .sessions
                    .is_empty()
            );
            cx.send_request(v2::LogoutAuthRequest::new())
                .block_task()
                .await?;
            assert_eq!(
                cx.send_request(v2::NewSessionRequest::new(case.root.clone()))
                    .block_task()
                    .await
                    .expect_err("logged out")
                    .code,
                ErrorCode::AuthRequired
            );
            Ok::<(), Error>(())
        }),
    )
    .await
    .expect("scenario deadline")
    .expect("v2 scenario");
}
