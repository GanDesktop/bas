//! Session creation, import, lifecycle, and configuration.
use super::*;
use super::{decode, status::metadata_error};
use crate::{agent::AgentSpec, session::CreateRequest};
use bas_protocol::{acp, model::Owner};
use std::path::PathBuf;

impl Service {
    pub(super) async fn handle_create_session(
        &self,
        request: Request<pb::CreateSessionRequest>,
    ) -> Result<Response<pb::CreateSessionResponse>, Status> {
        let request = request.into_inner();
        let current = Owner::current();
        let agent = if request.agent.is_empty() {
            AgentSpec::DEFAULT_ID.into()
        } else {
            request.agent.into()
        };

        let session = self
            .registry
            .create(CreateRequest {
                workspace: request
                    .workspace_id
                    .map(|id| id.parse())
                    .transpose()
                    .map_err(|error: bas_protocol::model::ParseIdError| {
                        Status::invalid_argument(error.to_string())
                    })?,
                agent,
                owner: Owner {
                    user: request.owner_user.unwrap_or(current.user),
                    host: request.owner_host.unwrap_or(current.host),
                },
                cwd: request.cwd.map(PathBuf::from),
                title: request.title,
            })
            .await?;

        Ok(Response::new(pb::CreateSessionResponse {
            session: Some(pb::Session::try_from(&session.snapshot()).map_err(metadata_error)?),
        }))
    }

    pub(super) async fn handle_list_agent_sessions(
        &self,
        request: Request<pb::ListAgentSessionsRequest>,
    ) -> Result<Response<pb::ListAgentSessionsResponse>, Status> {
        let request = request.into_inner();
        let cwd = request.cwd.map(PathBuf::from);
        if cwd.as_ref().is_some_and(|cwd| !cwd.is_absolute()) {
            return Err(Status::invalid_argument(
                "Agent history cwd must be absolute",
            ));
        }
        let agent: bas_protocol::model::AgentId = request.agent.into();
        let listing = self
            .registry
            .list_agent_sessions(agent.clone(), cwd)
            .await?;
        Ok(Response::new(pb::ListAgentSessionsResponse {
            sessions: Some((&listing.sessions).into()),
            initialization: Some((&listing.initialization).into()),
            imported_sessions: self.registry.imported_sessions(&agent).await,
        }))
    }

    pub(super) async fn handle_import_agent_session(
        &self,
        request: Request<pb::ImportAgentSessionRequest>,
    ) -> Result<Response<pb::CreateSessionResponse>, Status> {
        let request = request.into_inner();
        let info: acp::SessionInfo = request
            .session_info
            .ok_or_else(|| Status::invalid_argument("Agent SessionInfo is required"))?
            .decode()
            .map_err(metadata_error)?;
        if !info.cwd.is_absolute() {
            return Err(Status::invalid_argument("Agent workspace must be absolute"));
        }
        if !info.additional_directories.is_empty() {
            return Err(Status::failed_precondition(
                "Importing additional workspace roots is not supported yet",
            ));
        }
        let session = self
            .registry
            .import_agent_session(
                CreateRequest {
                    workspace: None,
                    agent: request.agent.into(),
                    owner: Owner::current(),
                    cwd: Some(info.cwd),
                    title: info.title,
                },
                info.session_id.to_string(),
            )
            .await?;
        Ok(Response::new(pb::CreateSessionResponse {
            session: Some(pb::Session::try_from(&session.snapshot()).map_err(metadata_error)?),
        }))
    }

    pub(super) async fn handle_delete_agent_session(
        &self,
        request: Request<pb::DeleteAgentSessionRequest>,
    ) -> Result<Response<pb::DeleteSessionResponse>, Status> {
        let request = request.into_inner();
        if request.session_id.is_empty() {
            return Err(Status::invalid_argument("Agent session ID is required"));
        }
        self.registry
            .delete_agent_session(request.agent.into(), request.session_id)
            .await?;
        Ok(Response::new(pb::DeleteSessionResponse {}))
    }

    pub(super) async fn handle_close_session(
        &self,
        request: Request<pb::GetSessionRequest>,
    ) -> Result<Response<pb::GetSessionResponse>, Status> {
        let session = self.session(&request.into_inner().session).await?;
        if session.meta().status != bas_protocol::model::SessionStatus::Stopped {
            session.close_backend().await?;
            self.registry.permissions().close(session.id());
            self.registry.elicitations().close(session.id());
        }
        Ok(Response::new(pb::GetSessionResponse {
            session: Some(pb::Session::try_from(&session.snapshot()).map_err(metadata_error)?),
        }))
    }

    pub(super) async fn handle_restart_session(
        &self,
        request: Request<pb::GetSessionRequest>,
    ) -> Result<Response<pb::GetSessionResponse>, Status> {
        let session = self.session(&request.into_inner().session).await?;
        self.registry.restart_session(&session).await?;
        Ok(Response::new(pb::GetSessionResponse {
            session: Some(pb::Session::try_from(&session.snapshot()).map_err(metadata_error)?),
        }))
    }

