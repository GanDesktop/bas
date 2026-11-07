use super::support::Case;
use bas_client::service::pb;

#[tokio::test]
async fn workspace_archive_and_deletion_preserve_user_files() {
    let case = Case::new("workspace");
    let marker = case.root.join("workspace/keep.txt");
    std::fs::write(&marker, "user content").expect("workspace file");
    let mut daemon = case.start("daemon", &[]).await;
    let project = daemon
        .client
        .create_workspace(pb::CreateWorkspaceRequest {
            cwd: case.root.join("workspace").to_str().expect("path").into(),
            name: None,
        })
        .await
        .expect("workspace")
        .into_inner();
    let session = daemon
        .client
        .create_session(pb::CreateSessionRequest {
            workspace_id: Some(project.id.clone()),
            ..Default::default()
        })
        .await
        .expect("session in workspace")
        .into_inner()
        .session
        .expect("session");
    let error = daemon
        .client
        .delete_workspace(pb::DeleteWorkspaceRequest {
            workspace_id: project.id.clone(),
            force: false,
        })
        .await
        .expect_err("nonempty workspace");
    assert_eq!(error.code(), tonic::Code::FailedPrecondition);
    daemon
        .client
        .set_workspace_archived(pb::SetWorkspaceArchivedRequest {
            workspace_id: project.id.clone(),
            archived: true,
        })
        .await
        .expect("archive workspace");
    let archived = daemon
        .client
        .get_session(pb::GetSessionRequest {
            session: session.id.clone(),
        })
        .await
        .expect("archived session")
        .into_inner()
        .session
        .expect("session");
    case.record("archived.txt", &archived);
    assert!(archived.archived);
    let error = daemon
        .client
        .create_session(pb::CreateSessionRequest {
            workspace_id: Some(project.id.clone()),
            ..Default::default()
        })
        .await
        .expect_err("archived workspace");
    assert_eq!(error.code(), tonic::Code::FailedPrecondition);
    daemon
        .client
        .delete_workspace(pb::DeleteWorkspaceRequest {
            workspace_id: project.id,
            force: true,
        })
        .await
        .expect("delete workspace and sessions");
    assert_eq!(
        daemon
            .client
            .get_session(pb::GetSessionRequest {
                session: session.id
            })
            .await
            .expect_err("session deleted")
            .code(),
        tonic::Code::NotFound
    );
    assert_eq!(
        std::fs::read_to_string(marker).expect("user file survives"),
        "user content"
    );
    daemon.stop().await;
}
