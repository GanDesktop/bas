use super::*;
use bas_protocol::model::Timestamp;

impl Registry {
    pub async fn workspaces(&self) -> Vec<Workspace> {
        self.workspaces.read().await.values().cloned().collect()
    }

    /// # Errors
    /// Returns NotFound for an unknown or deleted workspace.
    pub async fn workspace(&self, id: WorkspaceId) -> Result<Workspace, CreateError> {
        self.workspaces
            .read()
            .await
            .get(&id)
            .cloned()
            .ok_or(CreateError::WorkspaceNotFound(id))
    }

    pub(super) async fn require_active_workspace(
        &self,
        id: WorkspaceId,
    ) -> Result<(), CreateError> {
        if self.workspace(id).await?.archived {
            return Err(CreateError::WorkspaceArchived(id));
        }
        Ok(())
    }

    /// Creating an already registered directory returns its workspace, including its archived state.
    /// # Errors
    /// Rejects invalid paths or names and propagates storage failures.
    pub async fn create_workspace(
        &self,
        cwd: PathBuf,
        name: Option<String>,
    ) -> Result<Workspace, CreateError> {
        let _gate = self.workspace_gate.read().await;
        self.workspace_for_directory(cwd, name).await
    }

    pub(super) async fn workspace_for_directory(
        &self,
        cwd: PathBuf,
        name: Option<String>,
    ) -> Result<Workspace, CreateError> {
        let cwd = tokio::fs::canonicalize(cwd)
            .await
            .map_err(CreateError::Cwd)?;
        bas_protocol::protocol::utf8_cwd(&cwd)
            .map_err(|_| CreateError::InvalidWorkspace("path must be UTF-8"))?;
        if !tokio::fs::metadata(&cwd)
            .await
            .map_err(CreateError::Cwd)?
            .is_dir()
        {
            return Err(CreateError::InvalidWorkspace("path must be a directory"));
        }
        let name = name.unwrap_or_else(|| {
            cwd.file_name()
                .unwrap_or(cwd.as_os_str())
                .to_string_lossy()
                .into_owned()
        });
        if name.trim().is_empty() {
            return Err(CreateError::InvalidWorkspace("name is empty"));
        }
        let workspaces = Arc::clone(&self.workspaces);
        let creation = Arc::clone(&self.workspace_creation);
        let storage = self.storage.clone();
        // A disconnected caller must not leave a committed workspace unpublished.
        tokio::spawn(async move {
            let _creation = creation.lock().await;
            if let Some(workspace) = workspaces
                .read()
                .await
                .values()
                .find(|workspace| workspace.cwd == cwd)
            {
                return Ok(workspace.clone());
            }
            let now = Timestamp::now();
            let workspace = Workspace {
                id: WorkspaceId::new(),
                name: name.trim().into(),
                cwd,
                archived: false,
                revision: 1,
                created_at: now,
                updated_at: now,
            };
            if let Some(storage) = storage {
                storage.save_workspace(workspace.clone()).await?;
            }
            workspaces
                .write()
                .await
                .insert(workspace.id, workspace.clone());
            Ok(workspace)
        })
        .await
        .expect("workspace creation task panicked")
    }

    async fn store_workspace(&self, workspace: Workspace) -> Result<(), CreateError> {
        if let Some(storage) = &self.storage {
            storage.save_workspace(workspace.clone()).await?;
        }
        self.workspaces
            .write()
            .await
            .insert(workspace.id, workspace);
        Ok(())
    }

    /// # Errors
    /// Propagates any session archival or persistence failure. The workspace is
    /// marked archived only after every session succeeds; partial progress is retryable.
    pub async fn set_workspace_archived(
        self: &Arc<Self>,
        id: WorkspaceId,
        archived: bool,
    ) -> Result<Workspace, CreateError> {
        let registry = Arc::clone(self);
        tokio::spawn(async move {
            let _gate = registry.workspace_gate.write().await;
            let mut workspace = registry.workspace(id).await?;
            if archived {
                let sessions: Vec<_> = registry
                    .sessions
                    .read()
                    .await
                    .values()
                    .filter(|session| session.meta().workspace == id)
                    .cloned()
                    .collect();
                for session in sessions {
                    registry.set_archived_locked(&session, true).await?;
                }
            }
            workspace.archived = archived;
            workspace.revision += 1;
            workspace.updated_at = Timestamp::now();
            registry.store_workspace(workspace.clone()).await?;
            Ok(workspace)
        })
        .await
        .expect("workspace archival task panicked")
    }

    /// # Errors
    /// Without force, returns WorkspaceNotEmpty even when all sessions are archived.
    /// A failed cascade retains the workspace so callers can inspect and retry it.
    pub async fn delete_workspace(
        self: &Arc<Self>,
        id: WorkspaceId,
        force: bool,
    ) -> Result<(), CreateError> {
        let registry = Arc::clone(self);
        tokio::spawn(async move {
            let _gate = registry.workspace_gate.write().await;
            registry.workspace(id).await?;
            let sessions: Vec<_> = registry
                .sessions
                .read()
                .await
                .values()
                .filter(|session| session.meta().workspace == id)
                .map(|session| session.id())
                .collect();
            if !force && !sessions.is_empty() {
                return Err(CreateError::WorkspaceNotEmpty(id));
            }
            for session in sessions {
                registry.remove_locked(session).await?;
            }
            if let Some(storage) = &registry.storage {
                storage.delete_workspace(id).await?;
            }
            registry.workspaces.write().await.remove(&id);
            Ok(())
        })
        .await
        .expect("workspace deletion task panicked")
    }
}
