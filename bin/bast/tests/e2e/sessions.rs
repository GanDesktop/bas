use super::support::{Case, completion, session_id};
use bas_client::service::pb;

#[tokio::test]
async fn conversation_survives_client_disconnect_archive_and_daemon_restart() {
    let case = Case::new("sessions");
    let mut daemon = case.start("first", &[]).await;
    let created = case
        .ok(
            &daemon,
            "create",
            &["session", "create", "--title", "conversation"],
        )
        .await;
    let id = session_id(&created);
    let turn = bas_client::model::TurnId::new().to_string();
    let prompt = [
        "prompt",
        id,
        "--submission",
        &turn,
        "hello from a disconnected CLI",
    ];
    case.ok(&daemon, "prompt", &prompt).await;
    let completed = completion(&mut daemon.client, id, 0).await;
    case.record("completion.txt", &completed);
    assert_eq!(completed.turn.as_deref(), Some(turn.as_str()));
    let before = case.ok(&daemon, "history-before", &["history", id]).await;
    assert!(before.contains("hello from a disconnected CLI"));
    assert_eq!(
        case.ok(&daemon, "retry", &prompt).await,
        format!("turn {turn}\n")
    );
    assert_eq!(
        case.ok(&daemon, "history-after-retry", &["history", id])
            .await,
        before
    );
    let conflict = case
        .cli(
            &daemon,
            "conflict",
            &["prompt", id, "--submission", &turn, "different input"],
        )
        .await;
    assert!(!conflict.status.success());
    assert!(String::from_utf8_lossy(&conflict.stderr).contains("Conflict"));
    case.ok(&daemon, "archive", &["session", "archive", id])
        .await;
    daemon.stop().await;

    let mut daemon = case.start("restarted", &[]).await;
    assert!(
        !case
            .ok(&daemon, "visible", &["session", "list"])
            .await
            .contains(id)
    );
    assert!(
        case.ok(&daemon, "archived", &["session", "list", "--all"])
            .await
            .contains(id)
    );
    let recovered = case
        .ok(&daemon, "history-restarted", &["history", id])
        .await;
    assert!(recovered.starts_with(&before));
    assert_eq!(
        case.ok(&daemon, "receipt", &["prompt-status", id, &turn])
            .await,
        format!("turn {turn}\n")
    );
    case.ok(&daemon, "restore", &["session", "restore", id])
        .await;
    let snapshot = daemon
        .client
        .get_session(pb::GetSessionRequest { session: id.into() })
        .await
        .expect("snapshot")
        .into_inner()
        .session
        .expect("session");
    case.ok(&daemon, "continued", &["prompt", id, "after restart"])
        .await;
    case.record(
        "continued-result.txt",
        completion(&mut daemon.client, id, snapshot.revision).await,
    );
    let history = case.ok(&daemon, "history-final", &["history", id]).await;
    assert!(history.contains("hello from a disconnected CLI"));
    assert!(history.contains("after restart"));
    daemon.stop().await;
}
