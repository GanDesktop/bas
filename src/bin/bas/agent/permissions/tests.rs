use super::*;
use crate::agent::{LocalHost, PermissionPolicy};

fn request(session: SessionId) -> acp::RequestPermissionRequest {
    acp::RequestPermissionRequest::new(
        session.to_string(),
        acp::ToolCallUpdate::new(
            "tool-1",
            acp::ToolCallUpdateFields::new().title("run a command"),
        ),
        vec![
            acp::PermissionOption::new(
                "allow-once",
                "Allow once",
                acp::PermissionOptionKind::AllowOnce,
            ),
            acp::PermissionOption::new(
                "reject-once",
                "Reject once",
                acp::PermissionOptionKind::RejectOnce,
            ),
        ],
    )
}

fn selected(id: &str) -> acp::RequestPermissionOutcome {
    acp::RequestPermissionOutcome::Selected(acp::SelectedPermissionOutcome::new(id.to_owned()))
}

async fn next_permission(subscription: &mut PermissionSubscription) -> PendingPermission {
    tokio::time::timeout(
        std::time::Duration::from_secs(2),
        subscription.snapshots.changed(),
    )
    .await
    .expect("approval snapshot arrives")
    .expect("subscription stays open");
    subscription
        .snapshots
        .borrow_and_update()
        .first()
        .expect("pending approval")
        .clone()
}

#[tokio::test]
async fn attached_client_must_answer_even_with_allow_policy() {
    let permissions = Arc::new(Permissions::default());
    let session = SessionId::new();
    let mut subscription = permissions.subscribe(session);
    let host = permissions.host(Arc::new(LocalHost::new(PermissionPolicy::Allow)));
    let answer = tokio::spawn(async move { host.request_permission(request(session)).await });
    let pending = next_permission(&mut subscription).await;
    assert!(
        !answer.is_finished(),
        "interactive requests wait for the user"
    );
    permissions
        .respond(session, &pending.request_id, selected("reject-once"))
        .expect("valid choice");
    assert_eq!(
        answer
            .await
            .expect("request task")
            .expect("approval outcome"),
        selected("reject-once")
    );
    assert!(subscription.snapshots.borrow().is_empty());
    assert!(matches!(
        permissions.respond(session, &pending.request_id, selected("allow-once")),
        Err(PermissionError::Expired)
    ));
}

#[tokio::test]
async fn invalid_options_and_wrong_sessions_preserve_pending_request() {
    let permissions = Arc::new(Permissions::default());
    let session = SessionId::new();
    let mut subscription = permissions.subscribe(session);
    let host = permissions.host(Arc::new(LocalHost::default()));
    let answer = tokio::spawn(async move { host.request_permission(request(session)).await });
    let pending = next_permission(&mut subscription).await;
    assert!(matches!(
        permissions.respond(session, &pending.request_id, selected("invented-option")),
        Err(PermissionError::InvalidOption)
    ));
    assert!(matches!(
        permissions.respond(
            SessionId::new(),
            &pending.request_id,
            selected("allow-once")
        ),
        Err(PermissionError::Expired)
    ));
    assert_eq!(subscription.snapshots.borrow().len(), 1);
    permissions
        .respond(session, &pending.request_id, selected("allow-once"))
        .expect("offered option remains answerable");
    assert_eq!(
        answer
            .await
            .expect("request task")
            .expect("approval outcome"),
        selected("allow-once")
    );
}

#[tokio::test]
async fn disconnected_approvers_leave_requests_pending_for_reconnection() {
    let permissions = Arc::new(Permissions::default());
    let session = SessionId::new();
    let mut first = permissions.subscribe(session);
    let second = permissions.subscribe(session);
    let host = permissions.host(Arc::new(LocalHost::new(PermissionPolicy::Allow)));
    let answer = tokio::spawn(async move { host.request_permission(request(session)).await });
    let pending = next_permission(&mut first).await;
    drop(first);
    drop(second);
    assert!(!answer.is_finished());
    let reconnected = permissions.subscribe(session);
    assert_eq!(
        reconnected.snapshots.borrow()[0].request_id,
        pending.request_id
    );
    permissions
        .respond(session, &pending.request_id, selected("allow-once"))
        .expect("reply after reconnect");
    assert_eq!(
        answer.await.expect("task").expect("answer"),
        selected("allow-once")
    );
    drop(reconnected);
    let host = permissions.host(Arc::new(LocalHost::default()));
    let answer = tokio::spawn(async move { host.request_permission(request(session)).await });
    let mut reconnected = permissions.subscribe(session);
    let pending = next_permission(&mut reconnected).await;
    permissions
        .respond(session, &pending.request_id, selected("reject-once"))
        .expect("new request");
    assert_eq!(
        answer.await.expect("task").expect("answer"),
        selected("reject-once")
    );
}

#[tokio::test]
async fn closed_subscription_cannot_detach_a_newer_channel() {
    let permissions = Arc::new(Permissions::default());
    let session = SessionId::new();
    let old = permissions.subscribe(session);
    permissions.close(session);
    let mut current = permissions.subscribe(session);
    drop(old);
    assert_eq!(
        permissions
            .lock()
            .get(&session)
            .expect("current subscription")
            .viewers,
        1
    );
    let host = permissions.host(Arc::new(LocalHost::default()));
    let answer = tokio::spawn(async move { host.request_permission(request(session)).await });
    let pending = next_permission(&mut current).await;
    permissions
        .respond(session, &pending.request_id, selected("allow-once"))
        .expect("current viewer can answer");
    assert_eq!(
        answer
            .await
            .expect("request task")
            .expect("approval outcome"),
        selected("allow-once")
    );
}
