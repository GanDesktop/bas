use super::*;
use crate::storage::{LocalDb, Storage};

async fn workspace(service: &Service, directory: &std::path::Path) -> pb::Workspace {
    service
        .create_workspace(Request::new(pb::CreateWorkspaceRequest {
            cwd: directory.to_str().expect("path").into(),
            name: None,
        }))
        .await
        .expect("workspace")
        .into_inner()
}

#[tokio::test]
async fn workspace_archive_and_force_delete_include_archived_sessions() {
    let directory = tempfile::tempdir().expect("project");
    let other_directory = tempfile::tempdir().expect("other project");
    let marker = directory.path().join("keep.txt");
    std::fs::write(&marker, "project file").expect("file");
    let registry = registry(100);
    let service = Service::new(Arc::clone(&registry));
    let project = workspace(&service, directory.path()).await;
    assert_eq!(project.id, workspace(&service, directory.path()).await.id);
    let other = workspace(&service, other_directory.path()).await;
    let mut ids = Vec::new();
    for _ in 0..2 {
        let session = service
            .create_session(Request::new(pb::CreateSessionRequest {
                workspace_id: Some(project.id.clone()),
                ..Default::default()
            }))
            .await
            .expect("session")
            .into_inner()
            .session
            .expect("snapshot");
        assert_eq!(session.workspace_id, project.id);
        ids.push(session.id);
    }
    let rejected = service
        .delete_workspace(Request::new(pb::DeleteWorkspaceRequest {
            workspace_id: project.id.clone(),
            ..Default::default()
        }))
        .await
        .expect_err("nonempty workspace");
    assert_eq!(rejected.code(), Code::FailedPrecondition);
    assert_eq!(registry.snapshots().await.len(), 2);
    service
        .set_workspace_archived(Request::new(pb::SetWorkspaceArchivedRequest {
            workspace_id: project.id.clone(),
            archived: true,
        }))
        .await
        .expect("cascade archive");
    assert!(
        registry
            .snapshots()
            .await
            .iter()
            .all(|session| session.meta.archived
                && session.meta.status == bas::model::SessionStatus::Stopped)
    );
    assert_eq!(
        service
            .create_session(Request::new(pb::CreateSessionRequest {
                workspace_id: Some(project.id.clone()),
                ..Default::default()
            }))
            .await
            .expect_err("archived parent")
            .code(),
        Code::FailedPrecondition
    );
    assert_eq!(
        service
            .set_archived(Request::new(pb::SetArchivedRequest {
                session: ids[0].clone(),
                archived: false,
            }))
            .await
            .expect_err("restore parent first")
            .code(),
        Code::FailedPrecondition
    );
    assert_eq!(
        service
            .delete_workspace(Request::new(pb::DeleteWorkspaceRequest {
                workspace_id: project.id.clone(),
                force: false,
            }))
            .await
            .expect_err("archived sessions still count")
            .code(),
        Code::FailedPrecondition
    );
    service
        .set_workspace_archived(Request::new(pb::SetWorkspaceArchivedRequest {
            workspace_id: project.id.clone(),
            archived: false,
        }))
        .await
        .expect("restore project");
    assert!(
        registry
            .snapshots()
            .await
            .iter()
            .all(|session| session.meta.archived)
    );
    service
        .delete_workspace(Request::new(pb::DeleteWorkspaceRequest {
            workspace_id: project.id.clone(),
            force: true,
        }))
        .await
        .expect("cascade delete");
    assert!(registry.snapshots().await.is_empty());
    assert_eq!(registry.workspaces().await.len(), 1);
    assert_eq!(registry.workspaces().await[0].id.to_string(), other.id);
    assert_eq!(
        std::fs::read_to_string(marker).expect("project survives"),
        "project file"
    );
    service
        .delete_workspace(Request::new(pb::DeleteWorkspaceRequest {
            workspace_id: other.id,
            force: false,
        }))
        .await
        .expect("empty workspace needs no force");
}

#[tokio::test]
async fn empty_workspace_and_session_membership_survive_restart_and_delete() {
    let directory = tempfile::tempdir().expect("directory");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(directory.path(), std::fs::Permissions::from_mode(0o700))
            .expect("private directory");
    }
    let database = directory.path().join("bas.sqlite");
    let storage = Arc::new(LocalDb::open(&database).await.expect("store"));
    let registry = Arc::new(Registry::with_storage(
        Arc::new(Catalog::with_defaults()),
        Arc::new(LocalHost::default()),
        Limits::default(),
        storage.clone(),
    ));
    let service = Service::new(registry.clone());
    let project = workspace(&service, directory.path()).await;
    let mut recovered = Registry::with_storage(
        Arc::new(Catalog::with_defaults()),
        Arc::new(LocalHost::default()),
        Limits::default(),
        storage.clone(),
    );
    recovered.recover().await.expect("recover empty workspace");
    assert_eq!(recovered.workspaces().await[0].id.to_string(), project.id);
    let session = service
        .create_session(Request::new(pb::CreateSessionRequest {
            workspace_id: Some(project.id.clone()),
            ..Default::default()
        }))
        .await
        .expect("create")
        .into_inner()
        .session
        .expect("snapshot");
    registry.shutdown().await;
    let mut recovered = Registry::with_storage(
        Arc::new(Catalog::with_defaults()),
        Arc::new(LocalHost::default()),
        Limits::default(),
        storage.clone(),
    );
    recovered.recover().await.expect("recover sessions");
    let recovered = Arc::new(recovered);
    assert_eq!(
        recovered.snapshots().await[0].meta.workspace.to_string(),
        project.id
    );
    recovered
        .delete_workspace(project.id.parse().expect("id"), true)
        .await
        .expect("delete");
    assert!(
        storage
            .load(session.id.parse().expect("id"))
            .await
            .expect("load")
            .is_none()
    );
    drop(service);
    drop(registry);
    drop(recovered);
    drop(storage);
    let storage = LocalDb::open(&database).await.expect("reopen");
    assert!(storage.workspaces().await.expect("workspaces").is_empty());
    assert!(storage.list().await.expect("sessions").is_empty());
}