    pub(super) async fn handle_list_sessions(
        &self,
        request: Request<pb::ListSessionsRequest>,
    ) -> Result<Response<pb::ListSessionsResponse>, Status> {
        let include_archived = request.into_inner().include_archived;
        let sessions = self
            .registry
            .snapshots()
            .await
            .iter()
            .filter(|session| include_archived || !session.meta.archived)
            .map(pb::Session::try_from)
            .collect::<Result<_, _>>()
            .map_err(metadata_error)?;

        Ok(Response::new(pb::ListSessionsResponse { sessions }))
    }

    pub(super) async fn handle_get_session(
        &self,
        request: Request<pb::GetSessionRequest>,
    ) -> Result<Response<pb::GetSessionResponse>, Status> {
        let session = self.session(&request.into_inner().session).await?;

        Ok(Response::new(pb::GetSessionResponse {
            session: Some(pb::Session::try_from(&session.snapshot()).map_err(metadata_error)?),
        }))
    }

    pub(super) async fn handle_delete_session(
        &self,
        request: Request<pb::DeleteSessionRequest>,
    ) -> Result<Response<pb::DeleteSessionResponse>, Status> {
        let id = request.into_inner().session;
        let id = SessionId::from_str(&id)
            .map_err(|error| Status::invalid_argument(error.to_string()))?;

        self.registry.remove(id).await?;

        Ok(Response::new(pb::DeleteSessionResponse {}))
    }

    pub(super) async fn handle_set_config_option(
        &self,
        request: Request<pb::AcpPayload>,
    ) -> Result<Response<pb::AcpPayload>, Status> {
        let request: acp::SetSessionConfigOptionRequest = decode(request)?;
        let session = self.session(&request.session_id.to_string()).await?;
        let _backend = self.registry.use_backend(&session).await?;
        let response = session.set_config_option(request).await?;
        Ok(Response::new((&response).into()))
    }

    pub(super) async fn handle_set_session_directories(
        &self,
        request: Request<pb::SetSessionDirectoriesRequest>,
    ) -> Result<Response<pb::GetSessionResponse>, Status> {
        let request = request.into_inner();
        let session = self.session(&request.session).await?;
        let mut directories = Vec::new();
        for path in request.additional_directories {
            let path = PathBuf::from(path);
            if !path.is_absolute() {
                return Err(Status::invalid_argument(
                    "additional directories must be absolute",
                ));
            }
            let path = tokio::fs::canonicalize(path)
                .await
                .map_err(|e| Status::invalid_argument(e.to_string()))?;
            bas_protocol::protocol::utf8_cwd(&path)
                .map_err(|e| Status::invalid_argument(e.to_string()))?;
            if !tokio::fs::metadata(&path)
                .await
                .map_err(|e| Status::invalid_argument(e.to_string()))?
                .is_dir()
            {
                return Err(Status::invalid_argument(
                    "additional workspace must be a directory",
                ));
            }
            if session.meta().cwd.as_ref() != Some(&path) && !directories.contains(&path) {
                directories.push(path);
            }
        }
        let _backend = self.registry.use_backend(&session).await?;
        let snapshot = session
            .set_directories(directories, self.registry.startup_timeout())
            .await?;
        Ok(Response::new(pb::GetSessionResponse {
            session: Some(pb::Session::try_from(&snapshot).map_err(metadata_error)?),
        }))
    }

    pub(super) async fn handle_set_mode(
        &self,
        request: Request<pb::AcpPayload>,
    ) -> Result<Response<pb::AcpPayload>, Status> {
        let request: acp::SetSessionModeRequest = decode(request)?;
        let session = self.session(&request.session_id.to_string()).await?;
        let _backend = self.registry.use_backend(&session).await?;
        let response = session.set_mode(request).await?;
        Ok(Response::new((&response).into()))
    }

    pub(super) async fn handle_set_archived(
        &self,
        request: Request<pb::SetArchivedRequest>,
    ) -> Result<Response<pb::SetArchivedResponse>, Status> {
        let request = request.into_inner();
        let session = self.session(&request.session).await?;
        let snapshot = self
            .registry
            .set_archived(&session, request.archived)
            .await?;
        Ok(Response::new(pb::SetArchivedResponse {
            session: Some(pb::Session::try_from(&snapshot).map_err(metadata_error)?),
        }))
    }

    pub(super) async fn handle_set_title(
        &self,
        request: Request<pb::SetTitleRequest>,
    ) -> Result<Response<pb::SetTitleResponse>, Status> {
        let request = request.into_inner();
        let session = self.session(&request.session).await?;
        session.set_title(&request.title).await?;

        Ok(Response::new(pb::SetTitleResponse {}))
    }
}
