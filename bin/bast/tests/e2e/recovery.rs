use super::support::{Case, DEADLINE, completion, session_id};
use bas_client::service::pb;

#[tokio::test]
async fn committed_submission_survives_process_death_without_redispatch() {
    let case = Case::new("crash");
    let mut daemon = case.start("first", &[]).await;
    let created = case.ok(&daemon, "create", &["session", "create"]).await;
    let id = session_id(&created);
    let turn = bas_client::model::TurnId::new().to_string();
    let prompt = [
        "prompt",
        id,
        "--submission",
        &turn,
        "accepted before process death",
    ];
    case.ok(&daemon, "submit", &prompt).await;
    daemon
        .child
        .kill()
        .await
        .expect("kill and reap without shutdown");
    drop(daemon);
    let daemon = case.start("restart", &[]).await;
    let before = case
        .ok(&daemon, "recovered-history", &["history", id])
        .await;
    assert!(before.contains("accepted before process death"));
    assert_eq!(
        case.ok(&daemon, "receipt", &["prompt-status", id, &turn])
            .await,
        format!("turn {turn}\n")
    );
    assert_eq!(
        case.ok(&daemon, "resubmit", &prompt).await,
        format!("turn {turn}\n")
    );
    assert_eq!(
        case.ok(&daemon, "history-after-resubmit", &["history", id])
            .await,
        before
    );
    daemon.stop().await;
}

#[tokio::test]
async fn failed_storage_commit_is_recoverable_without_repeating_accepted_work() {
    let case = Case::new("storage");
    let mut daemon = case.start("daemon", &[]).await;
    let session = daemon.create().await;
    let id = &session.id;
    case.ok(&daemon, "prompt", &["prompt", id, "committed input"])
        .await;
    case.record("completed.txt", completion(&mut daemon.client, id, 0).await);
    let before = case.ok(&daemon, "history-before", &["history", id]).await;
    let writer =
        rusqlite::Connection::open(case.root.join("sessions.sqlite3")).expect("isolated database");
    writer.execute_batch("CREATE TRIGGER unavailable BEFORE INSERT ON history_chunks BEGIN SELECT RAISE(ABORT, 'disk full'); END;").expect("inject real commit failure");
    let error = daemon
        .client
        .set_title(pb::SetTitleRequest {
            session: id.clone(),
            title: "uncommitted".into(),
        })
        .await
        .expect_err("write fails");
    case.record("write-error.txt", &error);
    let failed = daemon
        .client
        .get_session(pb::GetSessionRequest {
            session: id.clone(),
        })
        .await
        .expect("failure snapshot")
        .into_inner()
        .session
        .expect("session");
    assert!(failed.runtime.as_ref().expect("runtime").failure.is_some());
    assert_ne!(failed.title.as_deref(), Some("uncommitted"));
    assert_eq!(
        case.ok(&daemon, "history-failed", &["history", id]).await,
        before
    );
    writer
        .execute_batch("DROP TRIGGER unavailable")
        .expect("restore database writes");
    drop(writer);
    case.ok(&daemon, "recover", &["session", "recover", id])
        .await;
    let recovered = daemon
        .client
        .get_session(pb::GetSessionRequest {
            session: id.clone(),
        })
        .await
        .expect("recovered snapshot")
        .into_inner()
        .session
        .expect("session");
    case.record("recovered.txt", &recovered);
    assert!(
        recovered
            .runtime
            .as_ref()
            .expect("runtime")
            .failure
            .is_none()
    );
    assert_ne!(
        recovered.runtime.as_ref().expect("runtime").id,
        failed.runtime.expect("failed runtime").id
    );
    case.ok(&daemon, "continue", &["prompt", id, "after recovery"])
        .await;
    case.record(
        "continued.txt",
        tokio::time::timeout(
            DEADLINE,
            completion(&mut daemon.client, id, recovered.revision),
        )
        .await
        .expect("recovered execution"),
    );
    daemon.stop().await;
}
